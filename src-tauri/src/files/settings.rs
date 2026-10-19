//! What the person chose for Files search, kept in `files/settings.json`
//! under FNDR's app data. Off until they turn it on (ADR 033).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesSettings {
    pub enabled: bool,
    /// Folders to search, whole.
    pub folders: Vec<String>,
    /// Folders inside them to leave out.
    pub excluded_folders: Vec<String>,
    /// File types to leave out, as people write them ("heic, .txt").
    pub excluded_types: String,
    /// Folders whose code `code:` searches. Empty: code search is off.
    pub code_folders: Vec<String>,
    /// Where code opens: an editor's bundle id, "" for each file's default app,
    /// or `None` for the first installed editor, preferring a running one.
    pub code_editor: Option<String>,
    /// Read folders that look like datasets too.
    pub include_datasets: bool,
    /// Indexing stops until the person resumes it.
    pub paused: bool,
    /// When the first pass and big catch-ups may run; new files are always read.
    pub hours: IndexHours,
    /// Big catch-ups run on battery too (DigUp's default). Off: they wait for power.
    pub index_on_battery: bool,
}

impl Default for FilesSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            folders: Vec::new(),
            excluded_folders: Vec::new(),
            excluded_types: String::new(),
            code_folders: Vec::new(),
            code_editor: None,
            include_datasets: false,
            paused: false,
            hours: IndexHours::Always,
            index_on_battery: true,
        }
    }
}

/// When heavy indexing may run, by local time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IndexHours {
    Always,
    /// From `start` to `end`, in minutes after midnight (0 to 1439); a range
    /// that wraps midnight is fine (22:00 to 07:00). The same start and end
    /// mean all day.
    Between {
        start: u16,
        end: u16,
    },
}

impl IndexHours {
    /// Whether heavy indexing may run at `minute` after midnight.
    pub fn allows(self, minute: u16) -> bool {
        match self {
            IndexHours::Always => true,
            IndexHours::Between { start, end } if start == end => true,
            IndexHours::Between { start, end } if start < end => (start..end).contains(&minute),
            IndexHours::Between { start, end } => minute >= start || minute < end,
        }
    }

    /// "22:00": when indexing may start again, while it waits for the hours.
    pub fn resumes_at(self) -> Option<String> {
        match self {
            IndexHours::Between { start, end } if start != end => {
                Some(format!("{:02}:{:02}", start / 60 % 24, start % 60))
            }
            _ => None,
        }
    }
}

impl FilesSettings {
    pub fn path(app_data_dir: &Path) -> PathBuf {
        app_data_dir.join("files").join("settings.json")
    }

    /// The saved settings, or the defaults (off) when there are none or they are unreadable.
    pub fn load(app_data_dir: &Path) -> Self {
        std::fs::read_to_string(Self::path(app_data_dir))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, app_data_dir: &Path) -> std::io::Result<()> {
        let path = Self::path(app_data_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(temporary, path)
    }

    pub fn roots(&self) -> Vec<PathBuf> {
        self.folders.iter().map(PathBuf::from).collect()
    }

    pub fn index_options(&self) -> super::IndexOptions {
        super::IndexOptions {
            include_datasets: self.include_datasets,
            excluded_folders: self.excluded_folders.clone(),
            excluded_extensions: super::IndexOptions::extensions(&self.excluded_types),
            ..super::IndexOptions::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hours_wrap_midnight() {
        let night = IndexHours::Between {
            start: 22 * 60,
            end: 7 * 60,
        };
        assert!(night.allows(23 * 60) && night.allows(0) && night.allows(6 * 60 + 59));
        assert!(!night.allows(7 * 60) && !night.allows(12 * 60));
        assert_eq!(night.resumes_at().as_deref(), Some("22:00"));
        let day = IndexHours::Between {
            start: 9 * 60 + 30,
            end: 17 * 60,
        };
        assert!(!day.allows(9 * 60 + 29) && day.allows(9 * 60 + 30) && !day.allows(17 * 60));
        assert_eq!(day.resumes_at().as_deref(), Some("09:30"));
        assert!(IndexHours::Between {
            start: 300,
            end: 300
        }
        .allows(13 * 60));
        assert!(IndexHours::Always.allows(3 * 60));
        assert_eq!(IndexHours::Always.resumes_at(), None);
    }

    #[test]
    fn older_settings_index_on_battery() {
        let settings: FilesSettings = serde_json::from_str(r#"{"enabled": true}"#).unwrap();
        assert!(settings.index_on_battery);
    }

    #[test]
    fn settings_round_trip_and_default_off() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(FilesSettings::load(dir.path()), FilesSettings::default());
        assert!(!FilesSettings::default().enabled);
        let settings = FilesSettings {
            enabled: true,
            folders: vec!["/Users/a/Documents".into()],
            excluded_types: "HEIC, .txt".into(),
            hours: IndexHours::Between { start: 22, end: 7 },
            ..FilesSettings::default()
        };
        settings.save(dir.path()).unwrap();
        assert_eq!(FilesSettings::load(dir.path()), settings);
        assert_eq!(
            settings.index_options().excluded_extensions,
            ["heic", "txt"]
        );
    }

    #[test]
    fn unreadable_settings_fall_back_to_off() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("files")).unwrap();
        std::fs::write(FilesSettings::path(dir.path()), "{not json").unwrap();
        assert!(!FilesSettings::load(dir.path()).enabled);
    }
}
