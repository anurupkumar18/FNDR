#[path = "support/cer.rs"]
mod cer;

use fndr_lib::capture::{dhash_9x8, hamming, is_aba, luma_9x8_from_rgba, PerceptualHasher};
use serde::Deserialize;
use std::collections::VecDeque;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Fixture {
    id: String,
    file: String,
    app_class: String,
    expected_outcome: String,
    expected_text: String,
    cer_budget: f64,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens")
}

fn load_manifest() -> Vec<Fixture> {
    let raw = std::fs::read_to_string(fixtures_dir().join("manifest.json")).expect("manifest.json");
    serde_json::from_str(&raw).expect("manifest parses")
}

#[test]
fn corpus_has_thirty_screens_across_six_classes() {
    let fixtures = load_manifest();
    assert_eq!(fixtures.len(), 30);
    for class in [
        "editor",
        "terminal",
        "browser_article",
        "chat_mock",
        "pdf_paper",
        "privacy_negative",
    ] {
        assert_eq!(
            fixtures.iter().filter(|f| f.app_class == class).count(),
            5,
            "class {class}"
        );
    }
    for f in &fixtures {
        assert!(
            fixtures_dir().join(&f.file).exists(),
            "missing image for {}",
            f.id
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn ocr_plus_cleanup_stays_within_each_fixtures_cer_budget() {
    let mut failures = Vec::new();
    let mut table = Vec::new();
    for f in load_manifest()
        .iter()
        .filter(|f| f.expected_outcome == "store")
    {
        let recognized = recognize_and_clean(
            app_name_for_class(&f.app_class),
            &fixtures_dir().join(&f.file),
        );
        let cer = cer::char_error_rate(&f.expected_text, &recognized);
        table.push(format!(
            "{:<20} class={:<16} cer={:.3} budget={:.3}",
            f.id, f.app_class, cer, f.cer_budget
        ));
        if cer > f.cer_budget {
            failures.push(format!("{} cer {:.3} > {:.3}", f.id, cer, f.cer_budget));
        }
    }
    for line in &table {
        println!("{line}");
    }
    assert!(failures.is_empty(), "over budget: {failures:?}");
}

/// `text_cleanup`'s app-specific rules match on real app names ("chrome",
/// "terminal", "code", ...), not the fixture's generic `app_class` label.
#[cfg(target_os = "macos")]
fn app_name_for_class(app_class: &str) -> &'static str {
    match app_class {
        "editor" => "Visual Studio Code",
        "terminal" => "Terminal",
        "browser_article" => "Google Chrome",
        "chat_mock" => "Slack",
        "pdf_paper" => "Preview",
        _ => "Unknown",
    }
}

/// Same OCR entry point and cleanup the capture loop uses
/// (`ocr.recognize_with_metadata` in `capture/mod.rs`, then
/// `text_cleanup::build_high_signal_text_for_app`).
#[cfg(target_os = "macos")]
fn recognize_and_clean(app_class: &str, path: &std::path::Path) -> String {
    let image_data = std::fs::read(path).expect("read fixture png");
    let engine = fndr_lib::ocr::OcrEngine::new().expect("ocr engine");
    let (ocr_result, _qwen_cleaned) = engine
        .recognize_with_metadata(&image_data)
        .expect("ocr recognize");
    let high_signal = fndr_lib::capture::text_cleanup::build_high_signal_text_for_app(
        app_class,
        &ocr_result.text,
    );
    high_signal.text
}

// ===== CAP-08: dedupe sequences (dHash + A-B-A vs the current img_hash path) =====

const SEQ_WIDTH: usize = 160;
const SEQ_HEIGHT: usize = 120;
const DHASH_THRESHOLD: u32 = 4;
const ABA_THRESHOLD: u32 = 4;
/// `PerceptualHasher::is_duplicate` compares 8x8 average-hash bytes with a
/// sum-of-absolute-differences distance, not a bit count; its scale is not
/// comparable to a Hamming bit count, so it needs its own, separately tuned
/// threshold on the same fixtures.
const IMG_HASH_THRESHOLD: u32 = 12;

struct Sequence {
    name: &'static str,
    frames: Vec<Vec<u8>>,
    expected_keep: usize,
}

/// A textured (not monotonic) background: a pure gradient makes every
/// dhash bit within a row compare consistently one way (each column is
/// simply brighter than the last), so almost all 64 bits come out 0
/// regardless of local overrides. This pattern gives adjacent sampled
/// columns essentially uncorrelated brightness, so overrides produce a
/// realistic mix of bit flips in both directions.
fn solid_gradient(seed: u8) -> Vec<u8> {
    let mut buf = vec![0u8; SEQ_WIDTH * SEQ_HEIGHT * 4];
    for y in 0..SEQ_HEIGHT {
        for x in 0..SEQ_WIDTH {
            let i = (y * SEQ_WIDTH + x) * 4;
            let v = seed.wrapping_add(((x * 37 + y * 91) % 256) as u8);
            buf[i] = v;
            buf[i + 1] = v;
            buf[i + 2] = v;
            buf[i + 3] = 255;
        }
    }
    buf
}

fn set_block(buf: &mut [u8], x0: usize, y0: usize, size: usize, value: u8) {
    for y in y0..(y0 + size).min(SEQ_HEIGHT) {
        for x in x0..(x0 + size).min(SEQ_WIDTH) {
            let i = (y * SEQ_WIDTH + x) * 4;
            buf[i] = value;
            buf[i + 1] = value;
            buf[i + 2] = value;
        }
    }
}

fn idle_sequence() -> Sequence {
    let base = solid_gradient(30);
    Sequence {
        name: "idle",
        frames: (0..10).map(|_| base.clone()).collect(),
        expected_keep: 1,
    }
}

fn cursor_blink_sequence() -> Sequence {
    // The dhash grid samples pixel (0, 0) directly (col 0, row 0). A single
    // small block toggled by a few luma levels there is a real per-frame
    // byte difference, but far under DHASH_THRESHOLD.
    let frames = (0..10)
        .map(|i| {
            let mut frame = solid_gradient(30);
            let value = if i % 2 == 0 { 100 } else { 104 };
            set_block(&mut frame, 0, 0, 2, value);
            frame
        })
        .collect();
    Sequence {
        name: "cursor_blink",
        frames,
        expected_keep: 1,
    }
}

fn clock_tick_sequence() -> Sequence {
    // Sample points land at x = col*159/8, y = row*119/7 for col in 0..9,
    // row in 0..8: never x in [50, 53) with this width/height, so a "clock"
    // block there never reaches a sampled pixel and the dhash is unchanged.
    let frames = (0..10)
        .map(|i| {
            let mut frame = solid_gradient(30);
            set_block(&mut frame, 50, 50, 3, 10 + i as u8 * 5);
            frame
        })
        .collect();
    Sequence {
        name: "clock_tick",
        frames,
        expected_keep: 1,
    }
}

fn scroll_sequence() -> Sequence {
    // Each frame's gradient origin shifts by a large, distinct step so every
    // sampled point sees a substantially different luma value than the
    // previous kept frame.
    let frames = (0..10).map(|i| solid_gradient((i * 40) as u8)).collect();
    Sequence {
        name: "scroll",
        frames,
        expected_keep: 10,
    }
}

fn tab_flicker_sequence() -> Sequence {
    // Two maximally different gradients (increasing vs decreasing), the
    // same shape used in dhash_tests::dhash_identical_zero_and_reversed_gradient_is_max_distance.
    let mut a = vec![0u8; SEQ_WIDTH * SEQ_HEIGHT * 4];
    let mut b = vec![0u8; SEQ_WIDTH * SEQ_HEIGHT * 4];
    for y in 0..SEQ_HEIGHT {
        for x in 0..SEQ_WIDTH {
            let i = (y * SEQ_WIDTH + x) * 4;
            let inc = ((x * 250) / SEQ_WIDTH) as u8;
            a[i] = inc;
            a[i + 1] = inc;
            a[i + 2] = inc;
            a[i + 3] = 255;
            let dec = 250 - inc;
            b[i] = dec;
            b[i + 1] = dec;
            b[i + 2] = dec;
            b[i + 3] = 255;
        }
    }
    let frames = (0..10)
        .map(|i| if i % 2 == 0 { a.clone() } else { b.clone() })
        .collect();
    Sequence {
        name: "tab_flicker",
        frames,
        expected_keep: 2,
    }
}

fn typing_sequence() -> Sequence {
    // Reveal 7 more of the 72 sampled points (dark) per frame, in a fixed
    // order: a monotonically growing line of "text" with a strictly
    // increasing revealed-point count every frame (7, 14, ..., 70 of 72),
    // so it never saturates and repeats an earlier frame's state across
    // all 10 frames.
    let sample_x: Vec<usize> = (0..9).map(|col| (col * (SEQ_WIDTH - 1)) / 8).collect();
    let sample_y: Vec<usize> = (0..8).map(|row| (row * (SEQ_HEIGHT - 1)) / 7).collect();
    let all_points: Vec<(usize, usize)> = sample_y
        .iter()
        .flat_map(|&y| sample_x.iter().map(move |&x| (x, y)))
        .collect();
    let frames = (0..10)
        .map(|i| {
            let mut frame = solid_gradient(180);
            let revealed = ((i + 1) * 7).min(all_points.len());
            for &(x, y) in all_points.iter().take(revealed) {
                let idx = (y * SEQ_WIDTH + x) * 4;
                frame[idx] = 5;
                frame[idx + 1] = 5;
                frame[idx + 2] = 5;
            }
            frame
        })
        .collect();
    Sequence {
        name: "typing",
        frames,
        expected_keep: 10,
    }
}

/// The new dedupe decision: drop a frame within `DHASH_THRESHOLD` of the
/// last kept frame, or that revisits a state from two frames back without
/// having just shown it (A-B-A flicker), per CAP-08.
fn dhash_keep_flags(frames: &[Vec<u8>]) -> Vec<bool> {
    let mut keep = Vec::with_capacity(frames.len());
    let mut recent: VecDeque<u64> = VecDeque::new();
    let mut last_kept: Option<u64> = None;
    for frame in frames {
        let hash = dhash_9x8(&luma_9x8_from_rgba(frame, SEQ_WIDTH, SEQ_HEIGHT));
        let is_dup = last_kept
            .map(|prev| hamming(hash, prev) <= DHASH_THRESHOLD)
            .unwrap_or(false);
        let is_flicker = is_aba(&recent, hash, ABA_THRESHOLD);
        let keep_this = !is_dup && !is_flicker;
        keep.push(keep_this);
        if keep_this {
            last_kept = Some(hash);
        }
        recent.push_back(hash);
        if recent.len() > 2 {
            recent.pop_front();
        }
    }
    keep
}

fn png_bytes(rgba: &[u8]) -> Vec<u8> {
    let img: image::RgbaImage =
        image::ImageBuffer::from_raw(SEQ_WIDTH as u32, SEQ_HEIGHT as u32, rgba.to_vec())
            .expect("valid rgba buffer");
    let mut out = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("encode png");
    out
}

fn img_hash_keep_count(frames: &[Vec<u8>]) -> usize {
    let mut hasher = PerceptualHasher::new();
    frames
        .iter()
        .filter(|frame| !hasher.is_duplicate(&png_bytes(frame), IMG_HASH_THRESHOLD))
        .count()
}

#[test]
fn dhash_dedupe_keeps_expected_frames_with_zero_false_drops_on_sequences() {
    let sequences = [
        idle_sequence(),
        cursor_blink_sequence(),
        clock_tick_sequence(),
        scroll_sequence(),
        tab_flicker_sequence(),
        typing_sequence(),
    ];

    let mut report = String::from("# CAP-08 dedupe comparison: img_hash (before) vs dHash + A-B-A (after)\n\n");
    report.push_str("| Sequence | Expected keep | img_hash keeps | dHash+ABA keeps |\n");
    report.push_str("|---|---|---|---|\n");

    for seq in &sequences {
        let new_keep_flags = dhash_keep_flags(&seq.frames);
        let new_keep_count = new_keep_flags.iter().filter(|k| **k).count();
        let old_keep_count = img_hash_keep_count(&seq.frames);
        report.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            seq.name, seq.expected_keep, old_keep_count, new_keep_count
        ));

        assert_eq!(
            new_keep_count, seq.expected_keep,
            "sequence {} expected {} kept frames, dHash+ABA kept {} ({:?})",
            seq.name, seq.expected_keep, new_keep_count, new_keep_flags
        );
        if seq.name == "scroll" || seq.name == "typing" {
            assert_eq!(
                new_keep_count,
                seq.frames.len(),
                "sequence {} must have zero false drops",
                seq.name
            );
        }
    }

    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/evidence/W03");
    std::fs::create_dir_all(&out_dir).expect("create evidence dir");
    std::fs::write(out_dir.join("cap-08-before-after.md"), &report).expect("write evidence");
    println!("{report}");
}
