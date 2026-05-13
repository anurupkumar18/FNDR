//! Map a search hit to a coarse action bucket using URL, paths, titles, and
//! structured fields — not `app_name` alone.

use crate::storage::SearchResult;

use super::classify_rules::{
    any_path_suffix, any_substring, CODING_FILE_SUFFIXES, CODING_TITLE_PHRASES,
    COMMUNICATION_TITLE_FRAGMENTS, COMMUNICATION_URL_FRAGMENTS, MEETING_TITLE_FRAGMENTS,
    MEETING_URL_FRAGMENTS, PLANNING_TITLE_PHRASES, PLANNING_URL_HOSTS, RESEARCH_TITLE_PHRASES,
    RESEARCH_URL_HOSTS, REVIEW_TITLE_PHRASES, REVIEW_URL_PATHS, WRITING_TITLE_PHRASES,
    WRITING_URL_HOSTS,
};

/// Eight timeline-facing action categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionClass {
    Coding,
    Reviewing,
    Planning,
    Research,
    Writing,
    Communication,
    Meeting,
    Other,
}

impl ActionClass {
    pub fn as_str(self) -> &'static str {
        match self {
            ActionClass::Coding => "coding",
            ActionClass::Reviewing => "reviewing",
            ActionClass::Planning => "planning",
            ActionClass::Research => "research",
            ActionClass::Writing => "writing",
            ActionClass::Communication => "communication",
            ActionClass::Meeting => "meeting",
            ActionClass::Other => "other",
        }
    }
}

fn url_lower(result: &SearchResult) -> String {
    result.url.as_deref().unwrap_or("").to_lowercase()
}

fn title_lower(result: &SearchResult) -> String {
    format!(
        "{} {}",
        result.window_title.to_lowercase(),
        result.display_summary.to_lowercase()
    )
}

/// Classify using content-derived signals only.
pub fn classify_action_class(result: &SearchResult) -> ActionClass {
    let url = url_lower(result);
    let blob = title_lower(result);
    let activity = result.activity_type.to_lowercase();

    if any_substring(&activity, &["meeting"])
        || any_substring(&url, MEETING_URL_FRAGMENTS)
        || any_substring(&blob, MEETING_TITLE_FRAGMENTS)
    {
        return ActionClass::Meeting;
    }
    if any_substring(&url, COMMUNICATION_URL_FRAGMENTS)
        || any_substring(&blob, COMMUNICATION_TITLE_FRAGMENTS)
    {
        return ActionClass::Communication;
    }
    if any_substring(&url, REVIEW_URL_PATHS) || any_substring(&blob, REVIEW_TITLE_PHRASES) {
        return ActionClass::Reviewing;
    }
    if any_path_suffix(&result.files_touched, CODING_FILE_SUFFIXES)
        || any_substring(&blob, CODING_TITLE_PHRASES)
    {
        return ActionClass::Coding;
    }
    if any_substring(&url, WRITING_URL_HOSTS) || any_substring(&blob, WRITING_TITLE_PHRASES) {
        return ActionClass::Writing;
    }
    if any_substring(&url, PLANNING_URL_HOSTS) || any_substring(&blob, PLANNING_TITLE_PHRASES) {
        return ActionClass::Planning;
    }
    if any_substring(&url, RESEARCH_URL_HOSTS) || any_substring(&blob, RESEARCH_TITLE_PHRASES) {
        return ActionClass::Research;
    }

    match activity.as_str() {
        "coding" | "development" => ActionClass::Coding,
        "browsing" | "reading" => ActionClass::Research,
        "communication" => ActionClass::Communication,
        "docs" | "documentation" => ActionClass::Writing,
        "design" => ActionClass::Planning,
        _ => ActionClass::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SearchResult;

    fn hit(url: Option<&str>, title: &str, activity: &str, files: &[&str]) -> SearchResult {
        SearchResult {
            url: url.map(str::to_string),
            window_title: title.to_string(),
            display_summary: title.to_string(),
            activity_type: activity.to_string(),
            files_touched: files.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn app_name_alone_does_not_select_meeting() {
        let r = hit(None, "Notes", "other", &[]);
        assert_ne!(classify_action_class(&r), ActionClass::Meeting);
    }

    #[test]
    fn pull_request_url_is_reviewing() {
        let r = hit(
            Some("https://github.com/org/repo/pull/12"),
            "feat: pipeline",
            "other",
            &[],
        );
        assert_eq!(classify_action_class(&r), ActionClass::Reviewing);
    }

    #[test]
    fn touched_rust_file_is_coding() {
        let r = hit(None, "editing", "other", &["src/main.rs"]);
        assert_eq!(classify_action_class(&r), ActionClass::Coding);
    }

    #[test]
    fn zoom_title_is_meeting() {
        let r = hit(None, "Zoom Meeting", "other", &[]);
        assert_eq!(classify_action_class(&r), ActionClass::Meeting);
    }
}
