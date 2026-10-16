use fndr_lib::capture::{PerceptualHasher, ScreenObservation, TriggerGate, TriggerPolicy};
use fndr_lib::config::Config;
use fndr_lib::telemetry::system_metrics::sampler_plan;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Fixture {
    id: String,
    file: String,
    bundle_id: String,
    window_title: String,
    app_class: String,
    expected_outcome: String,
}

struct Screen {
    id: String,
    bytes: Vec<u8>,
    bundle_id: String,
    title: String,
    app_class: String,
}

fn positive_screens() -> Vec<Screen> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens");
    let raw = std::fs::read_to_string(dir.join("manifest.json")).expect("fixture manifest");
    let fixtures: Vec<Fixture> = serde_json::from_str(&raw).expect("valid fixture manifest");
    fixtures
        .into_iter()
        .filter(|fixture| fixture.expected_outcome == "store")
        .map(|fixture| Screen {
            id: fixture.id,
            bytes: std::fs::read(dir.join(fixture.file)).expect("fixture PNG"),
            bundle_id: fixture.bundle_id,
            title: fixture.window_title,
            app_class: fixture.app_class,
        })
        .collect()
}

struct Replay {
    pixel_captures: usize,
    kept: BTreeSet<String>,
    transient_kept: bool,
    animated_title_captures: usize,
}

fn replay(screens: &[Screen], event_driven: bool) -> Replay {
    let config = Config::default();
    let policy = TriggerPolicy::from_config(&config);
    let mut gate = TriggerGate::new();
    let mut hasher = PerceptualHasher::new();
    let mut last_input_at = -100.0;
    let mut result = Replay {
        pixel_captures: 0,
        kept: BTreeSet::new(),
        transient_kept: false,
        animated_title_captures: 0,
    };

    for second in (0u32..3_600).step_by(2) {
        let state_index = ((second / 30) as usize).min(screens.len() - 1);
        let transient = (1_002..1_006).contains(&second);
        let screen = if transient {
            screens
                .iter()
                .find(|screen| screen.id == "chat_mock-01")
                .expect("passive transient fixture")
        } else {
            &screens[state_index]
        };
        let metadata = &screens[state_index];
        let offset = second % 30;
        if matches!(metadata.app_class.as_str(), "editor" | "browser_article")
            && matches!(offset, 2 | 4 | 6)
            && second < (screens.len() as u32) * 30
        {
            last_input_at = second as f64;
        }
        let spinner_title = if (1_200..1_220).contains(&second) {
            Some(if second % 4 == 0 {
                "⠋ synthetic status"
            } else {
                "⠙ synthetic status"
            })
        } else {
            None
        };
        let title = spinner_title.unwrap_or_else(|| {
            if state_index == 1 {
                &screens[0].title
            } else {
                &metadata.title
            }
        });
        let observation = ScreenObservation {
            bundle_id: Some(&metadata.bundle_id),
            window_title: title,
            url: None,
            input_idle_secs: second as f64 - last_input_at,
        };
        let capture = !event_driven || gate.decide(second as f64, &observation, &policy).is_some();
        if !capture {
            continue;
        }
        result.pixel_captures += 1;
        if spinner_title.is_some() {
            result.animated_title_captures += 1;
        }
        if !hasher
            .check(&screen.bytes, config.dedupe_threshold)
            .is_duplicate
        {
            if transient {
                result.transient_kept = true;
            } else {
                result.kept.insert(screen.id.clone());
            }
        }
    }
    result
}

#[test]
fn event_policy_preserves_long_lived_fixture_states() {
    let screens = positive_screens();
    assert_eq!(screens.len(), 25);
    let timer = replay(&screens, false);
    let event = replay(&screens, true);
    let lost: Vec<_> = timer.kept.difference(&event.kept).collect();
    println!(
        "active one hour: timer={} pixel captures, event={} pixel captures, timer novel={}, event novel={}",
        timer.pixel_captures,
        event.pixel_captures,
        timer.kept.len(),
        event.kept.len()
    );
    println!(
        "short passive transient: timer kept={}, event kept={}",
        timer.transient_kept, event.transient_kept
    );
    assert_eq!(timer.pixel_captures, 1_800);
    assert!(lost.is_empty(), "long-lived fixture states lost: {lost:?}");
    assert!(
        timer.transient_kept,
        "timer should see the passive transient"
    );
    assert!(
        !event.transient_kept,
        "short passive transient is the known loss"
    );
    assert!(event.animated_title_captures <= 2);
}

#[test]
fn idle_policy_caps_pixel_work_and_leaves_gpu_sampler_off() {
    let config = Config::default();
    let policy = TriggerPolicy::from_config(&config);
    let mut gate = TriggerGate::new();
    let mut captures = 0;
    for second in (0u32..3_600).step_by(5) {
        let observation = ScreenObservation {
            bundle_id: Some("com.apple.Preview"),
            window_title: "Synthetic static page",
            url: None,
            input_idle_secs: 100.0 + second as f64,
        };
        if gate.decide(second as f64, &observation, &policy).is_some() {
            captures += 1;
        }
    }
    println!("idle one hour: timer=720 pixel captures, event={captures}");
    assert!(captures <= 180);
    assert!(!sampler_plan(100_000, 0).sample_gpu);
}
