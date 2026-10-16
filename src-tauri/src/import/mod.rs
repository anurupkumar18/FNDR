//! Imports: the one way past data (browser history, files, calendar, mail,
//! Slack exports, git) becomes memory.
//!
//! A connector implements [`ImportSource`] and only reads: `scan` returns
//! [`ImportItem`]s. It never touches the store, the embedder, or the network.
//! [`ImportRunner`] is the only code that writes. Every item passes
//! [`admit::admit`] first (safety gate, user blocklist, capture surface
//! policy, secret redaction), so a new connector cannot skip privacy
//! admission. Admitted items become leaf rows (`source_type =
//! "import:<kind>"`, session key `import:<kind>:<external id>`): stored whole,
//! found by the normal routes, shown with provenance, deleted; never merged,
//! reviewed, or fed to derived artifacts.
//!
//! `sources/` holds the connectors. `tests::connectors_never_reach_the_store`
//! fails if a file there names the Lance store.

pub mod admit;
pub mod consent;
pub mod cursor;
pub mod sources;

mod record;
mod runner;

pub use runner::{ImportEmbedder, ImportRunner, ImportStatus, RunState, SourceInfo};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Default cap on one item's text, in characters. The runner enforces it.
pub const ITEM_TEXT_MAX_CHARS: usize = 20_000;

/// What an import source is. One connector per kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    Browser,
    File,
    Calendar,
    Mail,
    Slack,
    Git,
}

impl ImportKind {
    pub const ALL: [ImportKind; 6] = [
        ImportKind::Browser,
        ImportKind::File,
        ImportKind::Calendar,
        ImportKind::Mail,
        ImportKind::Slack,
        ImportKind::Git,
    ];

    /// The stable name used in `source_type`, session keys, cursors and IPC.
    pub fn as_str(self) -> &'static str {
        match self {
            ImportKind::Browser => "browser",
            ImportKind::File => "file",
            ImportKind::Calendar => "calendar",
            ImportKind::Mail => "mail",
            ImportKind::Slack => "slack",
            ImportKind::Git => "git",
        }
    }

    pub fn parse(value: &str) -> Option<ImportKind> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == value)
    }

    /// `import:<kind>`, the `source_type` of every row of this kind.
    pub fn source_type(self) -> String {
        format!(
            "{}{}",
            crate::storage::IMPORT_SOURCE_TYPE_PREFIX,
            self.as_str()
        )
    }
}

/// What a source needs the person to have granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    None,
    Files,
    FullDiskAccess,
    Calendar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    Io(String),
    InvalidData(String),
    Command(String),
    /// The OS refused a read; the variant says what to ask the person for.
    PermissionDenied(Permission),
}

impl ImportError {
    /// Plain-language next step for a source error, without source contents.
    pub fn advice(&self) -> &'static str {
        match self {
            ImportError::PermissionDenied(Permission::FullDiskAccess) => {
                "Grant FNDR Full Disk Access in System Settings, then retry."
            }
            ImportError::PermissionDenied(Permission::Calendar) => {
                "Allow Calendar access for FNDR in System Settings, then retry."
            }
            ImportError::PermissionDenied(Permission::Files) => {
                "Choose a readable file or grant FNDR access, then retry."
            }
            ImportError::PermissionDenied(Permission::None) => "Check source access, then retry.",
            ImportError::Command(_) => "Install or sign in to the gh CLI, then retry.",
            ImportError::InvalidData(_) => "Check the source format, then retry.",
            ImportError::Io(_) => "Check that the source is available, then retry.",
        }
    }
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Io(message) => write!(f, "could not read the source: {message}"),
            ImportError::InvalidData(message) => write!(f, "unreadable data: {message}"),
            ImportError::Command(message) => write!(f, "source command failed: {message}"),
            ImportError::PermissionDenied(permission) => {
                write!(f, "permission needed: {permission:?}")
            }
        }
    }
}

/// Keep erased imported memories from being reimported on the next scan.
/// Call this before deleting the rows. Forget clears a source's tombstones.
pub fn tombstone_records(
    state_store: &crate::storage::StateStore,
    records: &[crate::storage::MemoryRecord],
) -> Result<(), String> {
    for kind in ImportKind::ALL {
        let source_type = kind.source_type();
        let ids = records
            .iter()
            .filter(|record| record.source_type == source_type)
            .map(|record| record.id.clone())
            .collect::<Vec<_>>();
        if !ids.is_empty() {
            cursor::add_tombstones(state_store, kind, &ids)?;
        }
    }
    Ok(())
}

impl std::error::Error for ImportError {}

impl From<std::io::Error> for ImportError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::PermissionDenied {
            ImportError::PermissionDenied(Permission::Files)
        } else {
            ImportError::Io(err.to_string())
        }
    }
}

/// One thing a source found. The connector fills it; the runner owns
/// everything after that.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportItem {
    pub kind: ImportKind,
    /// Stable per source item. With `kind` it is the identity of the row.
    pub external_id: String,
    pub timestamp_ms: i64,
    pub title: String,
    /// Plain text, already capped by the connector.
    pub text: String,
    /// File path, URL, `message://` id, permalink, or repo path.
    pub reopen_target: Option<String>,
    /// Shown as the memory's app, e.g. "Chrome", "Mail", "Slack export", "git".
    pub app_label: String,
    pub people: Vec<String>,
    /// Set for anything with a URL, so admission can check it.
    pub url: Option<String>,
}

impl ImportItem {
    /// Hash of what a person would see change: timestamp, title, text, url.
    /// Same `kind` and `external_id` with the same hash is a duplicate; a new
    /// hash is an update of the same row.
    pub fn content_hash(&self) -> String {
        let mut hasher = Sha256::new();
        for part in [
            self.timestamp_ms.to_string().as_str(),
            self.title.as_str(),
            self.text.as_str(),
            self.url.as_deref().unwrap_or(""),
        ] {
            hasher.update(part.as_bytes());
            hasher.update([0u8]);
        }
        hex(&hasher.finalize())
    }

    /// The row id: stable for (kind, external id), so a re-run lands on the
    /// same row.
    pub fn record_id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.kind.as_str().as_bytes());
        hasher.update([0u8]);
        hasher.update(self.external_id.as_bytes());
        format!("imp_{}", &hex(&hasher.finalize())[..32])
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Opaque, connector-defined position. The runner saves it after each batch
/// and hands it back to `scan`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor(pub String);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanBatch {
    pub items: Vec<ImportItem>,
    /// Where to resume. `None` means this scan has no position to save.
    pub next: Option<Cursor>,
    /// No more items in the window. A run ends on the first `done` batch.
    pub done: bool,
}

/// A connector. Read-only, synchronous (the runner calls it on a blocking
/// thread), and free of side effects on the source.
pub trait ImportSource: Send + Sync {
    fn kind(&self) -> ImportKind;

    /// Stable id for IPC and cursors. Defaults to the kind's name.
    fn id(&self) -> &'static str {
        self.kind().as_str()
    }

    fn label(&self) -> &'static str;

    fn permissions_needed(&self) -> Vec<Permission>;

    /// Up to `limit` items newer than `window_days` ago, resuming after
    /// `since` when the runner has one. A connector that cannot resume
    /// returns everything in the window and sets `done`; the runner
    /// deduplicates. `next` is the position after the last item returned.
    fn scan(
        &self,
        since: Option<Cursor>,
        window_days: u32,
        limit: usize,
    ) -> Result<ScanBatch, ImportError>;
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn permission_errors_give_a_specific_next_step() {
        assert!(ImportError::PermissionDenied(Permission::FullDiskAccess)
            .advice()
            .contains("Full Disk Access"));
        assert!(ImportError::PermissionDenied(Permission::Calendar)
            .advice()
            .contains("Calendar"));
        assert!(ImportError::Command("gh unavailable".into())
            .advice()
            .contains("gh"));
    }
}
