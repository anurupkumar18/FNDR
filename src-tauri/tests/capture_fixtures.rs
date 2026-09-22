#[path = "support/cer.rs"]
mod cer;

use serde::Deserialize;
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
