use crate::import::*;
use rusqlite::{Connection, OpenFlags};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;

const CHROME_EPOCH_US: i64 = 11_644_473_600_000_000;
const CORE_DATA_EPOCH_MS: i64 = 978_307_200_000;

// Local files, browser internals, and extensions are excluded by allowing HTTP(S) only.
pub const SKIPPED_URL_PATTERNS: &[&str] = &[
    "file:",
    "chrome:",
    "brave:",
    "edge:",
    "arc:",
    "vivaldi:",
    "about:",
    "data:",
    "javascript:",
    "chrome-extension:",
];

pub struct BrowserHistorySource {
    home: PathBuf,
    now_ms: Option<i64>,
}

impl BrowserHistorySource {
    pub fn new(home: PathBuf, now_ms: i64) -> Self {
        Self {
            home,
            now_ms: Some(now_ms),
        }
    }

    pub fn live(home: PathBuf) -> Self {
        Self { home, now_ms: None }
    }

    pub fn for_current_user() -> Option<Self> {
        Some(Self::live(dirs::home_dir()?))
    }

    fn histories(&self) -> Result<Vec<(String, String, PathBuf, bool)>, ImportError> {
        let support = self.home.join("Library/Application Support");
        let families = [
            ("Chrome", "Google/Chrome"),
            ("Brave", "BraveSoftware/Brave-Browser"),
            ("Edge", "Microsoft Edge"),
            ("Arc", "Arc/User Data"),
            ("Dia", "Dia/User Data"),
            ("Vivaldi", "Vivaldi"),
        ];
        let mut found = Vec::new();
        for (label, relative) in families {
            let base = support.join(relative);
            let entries = match fs::read_dir(&base) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(permission_or_io(error, false)),
            };
            for entry in entries {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let profile = entry.file_name().to_string_lossy().to_string();
                if profile.starts_with('.') {
                    continue;
                }
                let history = entry.path().join("History");
                if history.is_file() {
                    found.push((label.into(), profile, history, false));
                }
            }
        }
        let safari = self.home.join("Library/Safari/History.db");
        match fs::symlink_metadata(&safari) {
            Ok(metadata) if metadata.is_file() => {
                found.push(("Safari".into(), "Default".into(), safari, true));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(permission_or_io(error, true)),
        }
        Ok(found)
    }
}

impl Default for BrowserHistorySource {
    fn default() -> Self {
        Self::for_current_user()
            .unwrap_or_else(|| Self::live(PathBuf::from("/nonexistent-fndr-home")))
    }
}

#[derive(Default)]
struct VisitGroup {
    title: String,
    latest_ms: i64,
    count: usize,
    terms: BTreeSet<String>,
}

impl ImportSource for BrowserHistorySource {
    fn kind(&self) -> ImportKind {
        ImportKind::Browser
    }
    fn label(&self) -> &'static str {
        "Browser history"
    }
    fn permissions_needed(&self) -> Vec<Permission> {
        vec![Permission::FullDiskAccess]
    }

    fn scan(
        &self,
        since: Option<Cursor>,
        window_days: u32,
        limit: usize,
    ) -> Result<ScanBatch, ImportError> {
        if limit == 0 {
            return Ok(ScanBatch {
                items: vec![],
                next: None,
                done: true,
            });
        }
        let now_ms = self
            .now_ms
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
        let cutoff = now_ms.saturating_sub(i64::from(window_days).saturating_mul(86_400_000));
        let mut groups: HashMap<(String, String, String), VisitGroup> = HashMap::new();
        for (app, profile, path, safari) in self.histories()? {
            let (conn, _snapshot_dir) = open_snapshot(&path, safari)?;
            if safari {
                read_safari(&conn, cutoff, now_ms, &app, &profile, &mut groups)?;
            } else {
                read_chromium(&conn, cutoff, now_ms, &app, &profile, &mut groups)?;
            }
        }
        let mut items: Vec<_> = groups
            .into_iter()
            .map(|((app, profile, url), group)| {
                let domain = Url::parse(&url)
                    .ok()
                    .and_then(|parsed| parsed.host_str().map(str::to_owned))
                    .unwrap_or_default();
                let title = if group.title.is_empty() {
                    domain.clone()
                } else {
                    group.title
                };
                let mut text = format!("{}\n{}\nVisits: {}", title, domain, group.count);
                if !group.terms.is_empty() {
                    text.push_str("\nSearch terms: ");
                    text.push_str(&group.terms.into_iter().collect::<Vec<_>>().join(", "));
                }
                ImportItem {
                    kind: ImportKind::Browser,
                    external_id: format!("{}:{}:{}", app, profile, url),
                    timestamp_ms: group.latest_ms,
                    title,
                    text: text.chars().take(20_000).collect(),
                    reopen_target: Some(url.clone()),
                    app_label: app,
                    people: vec![],
                    url: Some(url),
                }
            })
            .collect();
        items.sort_by(|a, b| {
            a.timestamp_ms
                .cmp(&b.timestamp_ms)
                .then_with(|| a.external_id.cmp(&b.external_id))
        });
        if let Some(Cursor(cursor)) = since {
            items.retain(|item| cursor_key(item) > cursor);
        }
        let done = items.len() <= limit;
        items.truncate(limit);
        let next = if done {
            None
        } else {
            items.last().map(|item| Cursor(cursor_key(item)))
        };
        Ok(ScanBatch { items, next, done })
    }
}

fn cursor_key(item: &ImportItem) -> String {
    format!("{:019}:{}", item.timestamp_ms, item.external_id)
}

fn open_snapshot(
    path: &Path,
    safari: bool,
) -> Result<(Connection, tempfile::TempDir), ImportError> {
    let temp = tempfile::tempdir()?;
    let snapshot = temp.path().join("History.db");
    fs::copy(path, &snapshot).map_err(|error| permission_or_io(error, safari))?;
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{}", path.display(), suffix));
        if sidecar.exists() {
            fs::copy(&sidecar, temp.path().join(format!("History.db{suffix}")))
                .map_err(|error| permission_or_io(error, safari))?;
        }
    }
    let conn = Connection::open_with_flags(snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| ImportError::InvalidData(format!("{}: {error}", path.display())))?;
    Ok((conn, temp))
}

fn permission_or_io(error: std::io::Error, safari: bool) -> ImportError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        ImportError::PermissionDenied(if safari {
            Permission::FullDiskAccess
        } else {
            Permission::Files
        })
    } else {
        ImportError::from(error)
    }
}

fn read_chromium(
    conn: &Connection,
    cutoff: i64,
    now: i64,
    app: &str,
    profile: &str,
    groups: &mut HashMap<(String, String, String), VisitGroup>,
) -> Result<(), ImportError> {
    let has_terms: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='keyword_search_terms')", [], |row| row.get(0)).map_err(db_error)?;
    let mut stmt = conn.prepare("SELECT u.id, u.url, COALESCE(u.title, ''), v.visit_time FROM visits v JOIN urls u ON u.id = v.url WHERE v.visit_time >= ?1 AND v.visit_time <= ?2 ORDER BY v.visit_time DESC").map_err(db_error)?;
    let from = cutoff.saturating_mul(1000).saturating_add(CHROME_EPOCH_US);
    let to = now.saturating_mul(1000).saturating_add(CHROME_EPOCH_US);
    let rows = stmt
        .query_map([from, to], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(db_error)?;
    let mut ids: HashMap<i64, (String, String, String)> = HashMap::new();
    for row in rows {
        let (id, raw_url, title, raw_time) = row.map_err(db_error)?;
        let Some(url) = clean_url(&raw_url) else {
            continue;
        };
        let key = (app.to_owned(), profile.to_owned(), url);
        let group = groups.entry(key.clone()).or_default();
        let timestamp = (raw_time - CHROME_EPOCH_US) / 1000;
        if timestamp > group.latest_ms {
            group.latest_ms = timestamp;
            group.title = title;
        }
        group.count += 1;
        ids.insert(id, key);
    }
    if has_terms {
        let mut terms = conn
            .prepare("SELECT DISTINCT k.url_id, k.term FROM keyword_search_terms k JOIN visits v ON v.url = k.url_id WHERE v.visit_time >= ?1 AND v.visit_time <= ?2")
            .map_err(db_error)?;
        let rows = terms
            .query_map([from, to], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(db_error)?;
        for row in rows {
            let (id, term) = row.map_err(db_error)?;
            if let Some(key) = ids.get(&id) {
                if !term.trim().is_empty() {
                    if let Some(group) = groups.get_mut(key) {
                        group.terms.insert(term);
                    }
                }
            }
        }
    }
    Ok(())
}

fn read_safari(
    conn: &Connection,
    cutoff: i64,
    now: i64,
    app: &str,
    profile: &str,
    groups: &mut HashMap<(String, String, String), VisitGroup>,
) -> Result<(), ImportError> {
    let mut stmt = conn.prepare("SELECT h.url, COALESCE(v.title, ''), v.visit_time FROM history_visits v JOIN history_items h ON h.id = v.history_item WHERE v.visit_time >= ?1 AND v.visit_time <= ?2 ORDER BY v.visit_time DESC").map_err(db_error)?;
    let from = (cutoff - CORE_DATA_EPOCH_MS) as f64 / 1000.0;
    let to = (now - CORE_DATA_EPOCH_MS) as f64 / 1000.0;
    let rows = stmt
        .query_map([from, to], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })
        .map_err(db_error)?;
    for row in rows {
        let (raw_url, title, raw_time) = row.map_err(db_error)?;
        let Some(url) = clean_url(&raw_url) else {
            continue;
        };
        let group = groups
            .entry((app.to_owned(), profile.to_owned(), url))
            .or_default();
        let timestamp = CORE_DATA_EPOCH_MS + (raw_time * 1000.0) as i64;
        if timestamp > group.latest_ms {
            group.latest_ms = timestamp;
            group.title = title;
        }
        group.count += 1;
    }
    Ok(())
}

fn clean_url(raw: &str) -> Option<String> {
    let mut url = Url::parse(raw).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    url.set_username("").ok()?;
    url.set_password(None).ok()?;
    url.set_fragment(None);
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| {
            let key = key.to_ascii_lowercase();
            !key.starts_with("utm_")
                && !matches!(
                    key.as_str(),
                    "fbclid"
                        | "gclid"
                        | "dclid"
                        | "gbraid"
                        | "wbraid"
                        | "yclid"
                        | "msclkid"
                        | "igshid"
                        | "mc_cid"
                        | "mc_eid"
                        | "ref"
                        | "ref_src"
                )
        })
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    url.set_query(None);
    if !kept.is_empty() {
        url.query_pairs_mut().extend_pairs(kept);
    }
    Some(url.to_string())
}

fn db_error(error: rusqlite::Error) -> ImportError {
    ImportError::InvalidData(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chrome_path(home: &Path) -> PathBuf {
        let path = home.join("Library/Application Support/Google/Chrome/Default/History");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        path
    }

    #[test]
    fn chromium_groups_visits_and_keeps_search_terms() {
        let home = tempfile::tempdir().unwrap();
        let path = chrome_path(home.path());
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT); CREATE TABLE visits (url INTEGER, visit_time INTEGER); CREATE TABLE keyword_search_terms (url_id INTEGER, term TEXT);").unwrap();
        db.execute(
            "INSERT INTO urls VALUES (1, ?1, ?2)",
            [
                "https://alice:secret@example.com/find?q=rust&utm_source=ad#part",
                "Café 🔎",
            ],
        )
        .unwrap();
        db.execute(
            "INSERT INTO keyword_search_terms VALUES (1, 'rust import')",
            [],
        )
        .unwrap();
        for offset in [1000, 2000] {
            db.execute(
                "INSERT INTO visits VALUES (1, ?1)",
                [CHROME_EPOCH_US + (1_760_000_000_000i64 - offset) * 1000],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO urls VALUES (2, 'file:///secret', 'secret')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO visits VALUES (2, ?1)",
            [CHROME_EPOCH_US + 1_760_000_000_000i64 * 1000],
        )
        .unwrap();
        let source = BrowserHistorySource::new(home.path().to_path_buf(), 1_760_000_000_000);
        let batch = source.scan(None, 30, 100).unwrap();
        assert_eq!(batch.items.len(), 1);
        assert_eq!(batch.items[0].title, "Café 🔎");
        assert_eq!(
            batch.items[0].url.as_deref(),
            Some("https://example.com/find?q=rust")
        );
        assert!(batch.items[0].text.contains("Visits: 2"));
        assert!(batch.items[0].text.contains("rust import"));
        assert_eq!(batch.items[0].timestamp_ms, 1_759_999_999_000);
    }

    #[test]
    fn safari_uses_core_data_epoch() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("Library/Safari/History.db");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE history_items (id INTEGER PRIMARY KEY, url TEXT); CREATE TABLE history_visits (history_item INTEGER, visit_time REAL, title TEXT);").unwrap();
        db.execute(
            "INSERT INTO history_items VALUES (1, 'https://example.org/page#part')",
            [],
        )
        .unwrap();
        let timestamp = 1_760_000_000_000i64;
        db.execute(
            "INSERT INTO history_visits VALUES (1, ?1, 'Résumé')",
            [(timestamp - CORE_DATA_EPOCH_MS) as f64 / 1000.0],
        )
        .unwrap();
        let batch = BrowserHistorySource::new(home.path().to_path_buf(), timestamp)
            .scan(None, 30, 10)
            .unwrap();
        assert_eq!(batch.items[0].timestamp_ms, timestamp);
        assert_eq!(batch.items[0].title, "Résumé");
        assert_eq!(batch.items[0].app_label, "Safari");
    }

    #[test]
    fn locked_original_is_copied_and_corrupt_database_is_reported() {
        let home = tempfile::tempdir().unwrap();
        let path = chrome_path(home.path());
        let db = Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE urls (id INTEGER, url TEXT, title TEXT); CREATE TABLE visits (url INTEGER, visit_time INTEGER); BEGIN EXCLUSIVE;").unwrap();
        let source = BrowserHistorySource::new(home.path().to_path_buf(), 1_760_000_000_000);
        assert!(source.scan(None, 30, 10).unwrap().items.is_empty());
        db.execute_batch("ROLLBACK;").unwrap();
        drop(db);
        fs::write(&path, b"not a sqlite database").unwrap();
        assert!(matches!(
            source.scan(None, 30, 10),
            Err(ImportError::InvalidData(_))
        ));
    }

    #[test]
    fn safari_permission_error_is_typed() {
        let error = permission_or_io(
            std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            true,
        );
        assert!(matches!(
            error,
            ImportError::PermissionDenied(Permission::FullDiskAccess)
        ));
    }

    #[test]
    fn discovers_each_chromium_family_and_custom_profile() {
        let home = tempfile::tempdir().unwrap();
        let families = [
            ("Chrome", "Google/Chrome"),
            ("Brave", "BraveSoftware/Brave-Browser"),
            ("Edge", "Microsoft Edge"),
            ("Arc", "Arc/User Data"),
            ("Dia", "Dia/User Data"),
            ("Vivaldi", "Vivaldi"),
        ];
        for (index, (_, relative)) in families.iter().enumerate() {
            let path = home
                .path()
                .join("Library/Application Support")
                .join(relative)
                .join("Custom Profile")
                .join("History");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let db = Connection::open(path).unwrap();
            db.execute_batch("CREATE TABLE urls (id INTEGER, url TEXT, title TEXT); CREATE TABLE visits (url INTEGER, visit_time INTEGER);").unwrap();
            db.execute(
                "INSERT INTO urls VALUES (1, ?1, 'Title')",
                [format!("https://example.com/{index}")],
            )
            .unwrap();
            db.execute(
                "INSERT INTO visits VALUES (1, ?1)",
                [CHROME_EPOCH_US + 1_760_000_000_000i64 * 1000],
            )
            .unwrap();
        }
        let batch = BrowserHistorySource::new(home.path().to_path_buf(), 1_760_000_000_000)
            .scan(None, 30, 10)
            .unwrap();
        assert_eq!(batch.items.len(), families.len());
        for (label, _) in families {
            assert!(batch.items.iter().any(|item| item.app_label == label));
        }
    }

    #[test]
    fn cursor_and_window_limit_results() {
        let home = tempfile::tempdir().unwrap();
        let db = Connection::open(chrome_path(home.path())).unwrap();
        db.execute_batch("CREATE TABLE urls (id INTEGER, url TEXT, title TEXT); CREATE TABLE visits (url INTEGER, visit_time INTEGER);").unwrap();
        for id in 1..=3 {
            db.execute(
                "INSERT INTO urls VALUES (?1, ?2, 'Title')",
                rusqlite::params![id, format!("https://example.com/{id}")],
            )
            .unwrap();
            let age_days = if id == 3 { 40 } else { id };
            db.execute(
                "INSERT INTO visits VALUES (?1, ?2)",
                [
                    id,
                    CHROME_EPOCH_US + (1_760_000_000_000i64 - age_days * 86_400_000) * 1000,
                ],
            )
            .unwrap();
        }
        let source = BrowserHistorySource::new(home.path().to_path_buf(), 1_760_000_000_000);
        let empty = source.scan(None, 30, 0).unwrap();
        assert!(empty.done && empty.items.is_empty());
        let first = source.scan(None, 30, 1).unwrap();
        assert!(!first.done);
        let second = source.scan(first.next, 30, 1).unwrap();
        assert!(second.done);
        assert_ne!(first.items[0].external_id, second.items[0].external_id);
    }

    #[test]
    fn cursor_resume_includes_a_newer_visit() {
        let home = tempfile::tempdir().unwrap();
        let db = Connection::open(chrome_path(home.path())).unwrap();
        db.execute_batch("CREATE TABLE urls (id INTEGER, url TEXT, title TEXT); CREATE TABLE visits (url INTEGER, visit_time INTEGER);").unwrap();
        let now = 1_760_000_000_000i64;
        for id in 1..=3i64 {
            db.execute(
                "INSERT INTO urls VALUES (?1, ?2, 'Title')",
                rusqlite::params![id, format!("https://example.com/{id}")],
            )
            .unwrap();
            db.execute(
                "INSERT INTO visits VALUES (?1, ?2)",
                [id, CHROME_EPOCH_US + (now - id * 60_000) * 1000],
            )
            .unwrap();
        }
        let source = BrowserHistorySource::new(home.path().to_path_buf(), now);
        let first = source.scan(None, 30, 1).unwrap();
        let second = source.scan(first.next, 30, 1).unwrap();
        let saved_cursor = second.next;
        db.execute(
            "INSERT INTO urls VALUES (4, 'https://example.com/new', 'New')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO visits VALUES (4, ?1)",
            [CHROME_EPOCH_US + now * 1000],
        )
        .unwrap();
        let resumed = source.scan(saved_cursor, 30, 10).unwrap();
        assert!(resumed
            .items
            .iter()
            .any(|item| item.url.as_deref() == Some("https://example.com/new")));
    }

    #[test]
    fn live_source_uses_scan_time_for_new_visits() {
        let home = tempfile::tempdir().unwrap();
        let db = Connection::open(chrome_path(home.path())).unwrap();
        db.execute_batch("CREATE TABLE urls (id INTEGER, url TEXT, title TEXT); CREATE TABLE visits (url INTEGER, visit_time INTEGER);").unwrap();
        let source = BrowserHistorySource::live(home.path().to_path_buf());
        assert!(source.scan(None, 30, 10).unwrap().items.is_empty());
        std::thread::sleep(std::time::Duration::from_millis(50));
        let visit_ms = chrono::Utc::now().timestamp_millis();
        db.execute(
            "INSERT INTO urls VALUES (1, 'https://example.com/later', 'Later')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO visits VALUES (1, ?1)",
            [CHROME_EPOCH_US + visit_ms * 1000],
        )
        .unwrap();
        assert_eq!(source.scan(None, 30, 10).unwrap().items.len(), 1);
    }
}
