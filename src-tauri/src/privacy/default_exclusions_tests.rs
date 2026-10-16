//! Built-in exclusions, exercised with synthetic frames: an app, a window
//! title and a URL, never captured text from a real screen.

use super::exclusions;
use super::safety_gate::{evaluate, SafetyDecision};
use crate::capture::capture_context_skip_reason;
use crate::SkipReason;

struct Frame {
    app: &'static str,
    bundle: Option<&'static str>,
    title: &'static str,
    url: Option<&'static str>,
}

fn frame(app: &'static str, title: &'static str) -> Frame {
    Frame {
        app,
        bundle: None,
        title,
        url: None,
    }
}

fn gate(frame: &Frame, blocklist: &[String]) -> SafetyDecision {
    evaluate(
        Some(frame.app),
        frame.bundle,
        frame.url,
        Some(frame.title),
        None,
        blocklist,
    )
}

fn is_excluded_with_empty_blocklist(frame: &Frame) -> bool {
    gate(frame, &[]) == SafetyDecision::SkipStorage
}

#[test]
fn password_managers_and_keychain_are_never_stored_with_an_empty_blocklist() {
    for app in [
        "1Password",
        "1Password 7 - Password Manager",
        "Bitwarden",
        "Dashlane",
        "LastPass",
        "Keychain Access",
        "Passwords",
        "KeePassXC",
        "NordPass",
        "Enpass",
        "Proton Pass",
    ] {
        assert!(
            is_excluded_with_empty_blocklist(&frame(app, "Vault")),
            "{app}"
        );
    }
}

#[test]
fn password_managers_are_found_by_bundle_id_when_the_name_is_localized() {
    for bundle in [
        "com.1password.1password",
        "com.agilebits.onepassword7",
        "com.bitwarden.desktop",
        "com.dashlane.dashlanephonefinal",
        "com.lastpass.LastPass",
        "com.apple.keychainaccess",
        "com.apple.Passwords",
    ] {
        let f = Frame {
            app: "Coffre",
            bundle: Some(bundle),
            title: "Coffre",
            url: None,
        };
        assert!(is_excluded_with_empty_blocklist(&f), "{bundle}");
    }
}

#[test]
fn an_app_with_passwords_in_its_name_is_not_the_passwords_app() {
    assert!(!is_excluded_with_empty_blocklist(&frame(
        "Notes",
        "Reset passwords for the team"
    )));
    assert!(!is_excluded_with_empty_blocklist(&frame(
        "Xcode",
        "PasswordsViewController.swift"
    )));
}

#[test]
fn system_settings_are_never_stored() {
    assert!(is_excluded_with_empty_blocklist(&frame(
        "System Settings",
        "Privacy & Security"
    )));
    assert!(is_excluded_with_empty_blocklist(&frame(
        "System Preferences",
        "Security"
    )));
    let f = Frame {
        app: "Systemeinstellungen",
        bundle: Some("com.apple.systempreferences"),
        title: "Datenschutz",
        url: None,
    };
    assert!(is_excluded_with_empty_blocklist(&f));
}

#[test]
fn banking_sites_are_never_stored_by_host_or_by_the_older_markers() {
    for url in [
        "https://secure.chase.com/web/auth/dashboard",
        "https://www.citi.com/login",
        "https://online.hsbc.co.uk/",
        "https://app.ally.com/accounts",
        "https://www.coinbase.com/portfolio",
        "https://pro.kraken.com/",
        "https://www.bankofamerica.com/",
        "https://www.paypal.com/myaccount",
    ] {
        let f = Frame {
            app: "Safari",
            bundle: None,
            title: "Accounts",
            url: Some(url),
        };
        assert!(is_excluded_with_empty_blocklist(&f), "{url}");
    }
}

#[test]
fn a_page_that_merely_mentions_a_bank_in_its_path_is_not_a_banking_host() {
    let f = Frame {
        app: "Safari",
        bundle: None,
        title: "How to read a statement",
        url: Some("https://blog.example.com/posts/ally.com-vs-sofi.com-review"),
    };
    assert!(!is_excluded_with_empty_blocklist(&f));
    assert!(!exclusions::is_banking_host(Some(
        "https://blog.example.com/wise.com"
    )));
    assert!(exclusions::is_banking_host(Some(
        "https://www.wise.com/send"
    )));
}

#[test]
fn private_windows_of_every_named_browser_are_never_stored() {
    let cases = [
        ("Safari", "Private Browsing \u{2014} Start Page"),
        ("Google Chrome", "New Tab - Google Chrome (Incognito)"),
        ("Arc", "Incognito \u{2014} Docs"),
        ("Dia", "Notes on the launch - Private"),
        ("Brave Browser", "New Tab - Brave (Private)"),
        ("Microsoft Edge", "New tab - InPrivate - Microsoft Edge"),
        ("Firefox", "Mozilla Firefox Private Browsing"),
        ("Brave Browser", "Private Window with Tor"),
    ];
    for (app, title) in cases {
        assert!(
            is_excluded_with_empty_blocklist(&frame(app, title)),
            "{app}: {title}"
        );
    }
}

#[test]
fn the_same_words_in_a_document_title_do_not_hide_a_normal_window() {
    assert!(!is_excluded_with_empty_blocklist(&frame(
        "Pages",
        "Budget - Private"
    )));
    assert!(!is_excluded_with_empty_blocklist(&frame(
        "Google Chrome",
        "Quarterly planning - Google Chrome"
    )));
}

#[test]
fn ordinary_work_is_still_stored() {
    let f = Frame {
        app: "Visual Studio Code",
        bundle: Some("com.microsoft.VSCode"),
        title: "lib.rs - fndr",
        url: None,
    };
    assert_eq!(gate(&f, &[]), SafetyDecision::Allow);
}

#[test]
fn capture_admission_reports_the_built_ins_as_sensitive_context_not_blocklist() {
    let reason = capture_context_skip_reason("Bitwarden", None, "Vault", None, &[]);
    assert_eq!(reason, Some(SkipReason::SensitiveContext));
}

#[test]
fn an_existing_users_custom_blocklist_still_applies_next_to_the_built_ins() {
    let custom = vec!["Acme Client Portal".to_string(), "Figma".to_string()];
    let figma = frame("Figma", "Wireframes");
    assert_eq!(gate(&figma, &custom), SafetyDecision::SkipStorage);
    assert_eq!(gate(&figma, &[]), SafetyDecision::Allow);
    let reason = capture_context_skip_reason("Figma", None, "Wireframes", None, &custom);
    assert_eq!(reason, Some(SkipReason::Blocklist));
    let reason = capture_context_skip_reason("1Password", None, "Vault", None, &custom);
    assert!(reason.is_some());
}

#[test]
fn the_default_config_lists_the_password_managers_so_new_users_see_them() {
    let blocklist = crate::config::Config::default().blocklist;
    for name in [
        "1Password",
        "Bitwarden",
        "Dashlane",
        "LastPass",
        "Keychain Access",
        "System Settings",
    ] {
        assert!(blocklist.iter().any(|b| b == name), "{name}");
    }
}

#[test]
fn every_category_shown_to_the_person_has_a_label_and_examples() {
    let categories = exclusions::categories();
    assert!(categories.len() >= 6);
    assert!(categories
        .iter()
        .all(|c| !c.id.is_empty() && !c.label.is_empty() && !c.examples.is_empty()));
}
