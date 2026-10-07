use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_anchor: Option<String>,
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

const MAX_PDF_PAGE: u32 = 99_999;

fn valid_page_pair(page: u32, of: u32) -> Option<u32> {
    (page >= 1 && of >= page && of <= MAX_PDF_PAGE).then_some(page)
}

fn parse_positive_page(value: &str) -> Option<u32> {
    let parsed = value.trim().parse::<u32>().ok()?;
    (parsed >= 1 && parsed <= MAX_PDF_PAGE).then_some(parsed)
}

/// Current page from a Preview-style window title.
///
/// Matches a dash (or parenthesized) page marker: `WORD N CONNECTOR M`,
/// where CONNECTOR is `of` / `sur` / `von` / `de`. Observed live:
/// English `– Page N of M`, French `– Page N sur M`, German `– Seite N von M`,
/// Spanish `– Página N de M`, plus `(page N of M)`. Requires `1 <= N <= M`.
/// A total-only title such as `x.pdf – 1 page` is not a current page.
pub fn page_from_window_title(title: &str) -> Option<u32> {
    if let Some(page) = page_from_parenthesized(title) {
        return Some(page);
    }
    page_from_dash_page_of(title)
}

fn page_from_parenthesized(title: &str) -> Option<u32> {
    let mut rest = title;
    while let Some(start) = rest.find('(') {
        let inside = rest[start + 1..].split(')').next()?;
        if let Some(page) = page_from_label_n_connector_m(inside) {
            return Some(page);
        }
        rest = &rest[start + 1..];
    }
    None
}

fn page_from_dash_page_of(title: &str) -> Option<u32> {
    for marker in ['–', '-', '—'] {
        if let Some(idx) = title.rfind(marker) {
            if let Some(page) = page_from_label_n_connector_m(&title[idx + marker.len_utf8()..]) {
                return Some(page);
            }
        }
    }
    None
}

fn page_from_label_n_connector_m(value: &str) -> Option<u32> {
    let mut parts = value.split_whitespace();
    let _label = parts.next()?;
    let n = parse_positive_page(parts.next()?)?;
    let connector = parts.next()?;
    if !is_page_connector(connector) {
        return None;
    }
    let m = parse_positive_page(parts.next()?)?;
    valid_page_pair(n, m)
}

fn is_page_connector(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "of" | "sur" | "von" | "de"
    )
}

/// Reads `page=N` from a URL fragment (`#page=12` or `#page=12&zoom=100`).
pub fn page_from_pdf_url(url: &str) -> Option<u32> {
    let fragment = url.split_once('#')?.1;
    for pair in fragment.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key.eq_ignore_ascii_case("page") {
            return parse_positive_page(value);
        }
    }
    None
}

fn url_path_ends_with_pdf(url: &str) -> bool {
    let without_fragment = url.split('#').next().unwrap_or(url);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    without_query.to_ascii_lowercase().ends_with(".pdf")
}

/// Chrome/Brave/Edge PDF toolbar OCR: a whole line that is `N / M` or `N of M`.
/// Runs only when the URL path (ignoring query and fragment) ends in `.pdf`.
pub fn page_from_pdf_viewer_ocr(url: &str, ocr_text: &str) -> Option<u32> {
    if !url_path_ends_with_pdf(url) {
        return None;
    }
    for line in ocr_text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(page) = page_from_slash_pair(trimmed).or_else(|| page_from_ocr_n_of_m(trimmed))
        {
            return Some(page);
        }
    }
    None
}

fn page_from_slash_pair(line: &str) -> Option<u32> {
    let (left, right) = line.split_once('/')?;
    let left = left.trim();
    let right = right.trim();
    if left.split_whitespace().nth(1).is_some() || right.split_whitespace().nth(1).is_some() {
        return None;
    }
    valid_page_pair(parse_positive_page(left)?, parse_positive_page(right)?)
}

fn page_from_ocr_n_of_m(line: &str) -> Option<u32> {
    let mut parts = line.split_whitespace();
    let n = parse_positive_page(parts.next()?)?;
    if !parts.next()?.eq_ignore_ascii_case("of") {
        return None;
    }
    let m = parse_positive_page(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    valid_page_pair(n, m)
}

/// File targets use the window title. Browser URLs use `#page=N` first, then
/// the PDF toolbar OCR. Other kinds never store a page.
pub fn detect_reopen_page(
    target: &ReopenTarget,
    window_title: &str,
    ocr_text: &str,
) -> Option<u32> {
    match target.kind {
        ReopenKind::FilePath => page_from_window_title(window_title),
        ReopenKind::BrowserUrl => {
            let url = target.url.as_deref().unwrap_or("");
            page_from_pdf_url(url).or_else(|| page_from_pdf_viewer_ocr(url, ocr_text))
        }
        _ => None,
    }
}

/// Replaces an existing `page=` fragment or appends `#page=N`. Any other
/// fragment is left unchanged so text anchors and named destinations survive.
pub fn url_with_pdf_page(url: &str, page: u32) -> String {
    match url.split_once('#') {
        None => format!("{url}#page={page}"),
        Some((base, fragment)) => {
            if fragment_has_page_key(fragment) {
                format!("{base}#page={page}")
            } else {
                url.to_string()
            }
        }
    }
}

fn fragment_has_page_key(fragment: &str) -> bool {
    fragment.split('&').any(|pair| {
        pair.split_once('=')
            .is_some_and(|(key, _)| key.eq_ignore_ascii_case("page"))
    })
}

fn nonempty_ref(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

/// Same URL or same file path. A page or passage from one document must not
/// land on another.
fn same_reopen_document(
    incoming_url: Option<&str>,
    incoming_path: Option<&str>,
    existing_url: Option<&str>,
    existing_path: Option<&str>,
) -> bool {
    let same_url = match (nonempty_ref(incoming_url), nonempty_ref(existing_url)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    let same_path = match (nonempty_ref(incoming_path), nonempty_ref(existing_path)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    same_url || same_path
}

/// Keep a page across a merge only when both records name the same URL or the
/// same file. Otherwise the incoming page wins so a page from one document
/// cannot land on another.
pub fn merge_reopen_page(
    incoming_url: Option<&str>,
    incoming_path: Option<&str>,
    incoming_page: Option<u32>,
    existing_url: Option<&str>,
    existing_path: Option<&str>,
    existing_page: Option<u32>,
) -> Option<u32> {
    if same_reopen_document(incoming_url, incoming_path, existing_url, existing_path) {
        incoming_page.or(existing_page)
    } else {
        incoming_page
    }
}

const MIN_ANCHOR_WORDS: usize = 8;
const MAX_ANCHOR_WORDS: usize = 12;

fn normalize_ws(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A passage the browser can scroll to. Taken from `preferred_passage` only
/// when that passage is already inside `stored_text` (the privacy gate has
/// already accepted whatever was stored). Otherwise the first usable window
/// from the stored text's salient spans.
pub fn text_anchor_from(
    stored_text: &str,
    preferred_passage: Option<&str>,
    app_name: &str,
) -> Option<String> {
    let stored = normalize_ws(stored_text);
    if stored.is_empty() {
        return None;
    }
    if let Some(preferred) = preferred_passage
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let preferred_norm = normalize_ws(preferred);
        if !preferred_norm.is_empty() && stored.contains(&preferred_norm) {
            if let Some(anchor) = first_anchor_window(preferred) {
                if stored.contains(&normalize_ws(&anchor)) {
                    return Some(anchor);
                }
            }
        }
    }
    for span in crate::capture::text_cleanup::rank_salient_spans(stored_text, app_name) {
        if let Some(anchor) = first_anchor_window(&span.text) {
            if stored.contains(&normalize_ws(&anchor)) {
                return Some(anchor);
            }
        }
    }
    None
}

fn first_anchor_window(candidate: &str) -> Option<String> {
    let words: Vec<&str> = candidate.split_whitespace().collect();
    if words.len() < MIN_ANCHOR_WORDS {
        return None;
    }
    let take = words.len().min(MAX_ANCHOR_WORDS);
    let phrase = words[..take].join(" ");
    let trimmed = phrase.trim_matches(|c: char| !c.is_alphanumeric());
    let trimmed_words: Vec<&str> = trimmed.split_whitespace().collect();
    if trimmed_words.len() < MIN_ANCHOR_WORDS || window_rejected(&trimmed_words) {
        return None;
    }
    Some(trimmed_words.join(" "))
}

fn window_rejected(words: &[&str]) -> bool {
    let joined = words.join(" ");
    if joined.contains("://") || joined.contains('@') {
        return true;
    }
    words.iter().any(|word| token_mostly_non_letter(word))
}

fn token_mostly_non_letter(word: &str) -> bool {
    let core = word.trim_matches(|c: char| !c.is_alphanumeric());
    if core.is_empty() {
        return true;
    }
    let letters = core.chars().filter(|c| c.is_alphabetic()).count();
    letters * 2 < core.chars().count()
}

/// Percent-encode every byte except ASCII letters, digits, and `_.~`.
/// Hyphen, comma, and ampersand are always encoded because they are syntax
/// inside a text fragment directive.
pub fn encode_text_fragment(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn host_of_url(url: &str) -> &str {
    let rest = url.split_once("://").map(|(_, tail)| tail).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    host.split(':').next().unwrap_or(host)
}

fn skips_text_fragment(url: &str) -> bool {
    host_of_url(url).eq_ignore_ascii_case("docs.google.com") || url_path_ends_with_pdf(url)
}

/// Appends `#:~:text=` when the URL has no fragment and the site can scroll
/// to a passage. Google Docs and PDF URLs are left unchanged.
pub fn url_with_text_anchor(url: &str, anchor: &str) -> String {
    let anchor = anchor.trim();
    if anchor.is_empty() || url.contains('#') || skips_text_fragment(url) {
        return url.to_string();
    }
    format!("{url}#:~:text={}", encode_text_fragment(anchor))
}

/// Incoming passage wins. The existing one is kept only for the same URL or file.
pub fn merge_reopen_text_anchor(
    incoming_url: Option<&str>,
    incoming_path: Option<&str>,
    incoming_anchor: Option<&str>,
    existing_url: Option<&str>,
    existing_path: Option<&str>,
    existing_anchor: Option<&str>,
) -> Option<String> {
    let incoming = nonempty_ref(incoming_anchor).map(str::to_string);
    let existing = nonempty_ref(existing_anchor).map(str::to_string);
    if same_reopen_document(incoming_url, incoming_path, existing_url, existing_path) {
        incoming.or(existing)
    } else {
        incoming
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
            (
                "uppercase scheme",
                "HTTPS://EXAMPLE.COM/A",
                "HTTPS://EXAMPLE.COM/A",
            ),
            (
                "surrounding whitespace",
                "  https://example.com/a \n",
                "https://example.com/a",
            ),
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
            assert_eq!(
                target.validation_status,
                ReopenValidationStatus::Valid,
                "{label}"
            );
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
        assert_eq!(
            file_over_app.file_path.as_deref(),
            Some("/Users/qa/doc.pdf")
        );
        assert_eq!(file_over_app.app_bundle_id, None);
        assert_eq!(file_over_app.confidence, 0.85);
        assert_eq!(
            file_over_app.validation_status,
            ReopenValidationStatus::Unchecked
        );

        let app = build_reopen_target(
            None,
            Some("   "),
            Some(" com.apple.Preview "),
            " Preview ",
            AT,
        );
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
            assert_eq!(
                target.validation_status,
                ReopenValidationStatus::Invalid,
                "{label}"
            );
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
        let restored =
            deserialize_reopen_target(&serialize_reopen_target(&target)).expect("round trip");
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

    #[test]
    fn page_from_window_title_matches_preview_formats() {
        let cases: &[(&str, Option<u32>)] = &[
            ("re03-preview.pdf – Page 112 of 150", Some(112)),
            ("re03-preview.pdf - Page 112 of 150", Some(112)),
            ("re03-preview.pdf — Page 3 of 10", Some(3)),
            ("re03-preview.pdf (page 112 of 150)", Some(112)),
            ("report (1).pdf (page 112 of 150)", Some(112)),
            ("PDF – PAGE 3 OF 10", Some(3)),
            ("x.pdf – 1 page", None),
            ("re03-preview.pdf – 1 page", None),
            ("doc.pdf – Page 200 of 150", None),
            ("doc.pdf – Page 0 of 10", None),
            ("doc.pdf", None),
            ("", None),
            ("re04-preview-150.pdf – Page 112 sur 150", Some(112)),
            ("re04-preview-150.pdf – Seite 112 von 150", Some(112)),
            ("re04-preview-150.pdf – Página 112 de 150", Some(112)),
        ];
        for (title, expected) in cases {
            assert_eq!(page_from_window_title(title), *expected, "{title}");
        }
    }

    #[test]
    fn page_from_pdf_url_reads_fragment() {
        let cases: &[(&str, Option<u32>)] = &[
            ("https://example.com/doc.pdf#page=12", Some(12)),
            ("https://example.com/doc.pdf#page=12&zoom=100", Some(12)),
            ("https://example.com/doc.pdf#zoom=100&page=7", Some(7)),
            ("https://example.com/doc.pdf#page=0", None),
            ("https://example.com/doc.pdf#section", None),
            ("https://example.com/doc.pdf", None),
        ];
        for (url, expected) in cases {
            assert_eq!(page_from_pdf_url(url), *expected, "{url}");
        }
    }

    #[test]
    fn page_from_pdf_viewer_ocr_requires_pdf_url_and_toolbar_line() {
        let pdf = "https://example.com/report.pdf?token=x#zoom=page-width";
        assert_eq!(
            page_from_pdf_viewer_ocr(pdf, "header\n112 / 300\nfooter"),
            Some(112)
        );
        assert_eq!(page_from_pdf_viewer_ocr(pdf, "112 of 300"), Some(112));
        assert_eq!(page_from_pdf_viewer_ocr(pdf, "0 / 10"), None);
        assert_eq!(page_from_pdf_viewer_ocr(pdf, "200 / 150"), None);
        assert_eq!(
            page_from_pdf_viewer_ocr("https://example.com/article", "3 / 4"),
            None
        );
        assert_eq!(
            page_from_pdf_viewer_ocr("https://example.com/report.pdfx", "3 / 4"),
            None
        );
        assert_eq!(
            page_from_pdf_viewer_ocr(pdf, "see page 3 / 4 in the toolbar"),
            None
        );
    }

    #[test]
    fn detect_reopen_page_uses_title_for_files_and_url_then_ocr_for_browsers() {
        let file = build_reopen_target(
            None,
            Some("/Users/qa/doc.pdf"),
            Some("com.apple.Preview"),
            "Preview",
            AT,
        );
        assert_eq!(
            detect_reopen_page(&file, "doc.pdf – Page 112 of 150", "3 / 4"),
            Some(112)
        );

        let browser = build_reopen_target(
            Some("https://example.com/doc.pdf#page=9"),
            None,
            Some("com.google.Chrome"),
            "Chrome",
            AT,
        );
        assert_eq!(
            detect_reopen_page(&browser, "doc.pdf", "112 / 300"),
            Some(9)
        );

        let browser_ocr = build_reopen_target(
            Some("https://example.com/doc.pdf"),
            None,
            Some("com.google.Chrome"),
            "Chrome",
            AT,
        );
        assert_eq!(
            detect_reopen_page(&browser_ocr, "doc.pdf", "112 / 300"),
            Some(112)
        );

        let app = build_reopen_target(None, None, Some("com.apple.Preview"), "Preview", AT);
        assert_eq!(
            detect_reopen_page(&app, "doc.pdf – Page 112 of 150", "112 / 300"),
            None
        );
    }

    #[test]
    fn url_with_pdf_page_adds_or_replaces_page_fragment() {
        assert_eq!(
            url_with_pdf_page("https://example.com/doc.pdf", 12),
            "https://example.com/doc.pdf#page=12"
        );
        assert_eq!(
            url_with_pdf_page("https://example.com/doc.pdf#page=1", 12),
            "https://example.com/doc.pdf#page=12"
        );
        assert_eq!(
            url_with_pdf_page("https://example.com/doc.pdf#page=1&zoom=100", 12),
            "https://example.com/doc.pdf#page=12"
        );
        assert_eq!(
            url_with_pdf_page("https://example.com/doc.pdf#section", 12),
            "https://example.com/doc.pdf#section"
        );
    }

    #[test]
    fn merge_reopen_page_keeps_page_only_on_the_same_document() {
        assert_eq!(
            merge_reopen_page(
                Some("https://example.com/a.pdf"),
                None,
                Some(3),
                Some("https://example.com/a.pdf"),
                None,
                Some(2),
            ),
            Some(3)
        );
        assert_eq!(
            merge_reopen_page(
                Some("https://example.com/a.pdf"),
                None,
                None,
                Some("https://example.com/a.pdf"),
                None,
                Some(2),
            ),
            Some(2)
        );
        assert_eq!(
            merge_reopen_page(
                None,
                Some("/tmp/a.pdf"),
                Some(9),
                None,
                Some("/tmp/b.pdf"),
                Some(2),
            ),
            Some(9)
        );
        assert_eq!(
            merge_reopen_page(None, None, Some(9), None, None, Some(2)),
            Some(9)
        );
    }

    #[test]
    fn reopen_target_page_round_trips_and_old_json_defaults_none() {
        let mut target = build_reopen_target(
            Some("https://example.com/doc.pdf"),
            None,
            None,
            "Chrome",
            AT,
        );
        target.page = Some(12);
        let restored = deserialize_reopen_target(&serialize_reopen_target(&target)).expect("json");
        assert_eq!(restored.page, Some(12));
        let without_page = deserialize_reopen_target(r#"{"kind":"BrowserUrl","captured_at_ms":1,"confidence":0.0,"validation_status":"Unchecked"}"#)
            .expect("legacy json");
        assert_eq!(without_page.page, None);
        assert_eq!(without_page.text_anchor, None);
    }

    #[test]
    fn text_anchor_from_uses_eight_to_twelve_words_of_a_stored_passage() {
        let stored = "Nitrogen is a chemical element with the symbol N and atomic number seven in the periodic table.";
        let anchor = text_anchor_from(stored, Some(stored), "Chrome").expect("anchor");
        let words = anchor.split_whitespace().count();
        assert!((8..=12).contains(&words), "{anchor}");
        assert_eq!(
            anchor,
            "Nitrogen is a chemical element with the symbol N and atomic number"
        );
        assert!(normalize_ws(stored).contains(&normalize_ws(&anchor)));
    }

    #[test]
    fn text_anchor_from_ignores_a_passage_that_was_not_stored() {
        let stored = "Nitrogen is a chemical element with the symbol N and atomic number seven in the periodic table.";
        let secret =
            "the password is hunter2 and this phrase was never written onto the stored page at all";
        let anchor = text_anchor_from(stored, Some(secret), "Chrome").expect("fallback");
        assert!(!anchor.to_ascii_lowercase().contains("hunter2"));
        assert!(!anchor.contains('@'));
        assert!(normalize_ws(stored).contains(&normalize_ws(&anchor)));

        let short = "Too short.";
        assert_eq!(text_anchor_from(short, Some(secret), "Chrome"), None);
    }

    #[test]
    fn text_anchor_from_rejects_windows_with_urls_emails_or_symbol_tokens() {
        let stored = "Contact user@example.com or open https://secret.example/token immediately before reading the public article body.";
        assert_eq!(text_anchor_from(stored, Some(stored), "Chrome"), None);
    }

    #[test]
    fn encode_text_fragment_encodes_punctuation_dashes_quotes_and_unicode() {
        assert_eq!(encode_text_fragment("one, two"), "one%2C%20two");
        assert_eq!(encode_text_fragment("well-known"), "well%2Dknown");
        assert_eq!(encode_text_fragment("a & b"), "a%20%26%20b");
        assert_eq!(encode_text_fragment("say \"hi\""), "say%20%22hi%22");
        assert_eq!(encode_text_fragment("it's"), "it%27s");
        assert_eq!(
            encode_text_fragment("en\u{2013}dash em\u{2014}dash \u{201C}quote\u{201D}"),
            "en%E2%80%93dash%20em%E2%80%94dash%20%E2%80%9Cquote%E2%80%9D"
        );
        assert_eq!(encode_text_fragment("caf\u{e9}"), "caf%C3%A9");
        assert_eq!(encode_text_fragment("keep_._~"), "keep_._~");
    }

    #[test]
    fn url_with_text_anchor_skips_fragments_google_docs_and_pdfs() {
        assert_eq!(
            url_with_text_anchor("https://example.com/article", "one, two words here"),
            "https://example.com/article#:~:text=one%2C%20two%20words%20here"
        );
        assert_eq!(
            url_with_text_anchor("https://example.com/article#section", "one two three"),
            "https://example.com/article#section"
        );
        assert_eq!(
            url_with_text_anchor("https://docs.google.com/document/d/abc", "one two three"),
            "https://docs.google.com/document/d/abc"
        );
        assert_eq!(
            url_with_text_anchor(
                "https://docs.google.com/spreadsheets/d/abc",
                "one two three"
            ),
            "https://docs.google.com/spreadsheets/d/abc"
        );
        assert_eq!(
            url_with_text_anchor("https://example.com/report.pdf?x=1", "one two three"),
            "https://example.com/report.pdf?x=1"
        );
        assert_eq!(
            url_with_text_anchor("https://example.com/article", "  "),
            "https://example.com/article"
        );
    }

    #[test]
    fn merge_reopen_text_anchor_keeps_passage_only_on_the_same_document() {
        assert_eq!(
            merge_reopen_text_anchor(
                Some("https://example.com/a"),
                None,
                Some("incoming passage wins here"),
                Some("https://example.com/a"),
                None,
                Some("existing passage stays"),
            )
            .as_deref(),
            Some("incoming passage wins here")
        );
        assert_eq!(
            merge_reopen_text_anchor(
                Some("https://example.com/a"),
                None,
                None,
                Some("https://example.com/a"),
                None,
                Some("existing passage stays"),
            )
            .as_deref(),
            Some("existing passage stays")
        );
        assert_eq!(
            merge_reopen_text_anchor(
                Some("https://example.com/b"),
                None,
                Some("incoming only"),
                Some("https://example.com/a"),
                None,
                Some("existing passage stays"),
            )
            .as_deref(),
            Some("incoming only")
        );
        assert_eq!(
            merge_reopen_text_anchor(
                Some("https://example.com/b"),
                None,
                None,
                Some("https://example.com/a"),
                None,
                Some("existing passage stays"),
            ),
            None
        );
    }

    #[test]
    fn reopen_target_text_anchor_round_trips() {
        let mut target = build_reopen_target(
            Some("https://example.com/article"),
            None,
            None,
            "Chrome",
            AT,
        );
        target.text_anchor = Some("Nitrogen is a chemical element with the symbol".into());
        let restored = deserialize_reopen_target(&serialize_reopen_target(&target)).expect("json");
        assert_eq!(restored.text_anchor, target.text_anchor);
    }
}
