//! Hybrid search over the Files index: meaning (EmbeddingGemma 2 vectors) plus
//! words (FTS5). Ported from DigUp's `Searcher`, including the choices its
//! evals measured:
//!
//! - Meaning is cosine similarity put on one scale per query: each vector's
//!   score minus its modality's mean, in standard deviations of all of them.
//!   Only segments that stand out (`z_gate`) count as meaning matches, which
//!   also gives an honest "nothing found".
//! - A file qualifies by standing out in meaning, or by holding at least half
//!   the query's words. Score: meaning z plus a bonus that grows with word
//!   coverage, plus 1 for the kind the query asks for. This beat reciprocal
//!   rank fusion on both of DigUp's evals (R@1 0.88 against 0.73).
//! - A query made of codes and numbers weighs words double and meaning half.

use super::store::{from_half_bytes, FilesStore};
use super::{search_text, FileKind, Modality, SegmentKind, DIMENSION};
use half::f16;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize)]
pub struct FileHit {
    pub path: String,
    pub kind: FileKind,
    /// Fused score, for ordering only.
    pub score: f64,
    /// How far the best segment stands out for this query.
    pub z: Option<f64>,
    pub cosine: Option<f64>,
    /// 1-based rank among keyword matches.
    pub keyword_rank: Option<usize>,
    pub segment: SegmentKind,
    pub segment_id: i64,
    /// PDF page, or seconds into a recording or video.
    pub loc: Option<f64>,
    pub loc_end: Option<f64>,
    pub excerpt: Option<String>,
    pub modified: f64,
    pub size: i64,
    pub info: serde_json::Value,
    /// Code: the line to open at, in the stretch `loc`..`loc_end`: where the
    /// name looked up is defined, or the first line with the query's words.
    pub focus: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub kinds: Option<HashSet<FileKind>>,
    pub limit: usize,
    pub z_gate: f64,
    pub word_weight: f64,
    /// The last word is still being typed: match it as a prefix.
    pub prefix_last: bool,
    /// This is the code index: names are looked up as names.
    pub code: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            kinds: None,
            limit: 20,
            z_gate: 2.0,
            word_weight: WORD_WEIGHT,
            prefix_last: false,
            code: false,
        }
    }
}

/// How much the query's words count against meaning. Code's words count for
/// less: they are everywhere in code.
pub const WORD_WEIGHT: f64 = 2.0;
pub const CODE_WORD_WEIGHT: f64 = 1.0;

/// The vectors, loaded once and scored by brute force.
pub struct Searcher {
    matrix: Vec<f16>,
    row_file: Vec<i64>,
    row_segment: Vec<i64>,
    row_modality: Vec<Modality>,
    /// False for the later pages of long files: scored, but they do not set the scale.
    row_sets_scale: Vec<bool>,
    file_kind: HashMap<i64, FileKind>,
}

impl Searcher {
    pub fn load(store: &FilesStore) -> rusqlite::Result<Self> {
        let mut searcher = Self {
            matrix: Vec::new(),
            row_file: Vec::new(),
            row_segment: Vec::new(),
            row_modality: Vec::new(),
            row_sets_scale: Vec::new(),
            file_kind: HashMap::new(),
        };
        {
            let mut statement = store.db.prepare("SELECT id, kind FROM files")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (id, kind) = row?;
                if let Some(kind) = FileKind::parse(&kind) {
                    searcher.file_kind.insert(id, kind);
                }
            }
        }
        let options = super::IndexOptions::default();
        let mut statement = store.db.prepare(
            "SELECT s.id, s.file, s.modality, s.vec, s.kind, s.loc, f.kind, CAST(json_extract(f.info, '$.pages') AS INTEGER)
             FROM segments s JOIN files f ON f.id = s.file WHERE s.vec IS NOT NULL ORDER BY s.id",
        )?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let blob: Vec<u8> = row.get(3)?;
            if blob.len() != DIMENSION * 2 {
                continue;
            }
            from_half_bytes(&blob, &mut searcher.matrix);
            searcher.row_segment.push(row.get(0)?);
            searcher.row_file.push(row.get(1)?);
            searcher
                .row_modality
                .push(Modality::parse(&row.get::<_, String>(2)?));
            let segment_kind: String = row.get(4)?;
            let place = row.get::<_, Option<f64>>(5)?.unwrap_or(0.0) as usize;
            let later = if row.get::<_, String>(6)? == "pdf" {
                let pages = row.get::<_, Option<i64>>(7)?.unwrap_or(0) as usize;
                segment_kind != "chunk" && pages > options.big_pdf_pages && place > options.pdf_step
            } else {
                segment_kind == "chunk" && place > options.doc_step
            };
            searcher.row_sets_scale.push(!later);
        }
        Ok(searcher)
    }

    pub fn vector_count(&self) -> usize {
        self.row_file.len()
    }

    pub fn search(
        &self,
        store: &FilesStore,
        text: &str,
        query_vector: Option<&[f32]>,
        options: &SearchOptions,
    ) -> rusqlite::Result<Vec<FileHit>> {
        let allowed = |file: i64| match &options.kinds {
            None => true,
            Some(kinds) => self
                .file_kind
                .get(&file)
                .is_some_and(|kind| kinds.contains(kind)),
        };

        // Meaning: every vector on this query's scale; each file's best segment.
        let mut best: HashMap<i64, (f64, f64, usize)> = HashMap::new();
        if let Some(query) =
            query_vector.filter(|query| query.len() == DIMENSION && !self.row_file.is_empty())
        {
            let scores = self.score(query);
            let meaning = meaning(&scores, &self.row_modality, Some(&self.row_sets_scale));
            for (row, z) in meaning.iter().enumerate() {
                let file = self.row_file[row];
                if allowed(file) && best.get(&file).is_none_or(|current| *z > current.0) {
                    best.insert(file, (*z, scores[row] as f64, row));
                }
            }
        }

        // Words: FTS5 over names, OCR, page and document text. A name from code
        // (quantize_row_q8_0_ref) is matched as one phrase, as it is written.
        let terms = terms(text);
        let identifier = if options.code {
            super::code::lookup::identifier(text)
        } else {
            None
        };
        let mut patterns = match &identifier {
            Some(words) => vec![super::code::lookup::phrase(words)],
            None => patterns(&terms, options.prefix_last),
        };
        // A word that is in no file as typed matches as the start of longer words.
        if identifier.is_none() {
            for (index, term) in terms.iter().enumerate() {
                if term.chars().count() >= 3
                    && !patterns[index].ends_with('*')
                    && !is_indexed(store, &patterns[index])?
                {
                    patterns[index].push('*');
                }
            }
        }
        let mut words: Vec<(i64, i64, Option<String>)> = Vec::new();
        let mut coverage: HashMap<i64, f64> = HashMap::new();
        if !patterns.is_empty() {
            let mut seen = HashSet::new();
            let mut statement = store.db.prepare(
                "SELECT s.file, s.id, snippet(fts, 1, '\u{AB}', '\u{BB}', '\u{2026}', 12), fts.original FROM fts
                 JOIN segments s ON s.id = fts.rowid
                 WHERE fts MATCH ? ORDER BY bm25(fts, 3.0, 1.0) LIMIT 1000",
            )?;
            let mut rows = statement.query([patterns.join(" OR ")])?;
            while let Some(row) = rows.next()? {
                let file: i64 = row.get(0)?;
                if words.len() >= 100 || !allowed(file) || !seen.insert(file) {
                    continue;
                }
                let original: Option<String> = row.get(3)?;
                let snippet = original
                    .and_then(|original| search_text::snippet(&original, &terms, 12))
                    .or(row.get(2)?);
                words.push((file, row.get(1)?, snippet));
            }
            if !words.is_empty() {
                let files = words
                    .iter()
                    .map(|(file, _, _)| file.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let query = format!(
                    "SELECT DISTINCT s.file FROM fts JOIN segments s ON s.id = fts.rowid WHERE fts MATCH ? AND s.file IN ({files})"
                );
                for pattern in &patterns {
                    let mut statement = store.db.prepare(&query)?;
                    let mut rows = statement.query([pattern])?;
                    while let Some(row) = rows.next()? {
                        *coverage.entry(row.get(0)?).or_insert(0.0) += 1.0 / patterns.len() as f64;
                    }
                }
            }
        }

        let lookup = is_lookup(&terms) || identifier.is_some();
        let (meaning_weight, words_weight) = if lookup {
            (0.5, options.word_weight * 2.0)
        } else {
            (1.0, options.word_weight)
        };
        let hinted = kind_hints(text);
        let mut candidates: HashSet<i64> = best
            .iter()
            .filter(|(_, value)| value.0 >= options.z_gate)
            .map(|(file, _)| *file)
            .collect();
        candidates.extend(
            coverage
                .iter()
                .filter(|(_, share)| **share >= 0.5)
                .map(|(file, _)| *file),
        );
        let mut scored: Vec<(i64, f64)> = candidates
            .into_iter()
            .map(|file| {
                let z = best.get(&file).map_or(0.0, |value| value.0.max(0.0));
                let mut score = meaning_weight * z
                    + words_weight * coverage.get(&file).copied().unwrap_or(0.0).powf(1.5);
                if self
                    .file_kind
                    .get(&file)
                    .is_some_and(|kind| hinted.contains(kind))
                {
                    score += 1.0;
                }
                (file, score)
            })
            .collect();
        let mut word_rank: HashMap<i64, (usize, i64, Option<String>)> = HashMap::new();
        for (rank, (file, segment, snippet)) in words.into_iter().enumerate() {
            word_rank
                .entry(file)
                .or_insert((rank + 1, segment, snippet));
        }
        // Equal scores go to the better word match (a definition over its callers), then the older file.
        let rank_of = |file: &i64| word_rank.get(file).map_or(usize::MAX, |rank| rank.0);
        scored.sort_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then(rank_of(&a.0).cmp(&rank_of(&b.0)))
                .then(a.0.cmp(&b.0))
        });
        let mut hits = Vec::new();
        for (file, score) in scored.into_iter().take(options.limit) {
            let semantic = best.get(&file);
            let keyword = word_rank.get(&file);
            // Point at the best meaning segment if it stands out; else at the segment the words matched.
            let use_meaning =
                semantic.is_some_and(|value| value.0 >= options.z_gate || keyword.is_none());
            let mut segment_id = match (use_meaning, semantic, keyword) {
                (true, Some(value), _) => self.row_segment[value.2],
                (_, _, Some(keyword)) => keyword.1,
                _ => continue,
            };
            let mut focus = None;
            if options.code {
                // Where a name is defined is looked for in the first 10 results only: a name
                // used all over a big repo is in hundreds of stretches of every file that has it.
                let words_here =
                    keyword.is_some_and(|keyword| keyword.1 == segment_id) || identifier.is_some();
                let looked_up = identifier.as_deref().filter(|_| hits.len() < 10);
                (segment_id, focus) =
                    code_place(store, file, segment_id, looked_up, text, &terms, words_here)?;
            }
            let Some(mut hit) = self.hit(store, file, segment_id, score, semantic, keyword)? else {
                continue;
            };
            hit.focus = focus;
            hits.push(hit);
        }
        Ok(hits)
    }

    fn hit(
        &self,
        store: &FilesStore,
        file: i64,
        segment_id: i64,
        score: f64,
        semantic: Option<&(f64, f64, usize)>,
        keyword: Option<&(usize, i64, Option<String>)>,
    ) -> rusqlite::Result<Option<FileHit>> {
        let facts: Option<(String, String, f64, i64, Option<String>)> = store
            .db
            .query_row(
                "SELECT path, kind, mtime, size, info FROM files WHERE id = ?",
                [file],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((path, kind, modified, size, info)) = facts else {
            return Ok(None);
        };
        let (segment, loc, loc_end, stored_excerpt): (
            String,
            Option<f64>,
            Option<f64>,
            Option<String>,
        ) = store.db.query_row(
            "SELECT kind, loc, loc_end, excerpt FROM segments WHERE id = ?",
            params![segment_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        // Quote the matched words only when they come from the segment pointed at,
        // so a PDF hit on page 5 never shows page 2's words.
        let marked =
            |snippet: &Option<String>| snippet.clone().filter(|text| text.contains('\u{AB}'));
        let mut snippet = keyword
            .filter(|keyword| keyword.1 == segment_id)
            .and_then(|keyword| marked(&keyword.2));
        // A segment with no place and no text of its own (a screenshot's picture) may
        // borrow its file's OCR words, which describe the same picture.
        if snippet.is_none() && stored_excerpt.is_none() && loc.is_none() {
            if let Some(keyword) = keyword {
                let matched_kind: Option<String> = store
                    .db
                    .query_row(
                        "SELECT kind FROM segments WHERE id = ?",
                        [keyword.1],
                        |row| row.get(0),
                    )
                    .optional()?;
                if matched_kind.as_deref() != Some("name") {
                    snippet = marked(&keyword.2);
                }
            }
        }
        Ok(Some(FileHit {
            path,
            kind: FileKind::parse(&kind).unwrap_or(FileKind::Doc),
            score,
            z: semantic.map(|value| value.0),
            cosine: semantic.map(|value| value.1),
            keyword_rank: keyword.map(|keyword| keyword.0),
            segment: SegmentKind::parse(&segment).unwrap_or(SegmentKind::Name),
            segment_id,
            loc,
            loc_end,
            excerpt: snippet
                .map(|text| search_text::excerpt(&text, 400))
                .or(stored_excerpt),
            modified,
            size,
            info: info
                .and_then(|info| serde_json::from_str(&info).ok())
                .unwrap_or(serde_json::Value::Null),
            focus: None,
        }))
    }

    /// Cosine similarity of `query` with every stored vector (all L2-normalized).
    fn score(&self, query: &[f32]) -> Vec<f32> {
        self.matrix
            .chunks_exact(DIMENSION)
            .map(|row| {
                row.iter()
                    .zip(query)
                    .map(|(value, q)| value.to_f32() * q)
                    .sum()
            })
            .collect()
    }
}

/// The text of a segment as written: a page, a passage, a stretch of code.
pub fn segment_text(store: &FilesStore, segment: i64) -> rusqlite::Result<Option<String>> {
    store
        .db
        .query_row(
            "SELECT coalesce(original, body) FROM fts WHERE rowid = ?",
            [segment],
            |row| row.get(0),
        )
        .optional()
}

/// Code: the stretch to point at and the line in it to open at. A name looked
/// up points at the stretch that defines it, when one does; otherwise the line
/// is the first with the query's words, when they matched in this stretch.
fn code_place(
    store: &FilesStore,
    file: i64,
    segment: i64,
    identifier: Option<&[String]>,
    query: &str,
    terms: &[String],
    words_here: bool,
) -> rusqlite::Result<(i64, Option<f64>)> {
    use super::code::lookup;
    let name = query.trim().replace("()", "");
    if let Some(words) = identifier {
        let mut best: Option<(i64, usize, u8)> = None;
        let mut statement = store.db.prepare(
            "SELECT s.id, s.loc, coalesce(fts.original, fts.body) FROM fts JOIN segments s ON s.id = fts.rowid
             WHERE fts MATCH ? AND s.file = ? ORDER BY s.loc LIMIT 300",
        )?;
        let mut rows = statement.query(params![lookup::phrase(words), file])?;
        while let Some(row) = rows.next()? {
            if best.is_some_and(|best| best.2 >= 2) {
                break;
            }
            let Some(text) = row.get::<_, Option<String>>(2)? else {
                continue;
            };
            let first = row.get::<_, Option<f64>>(1)?.unwrap_or(1.0) as usize;
            if let Some((line, strength)) = lookup::definition(&name, &text, first) {
                if strength > best.map_or(0, |best| best.2) {
                    best = Some((row.get(0)?, line, strength));
                }
            }
        }
        if let Some((segment, line, _)) = best {
            return Ok((segment, Some(line as f64)));
        }
    }
    if !words_here {
        return Ok((segment, None));
    }
    let Some(text) = segment_text(store, segment)? else {
        return Ok((segment, None));
    };
    let first: f64 = store
        .db
        .query_row("SELECT loc FROM segments WHERE id = ?", [segment], |row| {
            row.get::<_, Option<f64>>(0)
        })?
        .unwrap_or(1.0);
    let words: Vec<String> = if identifier.is_some() {
        vec![name.to_lowercase()]
    } else {
        terms.to_vec()
    };
    Ok((
        segment,
        lookup::first_line(&words, &text, first as usize).map(|line| line as f64),
    ))
}

/// This query's meaning scores: each score minus its modality's mean, in
/// standard deviations of all of them together. Scaling each modality by its
/// own deviation instead inflated the narrow ones (DigUp 2026-10-08: 91 to 94
/// of 105 first). The scale comes from rows in `sets_scale` only, so a book's
/// hundreds of later pages do not pull the text mean down.
pub fn meaning(scores: &[f32], modality: &[Modality], sets_scale: Option<&[bool]>) -> Vec<f64> {
    let counts = |row: usize| sets_scale.is_none_or(|scale| scale[row]);
    let means = modality_means(scores, modality, sets_scale);
    let centered: Vec<f64> = scores
        .iter()
        .zip(modality)
        .map(|(score, modality)| *score as f64 - means.get(modality).copied().unwrap_or(0.0))
        .collect();
    let rows: Vec<f64> = centered
        .iter()
        .enumerate()
        .filter(|(row, _)| counts(*row))
        .map(|(_, value)| *value)
        .collect();
    let count = rows.len().max(1) as f64;
    let mean = rows.iter().sum::<f64>() / count;
    let variance = rows.iter().map(|value| (value - mean).powi(2)).sum::<f64>() / count;
    let deviation = variance.sqrt().max(0.02);
    centered
        .iter()
        .map(|value| (value - mean) / deviation)
        .collect()
}

/// Each modality's mean score for this query. Groups under 20 vectors take the overall mean.
pub fn modality_means(
    scores: &[f32],
    modality: &[Modality],
    sets_scale: Option<&[bool]>,
) -> HashMap<Modality, f64> {
    let mut sum: HashMap<Modality, f64> = HashMap::new();
    let mut count: HashMap<Modality, usize> = HashMap::new();
    for row in 0..scores.len() {
        if sets_scale.is_none_or(|scale| scale[row]) {
            *sum.entry(modality[row]).or_insert(0.0) += scores[row] as f64;
            *count.entry(modality[row]).or_insert(0) += 1;
        }
    }
    let total: usize = count.values().sum();
    let all = sum.values().sum::<f64>() / total.max(1) as f64;
    Modality::ALL
        .into_iter()
        .map(|modality| {
            let mean = match count.get(&modality) {
                Some(n) if *n >= 20 => sum[&modality] / *n as f64,
                _ => all,
            };
            (modality, mean)
        })
        .collect()
}

/// How many of `hits` (best first) score at least `ratio` times the top hit.
/// Surfaces fold the rest under "weaker matches". On DigUp's evals 0.5 cut
/// lists from 26.2 to 5.7 results and never folded an expected answer.
pub fn strong_count(hits: &[FileHit], ratio: f64) -> usize {
    match hits.first() {
        Some(top) if top.score > 0.0 => hits
            .iter()
            .take_while(|hit| hit.score >= top.score * ratio)
            .count(),
        _ => hits.len(),
    }
}

const STOPWORDS: &[&str] = &[
    "a",
    "an",
    "the",
    "of",
    "in",
    "on",
    "at",
    "to",
    "for",
    "with",
    "and",
    "or",
    "my",
    "me",
    "that",
    "this",
    "these",
    "those",
    "from",
    "about",
    "where",
    "what",
    "which",
    "who",
    "when",
    "is",
    "was",
    "are",
    "were",
    "be",
    "it",
    "its",
    "some",
    "any",
    "find",
    "show",
    "file",
    "files",
    "photo",
    "photos",
    "picture",
    "image",
    "images",
    "screenshot",
    "screenshots",
    "video",
    "videos",
    "pdf",
    "document",
    "documents",
    "recording",
    "\u{098F}\u{09AC}\u{0982}",
    "\u{0993}",
    "\u{098F}\u{0987}",
    "\u{09B8}\u{09C7}\u{0987}",
    "\u{099B}\u{09AC}\u{09BF}",
    "\u{09AD}\u{09BF}\u{09A1}\u{09BF}\u{0993}",
    "\u{09AB}\u{09BE}\u{0987}\u{09B2}",
    "\u{0641}\u{064A}",
    "\u{0645}\u{0646}",
    "\u{0639}\u{0644}\u{0649}",
    "\u{0639}\u{0646}",
    "\u{0645}\u{0639}",
    "\u{0635}\u{0648}\u{0631}\u{0629}",
    "\u{0641}\u{064A}\u{062F}\u{064A}\u{0648}",
    "\u{0645}\u{0644}\u{0641}",
];

/// The content words of a query, normalized as keyword search keeps text. Kind
/// words and stopwords describe what to look for, not what is in it.
pub fn terms(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    search_text::normalized(text)
        .to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || super::search_text::inflects(&c.to_string())))
        .filter(|word| word.chars().count() >= 2 && !STOPWORDS.contains(word))
        .filter(|word| seen.insert(word.to_string()))
        .map(str::to_string)
        .collect()
}

/// FTS5 patterns for the query's words: quoted, and as prefixes for the last
/// word while it is typed and for words of inflecting scripts.
pub fn patterns(terms: &[String], prefix_last: bool) -> Vec<String> {
    terms
        .iter()
        .enumerate()
        .map(|(index, term)| {
            let prefix = (prefix_last && index + 1 == terms.len()) || search_text::inflects(term);
            format!(
                "\"{}\"{}",
                term.replace('"', ""),
                if prefix { "*" } else { "" }
            )
        })
        .collect()
}

fn is_indexed(store: &FilesStore, pattern: &str) -> rusqlite::Result<bool> {
    store
        .db
        .query_row(
            "SELECT 1 FROM fts WHERE fts MATCH ? LIMIT 1",
            [pattern],
            |_| Ok(()),
        )
        .optional()
        .map(|found| found.is_some())
}

/// Codes and IDs (`K7Q2LM`, `TRX50`) and long numbers mean nothing to an embedding.
pub fn is_lookup(terms: &[String]) -> bool {
    !terms.is_empty()
        && terms.iter().all(|term| {
            term.chars().any(char::is_numeric)
                && (term.chars().any(char::is_alphabetic) || term.chars().count() >= 4)
        })
}

const KIND_WORDS: &[(&str, FileKind)] = &[
    ("screenshot", FileKind::Screenshot),
    ("screenshots", FileKind::Screenshot),
    ("photo", FileKind::Image),
    ("photos", FileKind::Image),
    ("picture", FileKind::Image),
    ("pictures", FileKind::Image),
    ("image", FileKind::Image),
    ("images", FileKind::Image),
    ("pdf", FileKind::Pdf),
    ("pdfs", FileKind::Pdf),
    ("paper", FileKind::Pdf),
    ("papers", FileKind::Pdf),
    ("document", FileKind::Doc),
    ("documents", FileKind::Doc),
    ("note", FileKind::Doc),
    ("notes", FileKind::Doc),
    ("video", FileKind::Video),
    ("videos", FileKind::Video),
    ("clip", FileKind::Video),
    ("movie", FileKind::Video),
    ("audio", FileKind::Audio),
    ("song", FileKind::Audio),
    ("podcast", FileKind::Audio),
    ("voice", FileKind::Audio),
    ("sound", FileKind::Audio),
    ("\u{099B}\u{09AC}\u{09BF}", FileKind::Image),
    ("\u{09AD}\u{09BF}\u{09A1}\u{09BF}\u{0993}", FileKind::Video),
    ("\u{0635}\u{0648}\u{0631}\u{0629}", FileKind::Image),
    ("\u{0641}\u{064A}\u{062F}\u{064A}\u{0648}", FileKind::Video),
];

/// Kinds the query asks for ("in a video", "screenshot of"): a soft boost, not a
/// filter. An inflecting kind word also counts at the start of a longer word.
pub fn kind_hints(text: &str) -> HashSet<FileKind> {
    search_text::normalized(text)
        .to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || search_text::inflects(&c.to_string())))
        .filter_map(|word| {
            KIND_WORDS
                .iter()
                .find(|(key, _)| *key == word)
                .map(|(_, kind)| *kind)
                .or_else(|| {
                    KIND_WORDS
                        .iter()
                        .find(|(key, _)| {
                            search_text::inflects(key)
                                && key.chars().count() >= 3
                                && word.starts_with(key)
                        })
                        .map(|(_, kind)| *kind)
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::store::{FileFacts, NewSegment};
    use super::*;

    fn vector(direction: &[(usize, f32)]) -> Vec<f32> {
        let mut out = vec![0.0; DIMENSION];
        for (index, value) in direction {
            out[*index] = *value;
        }
        let norm = out.iter().map(|v| v * v).sum::<f32>().sqrt();
        out.iter().map(|v| v / norm).collect()
    }

    /// A small index: one document about a pet clause, many unrelated ones,
    /// one picture, one file with an invoice code.
    fn fixture() -> FilesStore {
        let store = FilesStore::open_in_memory().unwrap();
        let add = |path: &str,
                   kind: FileKind,
                   text: &str,
                   modality: Modality,
                   vec: Vec<f32>,
                   inode: u64| {
            let (id, _) = store
                .upsert_file(&FileFacts {
                    path: path.into(),
                    kind,
                    size: 1,
                    mtime: 1.0,
                    inode,
                    device: 1,
                })
                .unwrap();
            store.add_name(id, path).unwrap();
            let segment_kind = if kind == FileKind::Image {
                SegmentKind::Image
            } else {
                SegmentKind::Chunk
            };
            store
                .add_segment(
                    id,
                    &NewSegment {
                        kind: segment_kind,
                        modality,
                        loc: Some(1.0),
                        loc_end: None,
                        excerpt: None,
                        text: (!text.is_empty()).then_some(text),
                        vector: Some(&vec),
                    },
                )
                .unwrap();
        };
        add(
            "/d/lease.pdf",
            FileKind::Pdf,
            "Pets other than one cat need written consent.",
            Modality::Text,
            vector(&[(0, 1.0), (1, 0.2)]),
            1,
        );
        for n in 0..30 {
            add(
                &format!("/d/misc{n}.md"),
                FileKind::Doc,
                "quarterly planning notes",
                Modality::Text,
                vector(&[(2 + n % 5, 1.0), (1, 0.3)]),
                100 + n as u64,
            );
        }
        add(
            "/d/beach.jpg",
            FileKind::Image,
            "",
            Modality::Image,
            vector(&[(40, 1.0), (0, 0.3)]),
            2,
        );
        add(
            "/d/invoice.md",
            FileKind::Doc,
            "Invoice K7Q2LM for the studio rent.",
            Modality::Text,
            vector(&[(3, 1.0)]),
            3,
        );
        store
    }

    #[test]
    fn meaning_finds_the_file_that_stands_out() {
        let store = fixture();
        let searcher = Searcher::load(&store).unwrap();
        let hits = searcher
            .search(
                &store,
                "where can I keep a dog",
                Some(&vector(&[(0, 1.0)])),
                &SearchOptions::default(),
            )
            .unwrap();
        assert_eq!(hits[0].path, "/d/lease.pdf");
        assert!(hits[0].z.unwrap() >= 2.0);
        assert_eq!(hits[0].loc, Some(1.0));
    }

    #[test]
    fn nothing_stands_out_means_nothing_found() {
        let store = fixture();
        let searcher = Searcher::load(&store).unwrap();
        let hits = searcher
            .search(
                &store,
                "zzz qqq",
                Some(&vector(&[(700, 1.0)])),
                &SearchOptions::default(),
            )
            .unwrap();
        assert!(hits.is_empty(), "{hits:?}");
    }

    #[test]
    fn exact_codes_win_by_words() {
        let store = fixture();
        let searcher = Searcher::load(&store).unwrap();
        let hits = searcher
            .search(&store, "K7Q2LM", None, &SearchOptions::default())
            .unwrap();
        assert_eq!(hits[0].path, "/d/invoice.md");
        assert!(hits[0]
            .excerpt
            .as_deref()
            .unwrap()
            .contains("\u{AB}K7Q2LM\u{BB}"));
    }

    #[test]
    fn a_partial_word_matches_as_a_prefix() {
        let store = fixture();
        let searcher = Searcher::load(&store).unwrap();
        let hits = searcher
            .search(&store, "invoi", None, &SearchOptions::default())
            .unwrap();
        assert_eq!(hits[0].path, "/d/invoice.md");
    }

    #[test]
    fn kind_filters_apply() {
        let store = fixture();
        let searcher = Searcher::load(&store).unwrap();
        let only_images = SearchOptions {
            kinds: Some([FileKind::Image].into()),
            ..SearchOptions::default()
        };
        let hits = searcher
            .search(&store, "pets", Some(&vector(&[(0, 1.0)])), &only_images)
            .unwrap();
        assert!(hits.iter().all(|hit| hit.kind == FileKind::Image));
    }

    #[test]
    fn the_scale_centers_each_modality() {
        let mut scores = vec![0.8_f32; 20];
        scores.extend(vec![0.5_f32; 20]);
        let mut modality = vec![Modality::Text; 20];
        modality.extend(vec![Modality::Image; 20]);
        scores[0] = 0.9;
        scores[20] = 0.6;
        let z = meaning(&scores, &modality, None);
        // Both stand out by the same amount from their own modality.
        assert!((z[0] - z[20]).abs() < 1e-5);
        assert!(z[0] > z[1]);
    }

    #[test]
    fn small_modalities_use_the_overall_mean() {
        let means = modality_means(&[1.0, 0.0], &[Modality::Text, Modality::Image], None);
        assert_eq!(means[&Modality::Text], 0.5);
        assert_eq!(means[&Modality::Audio], 0.5);
    }

    #[test]
    fn query_words() {
        assert_eq!(
            terms("the clause about pets in the lease"),
            ["clause", "pets", "lease"]
        );
        assert_eq!(
            terms("screenshot of payment declined error"),
            ["payment", "declined", "error"]
        );
        assert!(is_lookup(&terms("K7Q2LM")));
        assert!(is_lookup(&terms("4815162342")));
        assert!(!is_lookup(&terms("q3 planning")));
        assert_eq!(
            patterns(&terms("dog beach"), true),
            ["\"dog\"", "\"beach\"*"]
        );
        assert_eq!(kind_hints("zebra in a video"), [FileKind::Video].into());
        assert!(kind_hints("zebra").is_empty());
    }

    #[test]
    fn weaker_matches_fold() {
        let hit = |score: f64| FileHit {
            path: String::new(),
            kind: FileKind::Doc,
            score,
            z: None,
            cosine: None,
            keyword_rank: None,
            segment: SegmentKind::Chunk,
            segment_id: 0,
            loc: None,
            loc_end: None,
            excerpt: None,
            modified: 0.0,
            size: 0,
            info: serde_json::Value::Null,
            focus: None,
        };
        assert_eq!(strong_count(&[hit(4.0), hit(2.5), hit(1.0)], 0.5), 2);
        assert_eq!(strong_count(&[], 0.5), 0);
    }
}
