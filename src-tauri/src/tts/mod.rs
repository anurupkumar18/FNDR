//! FNDR's on-device neural voice (ADR 030): Kokoro-82M run through `ort` on
//! the CPU, with a Rust port of misaki's English pronunciation step and no
//! espeak. Text goes `normalize` -> `g2p` -> `vocab` -> `engine`; nothing
//! leaves the Mac. The files come from `install`, after consent.
//!
//! One engine for the app, loaded on first use and dropped after
//! `IDLE_UNLOAD` without speech. Synthesis is serialized by its own lock,
//! not `AppState::model_pipeline_lock`: a spoken line must not wait behind
//! a screenshot's language-model call. `cancel` stops a long text between
//! its model-sized pieces.

pub mod engine;
pub mod g2p;
pub mod install;
pub mod lexicon;
pub mod lts;
pub mod normalize;
pub mod vocab;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use serde::Serialize;

use engine::{Engine, VoicePack};
use g2p::G2p;
use lexicon::Lexicon;

const IDLE_UNLOAD: Duration = Duration::from_secs(300);

struct Loaded {
    dir: PathBuf,
    engine: Engine,
    us: Option<G2p>,
    gb: Option<G2p>,
    voices: HashMap<String, VoicePack>,
    last_used: Instant,
}

static RUNTIME: Lazy<Mutex<Option<Loaded>>> = Lazy::new(|| Mutex::new(None));
static GENERATION: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsStatus {
    /// `not_installed`, `downloading`, `ready` or `error`.
    pub state: &'static str,
    pub detail: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub loaded: bool,
}

#[derive(Debug, Clone)]
pub struct Synthesis {
    pub samples: Vec<f32>,
    pub duration_ms: u64,
}

pub fn status(app_data_dir: &Path) -> TtsStatus {
    let progress = install::progress();
    let total = install::total_bytes();
    let loaded = RUNTIME.lock().map(|r| r.is_some()).unwrap_or(false);
    let installed = install::is_installed(&install::model_dir(app_data_dir));
    let (state, detail) = match (&progress.phase, installed) {
        (install::Phase::Downloading, _) => ("downloading", None),
        (_, true) => ("ready", None),
        (install::Phase::Failed(err), false) => ("error", Some(err.clone())),
        (install::Phase::Idle, false) => ("not_installed", None),
    };
    TtsStatus {
        state,
        detail,
        downloaded_bytes: if installed {
            total
        } else {
            progress.downloaded
        },
        total_bytes: total,
        loaded,
    }
}

/// Stops a synthesis in progress at its next piece.
pub fn cancel() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Drops the loaded model (before its files are removed).
pub fn unload() {
    if let Ok(mut runtime) = RUNTIME.lock() {
        *runtime = None;
    }
}

fn threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(1, 4))
        .unwrap_or(2)
}

fn load_g2p(dir: &Path, british: bool) -> Result<G2p, String> {
    let prefix = if british { "gb" } else { "us" };
    let read = |name: &str| {
        let path = dir.join(format!("{prefix}_{name}.json"));
        std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))
    };
    Ok(G2p::new(Lexicon::from_json(
        british,
        &read("gold")?,
        &read("silver")?,
    )?))
}

fn watch_idle() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(30));
        let Ok(mut runtime) = RUNTIME.lock() else {
            return;
        };
        match runtime.as_ref() {
            Some(loaded) if loaded.last_used.elapsed() >= IDLE_UNLOAD => {
                *runtime = None;
                tracing::info!(
                    "tts: unloaded the on-device voice after {}s idle",
                    IDLE_UNLOAD.as_secs()
                );
                return;
            }
            Some(_) => {}
            None => return,
        }
    });
}

fn ensure_loaded<'a>(slot: &'a mut Option<Loaded>, dir: &Path) -> Result<&'a mut Loaded, String> {
    if slot.as_ref().is_some_and(|l| l.dir != dir) {
        *slot = None;
    }
    if slot.is_none() {
        if !install::is_installed(dir) {
            return Err("The on-device voice is not installed.".to_string());
        }
        let started = Instant::now();
        let engine = Engine::load(&dir.join(install::MODEL_FILE), threads())?;
        tracing::info!("tts: loaded Kokoro in {} ms", started.elapsed().as_millis());
        *slot = Some(Loaded {
            dir: dir.to_path_buf(),
            engine,
            us: None,
            gb: None,
            voices: HashMap::new(),
            last_used: Instant::now(),
        });
        watch_idle();
    }
    slot.as_mut()
        .ok_or_else(|| "the on-device voice did not load".to_string())
}

/// Speech for `text` in `voice` (a Kokoro voice id; empty or unknown means
/// the default) at `speed` (1 is natural). Blocking: call it off the async
/// runtime.
pub fn synthesize(
    app_data_dir: &Path,
    text: &str,
    voice: &str,
    speed: f32,
) -> Result<Synthesis, String> {
    let generation = GENERATION.load(Ordering::SeqCst);
    let voice = install::voice_info(voice)
        .map(|v| v.id)
        .unwrap_or(install::DEFAULT_VOICE);
    let british = install::voice_info(voice).is_some_and(|v| v.accent == "gb");
    let dir = install::model_dir(app_data_dir);
    let mut runtime = RUNTIME
        .lock()
        .map_err(|_| "the on-device voice is unavailable".to_string())?;
    let loaded = ensure_loaded(&mut runtime, &dir)?;
    loaded.last_used = Instant::now();
    let g2p_slot = if british {
        &mut loaded.gb
    } else {
        &mut loaded.us
    };
    if g2p_slot.is_none() {
        *g2p_slot = Some(load_g2p(&dir, british)?);
    }
    let phonemes = g2p_slot
        .as_ref()
        .map(|g| g.phonemize(&normalize::normalize(text)))
        .unwrap_or_default();
    if !loaded.voices.contains_key(voice) {
        let pack = VoicePack::load(&install::voice_path(&dir, voice))?;
        loaded.voices.insert(voice.to_string(), pack);
    }
    let pack = loaded
        .voices
        .get(voice)
        .cloned()
        .ok_or("voice did not load")?;
    let mut samples = Vec::new();
    for piece in vocab::split_for_model(&phonemes) {
        if GENERATION.load(Ordering::SeqCst) != generation {
            return Err("cancelled".to_string());
        }
        let ids = vocab::encode(&piece);
        samples.extend(loaded.engine.synthesize(&ids, &pack, speed)?);
    }
    loaded.last_used = Instant::now();
    let duration_ms = engine::duration_ms(samples.len());
    Ok(Synthesis {
        samples,
        duration_ms,
    })
}

/// The pronunciation FNDR would give `text`, for tests and diagnostics.
pub fn phonemes_for(dir: &Path, text: &str, british: bool) -> Result<String, String> {
    Ok(load_g2p(dir, british)?.phonemize(&normalize::normalize(text)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A profile folder holding a real install: `FNDR_KOKORO_APP_DATA`, or
    /// the cache this module was measured with.
    fn real_app_data() -> PathBuf {
        std::env::var("FNDR_KOKORO_APP_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_default()
                    .join("Library/Caches/fndr-kokoro-spike/appdata")
            })
    }

    fn real_dir() -> Option<PathBuf> {
        let dir = install::model_dir(&real_app_data());
        install::is_installed(&dir).then_some(dir)
    }

    #[tokio::test]
    #[ignore = "downloads about 112 MB from Hugging Face and GitHub"]
    async fn real_install_downloads_and_verifies_every_file() {
        let app_data = real_app_data();
        install::begin(true).unwrap();
        install::run(app_data.clone()).await.unwrap();
        assert!(install::is_installed(&install::model_dir(&app_data)));
        assert_eq!(status(&app_data).state, "ready");
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    }

    #[test]
    fn status_reports_not_installed_for_an_empty_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let status = status(tmp.path());
        assert!(matches!(
            status.state,
            "not_installed" | "downloading" | "error"
        ));
        assert_eq!(status.total_bytes, install::total_bytes());
        assert!(synthesize(tmp.path(), "Hello.", "af_heart", 1.0).is_err());
    }

    /// Golden lines from misaki 0.9.4 run with spaCy `en_core_web_sm`
    /// (`fallback=None`), checked against the full dictionaries.
    const GOLDEN_US: &[(&str, &str)] = &[
        (
            "Life is like a box of chocolates. You never know what you're gonna get.",
            "lˈIf ɪz lˈIk ɐ bˈɑks ʌv ʧˈɑkᵊləts. jˌu nˈɛvəɹ nˈO wˌʌt jʊɹ ɡˈʌnə ɡˈɛt.",
        ),
        (
            "How could I know? It's an unanswerable question.",
            "hˌW kʊd ˌI nˈO? ˌɪts ɐn ˌʌnˈænsəɹəbᵊl kwˈɛsʧᵊn.",
        ),
        (
            "The quick brown fox jumps over the lazy dog.",
            "ðə kwˈɪk bɹˈWn fˈɑks ʤˈʌmps ˈOvəɹ ðə lˈAzi dˈɔɡ.",
        ),
        (
            "I'll read the book that she read yesterday.",
            "ˌIl ɹˈid ðə bˈʊk ðæt ʃi ɹˈɛd jˈɛstəɹdˌA.",
        ),
        (
            "The children's toys were scattered across James' room.",
            "ðə ʧˈɪldɹᵊnz tˈYz wɜɹ skˈæTəɹd əkɹˈɔs ʤˈAmz ɹˈum.",
        ),
        (
            "She's got a well-known, state-of-the-art system.",
            "ʃˌiz ɡɑt ɐ wˌɛlnˈOn, stˈAtʌvðiˈɑɹt sˈɪstəm.",
        ),
        (
            "We haven't seen them, and they don't care.",
            "wˌi hˈævᵊnt sˈin ðˌɛm, ænd ðA dˈOnt kˈɛɹ.",
        ),
        ("Hello there.", "həlˈO ðˈɛɹ."),
        (
            "The cats walked, talked, and kept jumping.",
            "ðə kˈæts wˈɔkt, tˈɔkt, ænd kˈɛpt ʤˈʌmpɪŋ.",
        ),
        ("He wanted to eat an apple.", "hˌi wˈɑntᵻd tʊ ˈit ɐn ˈæpᵊl."),
        ("Please record the record.", "plˈiz ɹəkˈɔɹd ðə ɹˈɛkəɹd."),
        (
            "I live here, and live music plays.",
            "ˌI lˈɪv hˈɪɹ, ænd lˈIv mjˈuzɪk plˈAz.",
        ),
        ("Where were you on Tuesday?", "wˌɛɹ wɜɹ ju ˌɔn tˈuzdˌA?"),
        (
            "The leaves fall while she leaves.",
            "ðə lˈivz fˈɔl wˌIl ʃi lˈivz.",
        ),
        ("Thanks for waiting.", "θˈæŋks fɔɹ wˈATɪŋ."),
    ];

    const GOLDEN_GB: &[(&str, &str)] = &[
        (
            "Life is like a box of chocolates. You never know what you're gonna get.",
            "lˈIf ɪz lˈIk ɐ bˈɒks ɒv ʧˈɒkᵊləts. jˌuː nˈɛvə nˈQ wˌɒt jɔː ɡˈʌnə ɡˈɛt.",
        ),
        (
            "The quick brown fox jumps over the lazy dog.",
            "ðə kwˈɪk bɹˈWn fˈɒks ʤˈʌmps ˈQvə ðə lˈAzi dˈɒɡ.",
        ),
        ("Please record the record.", "plˈiːz ɹɪkˈɔːd ðə ɹˈɛkɔːd."),
        ("Where were you on Tuesday?", "wˌɛː wɜː juː ˌɒn tjˈuːzdA?"),
    ];

    #[test]
    #[ignore = "needs the downloaded dictionaries (FNDR_KOKORO_APP_DATA)"]
    fn golden_sentences_match_misaki_with_the_full_dictionaries() {
        let Some(dir) = real_dir() else {
            panic!("run real_install_downloads_and_verifies_every_file first");
        };
        let mut misses = Vec::new();
        for (british, golden) in [(false, GOLDEN_US), (true, GOLDEN_GB)] {
            for (text, expected) in golden {
                let got = phonemes_for(&dir, text, british).unwrap();
                if got != *expected {
                    misses.push(format!("{text}\n  want {expected}\n  got  {got}"));
                }
            }
        }
        assert!(misses.is_empty(), "{}", misses.join("\n"));

        let oov = phonemes_for(
            &dir,
            "The flibbertigibbet zorbled the quixotic snarfblat.",
            false,
        )
        .unwrap();
        assert!(oov.starts_with("ðə flˌɪbəɹTiʤˈɪbət "), "{oov}");
        assert!(oov.contains(" ðə kwɪksˈɑTɪk "), "{oov}");
        assert_eq!(oov.split(' ').count(), 5, "{oov}");
        assert!(!oov.contains('❓'));
    }

    #[test]
    #[ignore = "needs the downloaded model (FNDR_KOKORO_APP_DATA); runs Kokoro for real"]
    fn real_model_speaks_a_sentence_with_plausible_audio() {
        let Some(dir) = real_dir() else {
            panic!("run real_install_downloads_and_verifies_every_file first");
        };
        let app_data = real_app_data();
        assert!(dir.starts_with(&app_data));
        let text = "Your meeting with Priya starts in ten minutes.";
        let started = Instant::now();
        let out = synthesize(&app_data, text, "af_heart", 1.0).unwrap();
        let first = started.elapsed();
        assert!(
            out.duration_ms > 1500 && out.duration_ms < 6000,
            "{} ms",
            out.duration_ms
        );
        assert!(
            rms(&out.samples) > 0.01,
            "silent: rms {}",
            rms(&out.samples)
        );
        let peak = out.samples.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak < 1.5, "peak {peak}");
        let again = Instant::now();
        let second = synthesize(&app_data, text, "bf_emma", 1.0).unwrap();
        let warm = again.elapsed();
        assert!(rms(&second.samples) > 0.01);
        eprintln!(
            "first (load + synth) {} ms, warm {} ms, audio {} ms, rtf {:.3}",
            first.as_millis(),
            warm.as_millis(),
            second.duration_ms,
            warm.as_secs_f64() * 1000.0 / second.duration_ms as f64
        );
    }
}
