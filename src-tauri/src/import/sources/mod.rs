//! Connectors. One file per kind, each implementing `ImportSource`.
//!
//! A connector reads its source and returns `ImportItem`s. It must not name
//! the store, the embedder, or the runner: `import::tests` checks this boundary.
//! Register a connector in [`registered`].

pub mod browser;
pub mod calendar;
pub mod files;
pub mod git;
pub mod mail;
pub mod slack;

use super::ImportSource;
use std::path::PathBuf;
use std::sync::Arc;

/// Scan scope, supplied by import settings before a source is started.
/// Registration only constructs readers; it does not inspect source data.
pub struct SourceOptions {
    pub browser_home: Option<PathBuf>,
    pub file_roots: Option<Vec<PathBuf>>,
    pub calendar_ids: Vec<String>,
    pub mail_inputs: Vec<mail::MailInput>,
    pub selected_mailboxes: Vec<String>,
    pub include_bulk_mail: bool,
    pub slack_export_path: Option<PathBuf>,
    pub git_roots: Vec<PathBuf>,
    pub scan_gh: bool,
}

impl Default for SourceOptions {
    fn default() -> Self {
        Self {
            browser_home: None,
            file_roots: None,
            calendar_ids: Vec::new(),
            mail_inputs: dirs::home_dir()
                .map(|home| mail::MailInput::AppleMail(home.join("Library/Mail")))
                .into_iter()
                .collect(),
            selected_mailboxes: Vec::new(),
            include_bulk_mail: false,
            slack_export_path: None,
            git_roots: Vec::new(),
            scan_gh: false,
        }
    }
}

/// Every connector the app ships, in the order the settings page lists them.
pub fn registered() -> Vec<Arc<dyn ImportSource>> {
    registered_with(&SourceOptions::default())
}

pub fn registered_with(options: &SourceOptions) -> Vec<Arc<dyn ImportSource>> {
    let mut sources: Vec<Arc<dyn ImportSource>> = Vec::new();
    sources.push(Arc::new(match &options.browser_home {
        Some(home) => browser::BrowserHistorySource::live(home.clone()),
        None => browser::BrowserHistorySource::default(),
    }));
    sources.push(Arc::new(match &options.file_roots {
        Some(roots) => files::RecentFilesSource::live(roots.clone()),
        None => files::RecentFilesSource::default(),
    }));
    sources.push(Arc::new(calendar::CalendarImport::new(
        options.calendar_ids.clone(),
    )));
    if !options.mail_inputs.is_empty() {
        sources.push(Arc::new(mail::MailImport::new(
            options.mail_inputs.clone(),
            options.selected_mailboxes.clone(),
            options.include_bulk_mail,
        )));
    }
    if let Some(path) = &options.slack_export_path {
        sources.push(Arc::new(slack::SlackSource { path: path.clone() }));
    }
    sources.push(Arc::new(git::GitSource {
        roots: if options.git_roots.is_empty() {
            git::GitSource::default_home().roots
        } else {
            options.git_roots.clone()
        },
        global_config_override: None,
        scan_gh: options.scan_gh,
    }));
    sources
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::ImportKind;
    use std::path::PathBuf;

    #[test]
    fn registry_is_ordered_and_paths_gate_file_supplied_sources() {
        let defaults = registered_with(&SourceOptions::default());
        assert_eq!(
            defaults
                .iter()
                .map(|source| source.kind())
                .collect::<Vec<_>>(),
            vec![
                ImportKind::Browser,
                ImportKind::File,
                ImportKind::Calendar,
                ImportKind::Mail,
                ImportKind::Git
            ]
        );
        let options = SourceOptions {
            mail_inputs: vec![mail::MailInput::Eml(PathBuf::from("/synthetic/mail.eml"))],
            slack_export_path: Some(PathBuf::from("/synthetic/slack.zip")),
            ..SourceOptions::default()
        };
        let configured = registered_with(&options);
        assert_eq!(
            configured
                .iter()
                .map(|source| source.kind())
                .collect::<Vec<_>>(),
            ImportKind::ALL.to_vec()
        );
    }
}
