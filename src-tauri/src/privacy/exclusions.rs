//! FNDR's built-in exclusions: contexts that are never stored, whatever the
//! person's own blocklist says.
//!
//! The person's blocklist (`Config::blocklist`) is theirs to edit and existing
//! users keep it as it is. These lists sit under it in
//! [`super::safety_gate::evaluate`], so a user who emptied their blocklist
//! still gets them. They are matched on the app name, bundle id, URL host and
//! window title only; nothing here reads captured text.

use super::Blocklist;

/// Password managers and credential stores, matched as a substring of the app
/// name or bundle id (lowercase).
pub const PASSWORD_MANAGER_MARKERS: &[&str] = &[
    "1password",
    "onepassword",
    "agilebits",
    "bitwarden",
    "keychain",
    "lastpass",
    "dashlane",
    "keepass",
    "nordpass",
    "enpass",
    "proton pass",
    "proton.pass",
    "keepersecurity",
    "roboform",
    "strongbox",
];

/// Apps whose name is a common word, so they match the whole name only.
pub const PASSWORD_MANAGER_EXACT_NAMES: &[&str] = &["passwords"];

/// macOS Passwords app and the settings apps, by bundle id.
pub const SENSITIVE_SYSTEM_BUNDLES: &[&str] = &[
    "com.apple.passwords",
    "com.apple.systempreferences",
    "com.apple.keychainaccess",
];

/// System settings, by app name.
pub const SYSTEM_SETTINGS_NAMES: &[&str] = &["system settings", "system preferences"];

/// Banking and brokerage markers matched anywhere in the URL, kept from the
/// first version of the gate.
pub const BANKING_URL_MARKERS: &[&str] = &[
    "chase.com",
    "bankofamerica",
    "wellsfargo",
    "citibank",
    "capitalone",
    "usbank",
    "fidelity",
    "vanguard",
    "schwab",
    "americanexpress",
    "discover.com",
    "paypal.com",
    "venmo.com",
    "robinhood.com",
];

/// Banking, brokerage, payment and crypto-exchange sites, matched on the URL
/// host and its subdomains. A host match cannot fire on a path or a query that
/// merely mentions the name.
pub const BANKING_HOSTS: &[&str] = &[
    "citi.com",
    "ally.com",
    "sofi.com",
    "chime.com",
    "pnc.com",
    "tdbank.com",
    "truist.com",
    "regions.com",
    "hsbc.com",
    "hsbc.co.uk",
    "barclays.co.uk",
    "lloydsbank.com",
    "natwest.com",
    "santander.com",
    "santander.co.uk",
    "monzo.com",
    "revolut.com",
    "wise.com",
    "coinbase.com",
    "binance.com",
    "kraken.com",
    "etrade.com",
    "merrilledge.com",
    "tdameritrade.com",
    "interactivebrokers.com",
    "navyfederal.org",
    "usaa.com",
    "cash.app",
    "creditkarma.com",
    "zellepay.com",
];

/// Window-title markers for a banking page.
pub const BANKING_TITLE_MARKERS: &[&str] =
    &["online banking", "mobile banking", "bank account portal"];

/// Markers a browser puts in a private window's title. Matched anywhere in
/// the title of any app, because the words are specific to private browsing.
const PRIVATE_TITLE_MARKERS: &[&str] = &[
    "incognito",
    "inprivate",
    "private browsing",
    "private window",
    "(private)",
];

/// Markers that are too common to trust outside a browser: a document called
/// "Budget - Private" is not a private window, but a browser's title ending
/// in " - Private" is.
const BROWSER_ONLY_PRIVATE_SUFFIXES: &[&str] = &[" - private", " \u{2014} private", " | private"];

/// Whether the frontmost window is a private or incognito browser window, from
/// its title. The title is the only signal FNDR reads: a browser that shows no
/// marker in the title of a private window cannot be told apart.
pub fn is_private_browser_window(
    app_name: &str,
    bundle_id: Option<&str>,
    window_title: &str,
) -> bool {
    let title = window_title.trim().to_lowercase();
    if title.is_empty() {
        return false;
    }
    if PRIVATE_TITLE_MARKERS.iter().any(|m| title.contains(m)) {
        return true;
    }
    let is_browser =
        crate::browsers::is_browser(app_name) || bundle_id.is_some_and(crate::browsers::is_browser);
    is_browser
        && BROWSER_ONLY_PRIVATE_SUFFIXES
            .iter()
            .any(|suffix| title.ends_with(suffix) || title.contains(&format!("{suffix} - ")))
}

/// Whether the app is a password manager, the Passwords app, Keychain Access
/// or a settings app.
pub fn is_credential_or_settings_app(app_name: &str, bundle_id: Option<&str>) -> bool {
    let app = app_name.trim().to_lowercase();
    let bundle = bundle_id.unwrap_or("").trim().to_lowercase();
    if PASSWORD_MANAGER_MARKERS
        .iter()
        .any(|marker| app.contains(marker) || bundle.contains(marker))
    {
        return true;
    }
    if PASSWORD_MANAGER_EXACT_NAMES.contains(&app.as_str()) {
        return true;
    }
    if SENSITIVE_SYSTEM_BUNDLES
        .iter()
        .any(|id| bundle == *id || bundle.starts_with(&format!("{id}.")))
    {
        return true;
    }
    SYSTEM_SETTINGS_NAMES.contains(&app.as_str())
}

/// Whether the URL's host is one of [`BANKING_HOSTS`] or a subdomain of one.
pub fn is_banking_host(url: Option<&str>) -> bool {
    let Some(host) = Blocklist::context_key(url, None) else {
        return false;
    };
    BANKING_HOSTS
        .iter()
        .any(|bank| host == *bank || host.ends_with(&format!(".{bank}")))
}

/// One row of "what FNDR never stores", for the Privacy Proof screen.
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ExclusionCategory {
    pub id: &'static str,
    pub label: &'static str,
    pub examples: &'static str,
}

/// The built-in categories, in the order the screen lists them.
pub fn categories() -> Vec<ExclusionCategory> {
    vec![
        ExclusionCategory {
            id: "password_managers",
            label: "Password managers and Keychain",
            examples: "1Password, Bitwarden, Dashlane, LastPass, Keychain Access, Passwords",
        },
        ExclusionCategory {
            id: "system_settings",
            label: "System Settings",
            examples: "System Settings, System Preferences",
        },
        ExclusionCategory {
            id: "banking",
            label: "Banking, brokerage and payments",
            examples: "banks, brokerages, PayPal, Venmo, crypto exchanges",
        },
        ExclusionCategory {
            id: "private_windows",
            label: "Private and incognito browser windows",
            examples: "Safari, Chrome, Arc, Dia, Brave, Edge, Firefox",
        },
        ExclusionCategory {
            id: "health_portals",
            label: "Patient and health portals",
            examples: "MyChart, patient portals",
        },
        ExclusionCategory {
            id: "sign_in_pages",
            label: "Sign-in and two-factor pages",
            examples: "pages titled sign in, log in, 2FA, OAuth",
        },
    ]
}
