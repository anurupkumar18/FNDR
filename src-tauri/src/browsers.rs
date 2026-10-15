//! Which apps are web browsers: one answer for capture (whether a window's
//! document is a page to reopen), Notch Do's planner and checks, and its
//! policy. There were five lists, and capture's had no Dia, so a Dia window
//! never kept its page's URL.

/// Words that name a browser in an app's name, matched whole: "Google Chrome
/// Canary", "Firefox Developer Edition", "Dia", but not "Archive Utility".
const BROWSER_WORDS: &[&str] = &[
    "safari", "chrome", "chromium", "firefox", "arc", "dia", "brave", "edge", "opera", "vivaldi",
    "orion", "zen",
];

/// Bundle ids whose apps are browsers, by prefix.
const BROWSER_BUNDLES: &[&str] = &[
    "com.apple.safari",
    "com.google.chrome",
    "org.chromium.",
    "org.mozilla.firefox",
    "company.thebrowser.",
    "com.brave.browser",
    "com.microsoft.edgemac",
    "com.operasoftware.opera",
    "com.vivaldi.vivaldi",
    "com.kagi.kagimacos",
    "app.zen-browser.",
];

/// Whether an app, by its name or its bundle id, is a web browser.
pub fn is_browser(app: &str) -> bool {
    let lower = app.trim().to_lowercase();
    if BROWSER_BUNDLES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }
    // A bundle id is not a name: "com.example.arcade" names no browser.
    !lower.contains('.')
        && lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| BROWSER_WORDS.contains(&word))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browsers_are_known_by_whole_words_and_bundle_ids() {
        for browser in [
            "Safari",
            "Google Chrome",
            "Google Chrome Canary",
            "Firefox Developer Edition",
            "Arc",
            "Dia",
            "Brave Browser",
            "Microsoft Edge",
            "Opera GX",
            "Vivaldi",
            "Orion",
            "Zen",
            "company.thebrowser.dia",
            "com.google.Chrome",
            "com.microsoft.edgemac",
        ] {
            assert!(is_browser(browser), "{browser}");
        }
        for app in [
            "Finder",
            "Archive Utility",
            "Knowledge Base",
            "Notes",
            "Xcode",
            "Slack",
            "Searchlight",
            "com.example.arcade",
            "",
        ] {
            assert!(!is_browser(app), "{app}");
        }
    }
}
