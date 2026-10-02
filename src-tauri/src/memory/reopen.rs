use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[derive(Default)]
pub enum ReopenKind {
    BrowserUrl,
    FilePath,
    AppBundle,
    AppDeepLink,
    #[default]
    Unknown,
}


impl ReopenKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BrowserUrl => "browser_url",
            Self::FilePath => "file_path",
            Self::AppBundle => "app_bundle",
            Self::AppDeepLink => "app_deep_link",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_label(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "browser_url" => Self::BrowserUrl,
            "file_path" => Self::FilePath,
            "app_bundle" => Self::AppBundle,
            "app_deep_link" => Self::AppDeepLink,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[derive(Default)]
pub enum ReopenValidationStatus {
    Valid,
    Invalid,
    #[default]
    Unchecked,
}


impl ReopenValidationStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Valid => "valid",
            Self::Invalid => "invalid",
            Self::Unchecked => "unchecked",
        }
    }

    pub fn from_label(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "valid" => Self::Valid,
            "invalid" => Self::Invalid,
            _ => Self::Unchecked,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReopenTarget {
    pub kind: ReopenKind,
    pub url: Option<String>,
    pub file_path: Option<String>,
    pub app_bundle_id: Option<String>,
    pub app_name: Option<String>,
    pub app_deep_link: Option<String>,
    pub captured_at_ms: i64,
    pub confidence: f32,
    pub validation_status: ReopenValidationStatus,
}

fn is_http_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

pub fn build_reopen_target(
    source_url: Option<&str>,
    first_file_path: Option<&str>,
    bundle_id: Option<&str>,
    app_name: &str,
    captured_at_ms: i64,
) -> ReopenTarget {
    if let Some(url) = source_url.map(str::trim).filter(|v| is_http_url(v)) {
        return ReopenTarget {
            kind: ReopenKind::BrowserUrl,
            url: Some(url.to_string()),
            captured_at_ms,
            confidence: 0.95,
            validation_status: ReopenValidationStatus::Valid,
            ..Default::default()
        };
    }

    if let Some(path) = first_file_path.map(str::trim).filter(|v| !v.is_empty()) {
        return ReopenTarget {
            kind: ReopenKind::FilePath,
            file_path: Some(path.to_string()),
            captured_at_ms,
            confidence: 0.85,
            validation_status: ReopenValidationStatus::Unchecked,
            ..Default::default()
        };
    }

    if let Some(bundle) = bundle_id.map(str::trim).filter(|v| !v.is_empty()) {
        return ReopenTarget {
            kind: ReopenKind::AppBundle,
            app_bundle_id: Some(bundle.to_string()),
            app_name: (!app_name.trim().is_empty()).then_some(app_name.trim().to_string()),
            captured_at_ms,
            confidence: 0.70,
            validation_status: ReopenValidationStatus::Unchecked,
            ..Default::default()
        };
    }

    ReopenTarget {
        kind: ReopenKind::Unknown,
        app_name: (!app_name.trim().is_empty()).then_some(app_name.trim().to_string()),
        captured_at_ms,
        confidence: 0.0,
        validation_status: ReopenValidationStatus::Invalid,
        ..Default::default()
    }
}

pub fn serialize_reopen_target(target: &ReopenTarget) -> String {
    serde_json::to_string(target).unwrap_or_else(|_| "{}".to_string())
}

pub fn deserialize_reopen_target(value: &str) -> Option<ReopenTarget> {
    serde_json::from_str::<ReopenTarget>(value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: i64 = 1_700_000_000_000;

    #[test]
    fn build_reopen_target_accepts_http_and_https() {
        let cases = [
            ("http", "http://example.com/a", "http://example.com/a"),
            ("https", "https://example.com/a", "https://example.com/a"),
            ("uppercase scheme", "HTTPS://EXAMPLE.COM/A", "HTTPS://EXAMPLE.COM/A"),
            ("surrounding whitespace", "  https://example.com/a \n", "https://example.com/a"),
        ];
        for (label, input, expected) in cases {
            let target = build_reopen_target(
                Some(input),
                Some("/Users/qa/doc.pdf"),
                Some("com.google.Chrome"),
                "Chrome",
                AT,
            );
            assert_eq!(target.kind, ReopenKind::BrowserUrl, "{label}");
            assert_eq!(target.url.as_deref(), Some(expected), "{label}");
            assert_eq!(target.file_path, None, "{label}");
            assert_eq!(target.app_bundle_id, None, "{label}");
            assert_eq!(target.confidence, 0.95, "{label}");
            assert_eq!(target.validation_status, ReopenValidationStatus::Valid, "{label}");
            assert_eq!(target.captured_at_ms, AT, "{label}");
        }
    }

    #[test]
    fn build_reopen_target_rejects_non_http_schemes() {
        let cases = [
            ("javascript", "javascript:alert(1)"),
            ("data", "data:text/html,<b>hi</b>"),
            ("file from a browser", "file:///Users/qa/page.html"),
            ("chrome", "chrome://settings"),
            ("about", "about:blank"),
            ("no scheme", "example.com/a"),
        ];
        for (label, input) in cases {
            let with_app =
                build_reopen_target(Some(input), None, Some("com.google.Chrome"), "Chrome", AT);
            assert_eq!(with_app.kind, ReopenKind::AppBundle, "{label} with app");
            assert_eq!(with_app.url, None, "{label} with app");

            let without_app = build_reopen_target(Some(input), None, None, "", AT);
            assert_eq!(without_app.kind, ReopenKind::Unknown, "{label} without app");
            assert_eq!(without_app.url, None, "{label} without app");
        }
    }

    #[test]
    fn build_reopen_target_prefers_url_then_file_then_app() {
        let url_over_file = build_reopen_target(
            Some("https://example.com"),
            Some("/Users/qa/doc.pdf"),
            None,
            "Preview",
            AT,
        );
        assert_eq!(url_over_file.kind, ReopenKind::BrowserUrl);

        let file_over_app = build_reopen_target(
            None,
            Some("  /Users/qa/doc.pdf "),
            Some("com.apple.Preview"),
            "Preview",
            AT,
        );
        assert_eq!(file_over_app.kind, ReopenKind::FilePath);
        assert_eq!(file_over_app.file_path.as_deref(), Some("/Users/qa/doc.pdf"));
        assert_eq!(file_over_app.app_bundle_id, None);
        assert_eq!(file_over_app.confidence, 0.85);
        assert_eq!(file_over_app.validation_status, ReopenValidationStatus::Unchecked);

        let app = build_reopen_target(None, Some("   "), Some(" com.apple.Preview "), " Preview ", AT);
        assert_eq!(app.kind, ReopenKind::AppBundle);
        assert_eq!(app.app_bundle_id.as_deref(), Some("com.apple.Preview"));
        assert_eq!(app.app_name.as_deref(), Some("Preview"));
        assert_eq!(app.confidence, 0.70);
        assert_eq!(app.validation_status, ReopenValidationStatus::Unchecked);
    }

    #[test]
    fn build_reopen_target_empty_inputs_give_unknown() {
        let cases: [(&str, Option<&str>, Option<&str>, Option<&str>, &str); 3] = [
            ("all none", None, None, None, ""),
            ("all whitespace", Some("  "), Some("\t"), Some(" "), "  "),
            ("all empty", Some(""), Some(""), Some(""), ""),
        ];
        for (label, url, file, bundle, app_name) in cases {
            let target = build_reopen_target(url, file, bundle, app_name, AT);
            assert_eq!(target.kind, ReopenKind::Unknown, "{label}");
            assert_eq!(target.app_name, None, "{label}");
            assert_eq!(target.confidence, 0.0, "{label}");
            assert_eq!(target.validation_status, ReopenValidationStatus::Invalid, "{label}");
        }

        let named = build_reopen_target(None, None, None, " Zoom ", AT);
        assert_eq!(named.kind, ReopenKind::Unknown);
        assert_eq!(named.app_name.as_deref(), Some("Zoom"));
    }

    #[test]
    fn reopen_labels_round_trip() {
        for kind in [
            ReopenKind::BrowserUrl,
            ReopenKind::FilePath,
            ReopenKind::AppBundle,
            ReopenKind::AppDeepLink,
            ReopenKind::Unknown,
        ] {
            assert_eq!(ReopenKind::from_label(kind.as_str()), kind);
        }
        assert_eq!(ReopenKind::from_label(" FILE_PATH "), ReopenKind::FilePath);
        assert_eq!(ReopenKind::from_label("folder"), ReopenKind::Unknown);

        for status in [
            ReopenValidationStatus::Valid,
            ReopenValidationStatus::Invalid,
            ReopenValidationStatus::Unchecked,
        ] {
            assert_eq!(ReopenValidationStatus::from_label(status.as_str()), status);
        }
        assert_eq!(
            ReopenValidationStatus::from_label("stale"),
            ReopenValidationStatus::Unchecked
        );
    }

    #[test]
    fn reopen_target_serialization_round_trips() {
        let target = build_reopen_target(
            None,
            Some("/Users/qa/café 📁/report (1) ✨.txt"),
            None,
            "Finder",
            AT,
        );
        let restored = deserialize_reopen_target(&serialize_reopen_target(&target))
            .expect("round trip");
        assert_eq!(restored.kind, target.kind);
        assert_eq!(restored.file_path, target.file_path);
        assert_eq!(restored.captured_at_ms, target.captured_at_ms);
        assert_eq!(restored.validation_status, target.validation_status);

        assert!(deserialize_reopen_target("not json").is_none());
        assert!(deserialize_reopen_target("").is_none());
    }

    // Should be app only (R18 unsaved TextEdit) or the folder (R20 Finder); a
    // relative or host-like path from `files_touched` is not a file target.
    #[test]
    fn build_reopen_target_accepts_relative_paths_over_app_flips_r18() {
        for input in ["plan.md", "en.wikipedia.org/wiki/Nitrogen", "./notes.txt"] {
            let target = build_reopen_target(
                None,
                Some(input),
                Some("com.apple.TextEdit"),
                "TextEdit",
                AT,
            );
            assert_eq!(target.kind, ReopenKind::FilePath, "{input}");
            assert_eq!(target.file_path.as_deref(), Some(input), "{input}");
        }
    }
}
