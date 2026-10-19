//! The Files index on disk: one SQLite file with files, their segments (each
//! with an optional fp16 vector) and an FTS5 table. Vectors live beside the
//! rows that describe them, so a crash never leaves them out of step. At this
//! scale (tens of thousands of vectors) brute-force cosine over an in-memory
//! copy takes milliseconds; no vector database is needed. DigUp's schema 5.

use super::{search_text, FileKind, Modality, SegmentKind, DIMENSION};
use half::f16;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub const SCHEMA_VERSION: i64 = 1;

/// Marks (M*) count as letters, so Bengali vowel signs and Arabic harakat stay
/// inside their words; by default SQLite splits on them.
const TOKENIZER: &str = "porter unicode61 remove_diacritics 2 categories 'L* N* Co M*'";

/// Where a file is in being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    Pending,
    Done,
    /// The first step of a long file is in; the rest comes after everything else.
    Partial,
    Skipped,
    Failed,
}

impl FileState {
    pub fn as_str(self) -> &'static str {
        match self {
            FileState::Pending => "pending",
            FileState::Done => "done",
            FileState::Partial => "partial",
            FileState::Skipped => "skipped",
            FileState::Failed => "failed",
        }
    }
}

/// A file as the crawl found it.
#[derive(Debug, Clone, PartialEq)]
pub struct FileFacts {
    pub path: String,
    pub kind: FileKind,
    pub size: i64,
    pub mtime: f64,
    pub inode: u64,
    pub device: u64,
}

/// A file waiting to be read.
#[derive(Debug, Clone)]
pub struct PendingFile {
    pub id: i64,
    pub path: String,
    pub kind: FileKind,
}

/// One piece to add to the index.
pub struct NewSegment<'a> {
    pub kind: SegmentKind,
    pub modality: Modality,
    pub loc: Option<f64>,
    pub loc_end: Option<f64>,
    pub excerpt: Option<&'a str>,
    /// Words for keyword search, stored normalized.
    pub text: Option<&'a str>,
    /// Meaning, 768 floats, L2-normalized.
    pub vector: Option<&'a [f32]>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize)]
pub struct IndexCounts {
    pub files: i64,
    pub done: i64,
    pub pending: i64,
    pub failed: i64,
    pub vectors: i64,
}

pub struct FilesStore {
    pub(crate) db: Connection,
}

impl FilesStore {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(db: Connection) -> rusqlite::Result<Self> {
        db.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;",
        )?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version == 0 {
            db.execute_batch(&format!(
                "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
                 CREATE TABLE files(
                     id INTEGER PRIMARY KEY,
                     path TEXT NOT NULL UNIQUE,
                     kind TEXT NOT NULL,
                     size INTEGER NOT NULL,
                     mtime REAL NOT NULL,
                     inode INTEGER NOT NULL,
                     device INTEGER NOT NULL,
                     state TEXT NOT NULL DEFAULT 'pending',
                     detail TEXT,
                     info TEXT,
                     indexed_at REAL
                 );
                 CREATE INDEX files_inode ON files(device, inode);
                 CREATE INDEX files_state ON files(state, kind);
                 CREATE TABLE segments(
                     id INTEGER PRIMARY KEY,
                     file INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                     kind TEXT NOT NULL,
                     modality TEXT NOT NULL,
                     loc REAL,
                     loc_end REAL,
                     excerpt TEXT,
                     vec BLOB
                 );
                 CREATE INDEX segments_file ON segments(file);
                 CREATE VIRTUAL TABLE fts USING fts5(name, body, original UNINDEXED, tokenize = \"{TOKENIZER}\");
                 PRAGMA user_version = {SCHEMA_VERSION};"
            ))?;
        }
        Ok(Self { db })
    }

    pub fn meta(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.db
            .query_row("SELECT value FROM meta WHERE key = ?", [key], |row| {
                row.get(0)
            })
            .optional()
    }

    pub fn set_meta(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.db
            .execute("INSERT INTO meta(key, value) VALUES(?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value", [key, value])
            .map(|_| ())
    }

    /// Records a file the crawl found. A new file, or one whose size or
    /// modification time changed, is queued and its old segments go. A file
    /// that moved (same device and inode, new path) keeps its pieces.
    /// Returns the file's id and whether it needs reading.
    pub fn upsert_file(&self, facts: &FileFacts) -> rusqlite::Result<(i64, bool)> {
        let existing: Option<(i64, i64, f64)> = self
            .db
            .query_row(
                "SELECT id, size, mtime FROM files WHERE path = ?",
                [&facts.path],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        if let Some((id, size, mtime)) = existing {
            if size == facts.size && (mtime - facts.mtime).abs() < 1e-3 {
                return Ok((id, false));
            }
            self.clear_segments(id)?;
            self.db.execute(
                "UPDATE files SET kind = ?, size = ?, mtime = ?, inode = ?, device = ?, state = 'pending', detail = NULL, info = NULL WHERE id = ?",
                params![facts.kind.as_str(), facts.size, facts.mtime, facts.inode as i64, facts.device as i64, id],
            )?;
            return Ok((id, true));
        }
        let moved: Option<(i64, String)> = self
            .db
            .query_row(
                "SELECT id, path FROM files WHERE device = ? AND inode = ? AND size = ? AND abs(mtime - ?) < 0.001",
                params![facts.device as i64, facts.inode as i64, facts.size, facts.mtime],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((id, old_path)) = moved {
            if !Path::new(&old_path).exists() {
                self.db.execute(
                    "UPDATE files SET path = ? WHERE id = ?",
                    params![facts.path, id],
                )?;
                self.reindex_name(id, &facts.path)?;
                return Ok((id, false));
            }
        }
        self.db.execute(
            "INSERT INTO files(path, kind, size, mtime, inode, device) VALUES(?, ?, ?, ?, ?, ?)",
            params![
                facts.path,
                facts.kind.as_str(),
                facts.size,
                facts.mtime,
                facts.inode as i64,
                facts.device as i64
            ],
        )?;
        Ok((self.db.last_insert_rowid(), true))
    }

    /// Removes files the crawl of `roots` no longer found under them.
    pub fn remove_missing(
        &self,
        roots: &[String],
        found: &std::collections::HashSet<String>,
    ) -> rusqlite::Result<usize> {
        let mut gone = Vec::new();
        {
            let mut statement = self.db.prepare("SELECT id, path FROM files")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, path) = row?;
                let under_root = roots
                    .iter()
                    .any(|root| path.starts_with(&format!("{root}/")) || &path == root);
                if under_root && !found.contains(&path) {
                    gone.push(id);
                }
            }
        }
        for id in &gone {
            self.remove_file(*id)?;
        }
        Ok(gone.len())
    }

    pub fn remove_file(&self, id: i64) -> rusqlite::Result<()> {
        self.clear_segments(id)?;
        self.db
            .execute("DELETE FROM files WHERE id = ?", [id])
            .map(|_| ())
    }

    /// Removes every file under `folder` (a folder the person stopped searching).
    pub fn remove_folder(&self, folder: &str) -> rusqlite::Result<usize> {
        let ids: Vec<i64> = {
            let mut statement = self
                .db
                .prepare("SELECT id FROM files WHERE path = ? OR substr(path, 1, ?) = ?")?;
            let prefix = format!("{folder}/");
            let rows = statement.query_map(
                params![folder, prefix.chars().count() as i64, prefix],
                |row| row.get(0),
            )?;
            rows.collect::<Result<_, _>>()?
        };
        for id in &ids {
            self.remove_file(*id)?;
        }
        Ok(ids.len())
    }

    pub fn clear_segments(&self, file: i64) -> rusqlite::Result<()> {
        self.db.execute(
            "DELETE FROM fts WHERE rowid IN (SELECT id FROM segments WHERE file = ?)",
            [file],
        )?;
        self.db
            .execute("DELETE FROM segments WHERE file = ?", [file])
            .map(|_| ())
    }

    pub fn add_segment(&self, file: i64, segment: &NewSegment<'_>) -> rusqlite::Result<i64> {
        let vector = segment
            .vector
            .filter(|vector| vector.len() == DIMENSION)
            .map(to_half_bytes);
        let modality = if vector.is_some() {
            segment.modality
        } else {
            Modality::None
        };
        self.db.execute(
            "INSERT INTO segments(file, kind, modality, loc, loc_end, excerpt, vec) VALUES(?, ?, ?, ?, ?, ?, ?)",
            params![file, segment.kind.as_str(), modality.as_str(), segment.loc, segment.loc_end, segment.excerpt, vector],
        )?;
        let id = self.db.last_insert_rowid();
        if let Some(text) = segment.text.filter(|text| !text.trim().is_empty()) {
            self.insert_words(id, "", text)?;
        }
        Ok(id)
    }

    /// The name segment: the file's name and its folders' names, for words only.
    pub fn add_name(&self, file: i64, path: &str) -> rusqlite::Result<i64> {
        self.db.execute(
            "INSERT INTO segments(file, kind, modality) VALUES(?, 'name', 'none')",
            [file],
        )?;
        let id = self.db.last_insert_rowid();
        self.insert_words(id, &name_words(path), "")?;
        Ok(id)
    }

    fn reindex_name(&self, file: i64, path: &str) -> rusqlite::Result<()> {
        let name_segment: Option<i64> = self
            .db
            .query_row(
                "SELECT id FROM segments WHERE file = ? AND kind = 'name'",
                [file],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = name_segment {
            self.db.execute("DELETE FROM fts WHERE rowid = ?", [id])?;
            self.insert_words(id, &name_words(path), "")?;
        }
        Ok(())
    }

    fn insert_words(&self, segment: i64, name: &str, text: &str) -> rusqlite::Result<()> {
        let body = search_text::normalized(text);
        let original = (body != text).then_some(text);
        self.db
            .execute(
                "INSERT INTO fts(rowid, name, body, original) VALUES(?, ?, ?, ?)",
                params![segment, search_text::normalized(name), body, original],
            )
            .map(|_| ())
    }

    pub fn set_state(
        &self,
        file: i64,
        state: FileState,
        detail: Option<&str>,
        info: Option<&serde_json::Value>,
    ) -> rusqlite::Result<()> {
        let info = info.map(|info| info.to_string());
        self.db
            .execute(
                "UPDATE files SET state = ?, detail = ?, info = coalesce(?, info), indexed_at = unixepoch('subsec') WHERE id = ?",
                params![state.as_str(), detail, info, file],
            )
            .map(|_| ())
    }

    /// Files waiting to be read, in the order people look for them most.
    pub fn pending(&self, limit: usize) -> rusqlite::Result<Vec<PendingFile>> {
        let mut statement = self.db.prepare(
            "SELECT id, path, kind FROM files WHERE state = 'pending'
             ORDER BY CASE kind WHEN 'screenshot' THEN 0 WHEN 'image' THEN 1 WHEN 'pdf' THEN 2 WHEN 'doc' THEN 3
                      WHEN 'audio' THEN 4 WHEN 'video' THEN 5 ELSE 6 END, mtime DESC
             LIMIT ?",
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, path, kind) = row?;
            if let Some(kind) = FileKind::parse(&kind) {
                out.push(PendingFile { id, path, kind });
            }
        }
        Ok(out)
    }

    /// What indexing learned about a file (pages, duration, how far a long one is read).
    pub fn info(&self, file: i64) -> rusqlite::Result<serde_json::Value> {
        let text: Option<String> = self
            .db
            .query_row("SELECT info FROM files WHERE id = ?", [file], |row| {
                row.get(0)
            })
            .optional()?
            .flatten();
        Ok(text
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| serde_json::json!({})))
    }

    /// Long files read part way: the rest is read after everything else.
    pub fn partial(&self, limit: usize) -> rusqlite::Result<Vec<PendingFile>> {
        let mut statement = self.db.prepare(
            "SELECT id, path, kind FROM files WHERE state = 'partial' ORDER BY indexed_at LIMIT ?",
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, path, kind) = row?;
            if let Some(kind) = FileKind::parse(&kind) {
                out.push(PendingFile { id, path, kind });
            }
        }
        Ok(out)
    }

    pub fn counts(&self) -> rusqlite::Result<IndexCounts> {
        let mut counts = self.db.query_row(
            "SELECT count(*), coalesce(sum(state IN ('done', 'partial')), 0), coalesce(sum(state = 'pending'), 0),
                    coalesce(sum(state = 'failed'), 0) FROM files",
            [],
            |row| Ok(IndexCounts { files: row.get(0)?, done: row.get(1)?, pending: row.get(2)?, failed: row.get(3)?, vectors: 0 }),
        )?;
        counts.vectors = self.db.query_row(
            "SELECT count(*) FROM segments WHERE vec IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(counts)
    }
}

/// File and folder names as words: separators become spaces, so
/// `Q3_report-final.pdf` matches "q3 report final".
fn name_words(path: &str) -> String {
    let mut parts: Vec<&str> = path.rsplit('/').take(4).collect();
    parts.reverse();
    parts
        .join(" ")
        .chars()
        .map(|c| {
            if c == '_' || c == '-' || c == '.' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

pub fn to_half_bytes(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| f16::from_f32(*value).to_le_bytes())
        .collect()
}

pub fn from_half_bytes(bytes: &[u8], out: &mut Vec<f16>) {
    out.extend(
        bytes
            .chunks_exact(2)
            .map(|pair| f16::from_le_bytes([pair[0], pair[1]])),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(path: &str, size: i64, mtime: f64, inode: u64) -> FileFacts {
        FileFacts {
            path: path.into(),
            kind: FileKind::Doc,
            size,
            mtime,
            inode,
            device: 1,
        }
    }

    fn unit(index: usize) -> Vec<f32> {
        let mut vector = vec![0.0; DIMENSION];
        vector[index] = 1.0;
        vector
    }

    #[test]
    fn a_changed_file_is_read_again_and_an_unchanged_one_is_not() {
        let store = FilesStore::open_in_memory().unwrap();
        let (id, new) = store.upsert_file(&facts("/d/a.md", 10, 1.0, 7)).unwrap();
        assert!(new);
        store
            .add_segment(
                id,
                &NewSegment {
                    kind: SegmentKind::Chunk,
                    modality: Modality::Text,
                    loc: None,
                    loc_end: None,
                    excerpt: None,
                    text: Some("hello"),
                    vector: Some(&unit(0)),
                },
            )
            .unwrap();
        store.set_state(id, FileState::Done, None, None).unwrap();
        assert_eq!(
            store.upsert_file(&facts("/d/a.md", 10, 1.0, 7)).unwrap(),
            (id, false)
        );
        assert_eq!(store.counts().unwrap().vectors, 1);
        assert_eq!(
            store.upsert_file(&facts("/d/a.md", 11, 2.0, 7)).unwrap(),
            (id, true)
        );
        assert_eq!(store.counts().unwrap().vectors, 0);
        assert_eq!(store.pending(10).unwrap().len(), 1);
    }

    #[test]
    fn vectors_round_trip_through_half_precision() {
        let vector: Vec<f32> = (0..DIMENSION)
            .map(|i| (i as f32 / DIMENSION as f32) - 0.5)
            .collect();
        let mut back = Vec::new();
        from_half_bytes(&to_half_bytes(&vector), &mut back);
        assert_eq!(back.len(), DIMENSION);
        assert!(back
            .iter()
            .zip(&vector)
            .all(|(half, full)| (half.to_f32() - full).abs() < 1e-3));
    }

    #[test]
    fn a_vector_of_the_wrong_size_is_not_stored() {
        let store = FilesStore::open_in_memory().unwrap();
        let (id, _) = store.upsert_file(&facts("/d/a.md", 1, 1.0, 1)).unwrap();
        store
            .add_segment(
                id,
                &NewSegment {
                    kind: SegmentKind::Chunk,
                    modality: Modality::Text,
                    loc: None,
                    loc_end: None,
                    excerpt: None,
                    text: None,
                    vector: Some(&[1.0, 0.0]),
                },
            )
            .unwrap();
        let modality: String = store
            .db
            .query_row("SELECT modality FROM segments", [], |row| row.get(0))
            .unwrap();
        assert_eq!(modality, "none");
        assert_eq!(store.counts().unwrap().vectors, 0);
    }

    #[test]
    fn words_are_kept_normalized_and_quoted_as_written() {
        let store = FilesStore::open_in_memory().unwrap();
        let (id, _) = store.upsert_file(&facts("/d/a.md", 1, 1.0, 1)).unwrap();
        let written = "\u{0623}\u{0628}";
        store
            .add_segment(
                id,
                &NewSegment {
                    kind: SegmentKind::Chunk,
                    modality: Modality::None,
                    loc: None,
                    loc_end: None,
                    excerpt: None,
                    text: Some(written),
                    vector: None,
                },
            )
            .unwrap();
        let (body, original): (String, Option<String>) = store
            .db
            .query_row("SELECT body, original FROM fts", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(body, "\u{0627}\u{0628}");
        assert_eq!(original.as_deref(), Some(written));
    }

    #[test]
    fn a_moved_file_keeps_its_pieces() {
        let dir = tempfile::tempdir().unwrap();
        let store = FilesStore::open_in_memory().unwrap();
        let old = dir.path().join("old.md").to_string_lossy().to_string();
        let (id, _) = store.upsert_file(&facts(&old, 5, 3.0, 42)).unwrap();
        store.add_name(id, &old).unwrap();
        // The old path no longer exists on disk, so the same inode at a new path is a move.
        let new = dir.path().join("new.md").to_string_lossy().to_string();
        assert_eq!(
            store.upsert_file(&facts(&new, 5, 3.0, 42)).unwrap(),
            (id, false)
        );
        let hits: i64 = store
            .db
            .query_row(
                "SELECT count(*) FROM fts WHERE fts MATCH 'new'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1);
    }

    #[test]
    fn missing_files_and_dropped_folders_leave_the_index() {
        let store = FilesStore::open_in_memory().unwrap();
        store.upsert_file(&facts("/d/a.md", 1, 1.0, 1)).unwrap();
        store.upsert_file(&facts("/d/b.md", 1, 1.0, 2)).unwrap();
        store.upsert_file(&facts("/dx/c.md", 1, 1.0, 3)).unwrap();
        let found = ["/d/a.md".to_string()].into_iter().collect();
        assert_eq!(
            store.remove_missing(&["/d".to_string()], &found).unwrap(),
            1
        );
        assert_eq!(store.remove_folder("/d").unwrap(), 1);
        assert_eq!(store.counts().unwrap().files, 1);
    }

    #[test]
    fn name_words_split_on_separators() {
        assert_eq!(
            name_words("/Users/a/Documents/Q3_report-final.pdf"),
            "Users a Documents Q3 report final pdf"
        );
    }
}
