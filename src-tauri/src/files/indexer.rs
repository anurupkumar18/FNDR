//! Keeps the Files index in step with the chosen folders: [`sync`] records
//! what the crawl found and drops what is gone; [`index_pending`] reads queued
//! files into segments. Meaning comes from an [`Embedder`], so the real model
//! (a helper process, ADR 033 part 2) and a test fake share one path.

use super::crawl::{self, CrawlReport};
use super::extract;
use super::store::{FileState, FilesStore, NewSegment};
use super::{prompts, search_text, FileKind, IndexOptions, Modality, SegmentKind};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Turns document text into 768-d, L2-normalized vectors.
pub trait Embedder {
    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
}

/// Records what the crawl of `roots` found, queues new and changed files, and
/// removes files that are gone. Reads no file contents.
pub fn sync(
    store: &FilesStore,
    roots: &[PathBuf],
    options: &IndexOptions,
    home: &Path,
) -> rusqlite::Result<CrawlReport> {
    let report = crawl::crawl(roots, options, home);
    let mut found = HashSet::with_capacity(report.candidates.len());
    for facts in &report.candidates {
        let (id, needs_reading) = store.upsert_file(facts)?;
        if needs_reading {
            store.add_name(id, &facts.path)?;
        }
        found.insert(facts.path.clone());
    }
    let roots: Vec<String> = roots
        .iter()
        .map(|root| root.to_string_lossy().to_string())
        .collect();
    store.remove_missing(&roots, &found)?;
    Ok(report)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct IndexRun {
    pub done: usize,
    pub failed: usize,
    /// Queued files of kinds this build cannot read yet.
    pub waiting: usize,
}

/// Reads up to `max_files` queued documents into segments: each passage's words
/// for keyword search and, when it says something in words, its meaning.
pub fn index_pending(
    store: &FilesStore,
    embedder: &mut dyn Embedder,
    options: &IndexOptions,
    max_files: usize,
) -> rusqlite::Result<IndexRun> {
    let mut run = IndexRun::default();
    for file in store.pending(max_files)? {
        if file.kind != FileKind::Doc {
            run.waiting += 1;
            continue;
        }
        match read_document(Path::new(&file.path), embedder, options) {
            Ok(pieces) => {
                let count = pieces.len();
                for (index, (text, vector)) in pieces.iter().enumerate() {
                    let excerpt = search_text::excerpt(text, 240);
                    store.add_segment(
                        file.id,
                        &NewSegment {
                            kind: SegmentKind::Chunk,
                            modality: Modality::Text,
                            loc: Some((index + 1) as f64),
                            loc_end: None,
                            excerpt: Some(&excerpt),
                            text: Some(text),
                            vector: vector.as_deref(),
                        },
                    )?;
                }
                let info = serde_json::json!({ "chunks": count.to_string() });
                store.set_state(file.id, FileState::Done, None, Some(&info))?;
                run.done += 1;
            }
            Err(why) => {
                store.set_state(file.id, FileState::Failed, Some(&why), None)?;
                run.failed += 1;
            }
        }
    }
    Ok(run)
}

/// A passage and, when it has words to mean something, its vector.
type Piece = (String, Option<Vec<f32>>);

/// A document's passages.
fn read_document(
    path: &Path,
    embedder: &mut dyn Embedder,
    options: &IndexOptions,
) -> Result<Vec<Piece>, String> {
    let text = extract::document_text(path).map_err(|error| error.to_string())?;
    let pieces = extract::chunks(&text, 1800, 200, options.max_chunks);
    let title = path.file_stem().and_then(|stem| stem.to_str());
    let worded: Vec<usize> = (0..pieces.len())
        .filter(|index| search_text::has_words(&pieces[*index]))
        .collect();
    let inputs: Vec<String> = worded
        .iter()
        .map(|index| prompts::document(title, &pieces[*index]))
        .collect();
    let vectors = if inputs.is_empty() {
        Vec::new()
    } else {
        embedder.embed_texts(&inputs)?
    };
    if vectors.len() != inputs.len() {
        return Err("the model returned the wrong number of vectors".into());
    }
    let mut out: Vec<Piece> = pieces.into_iter().map(|piece| (piece, None)).collect();
    for (index, vector) in worded.into_iter().zip(vectors) {
        out[index].1 = Some(vector);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::searcher::{SearchOptions, Searcher};
    use super::super::DIMENSION;
    use super::*;

    /// Words hashed into directions: texts sharing words point the same way.
    struct BagOfWords;

    impl BagOfWords {
        fn vector(text: &str) -> Vec<f32> {
            let mut out = vec![0.0_f32; DIMENSION];
            let body = text.rsplit("text: ").next().unwrap_or(text);
            for word in body
                .split(|c: char| !c.is_alphanumeric())
                .filter(|word| word.len() > 2)
            {
                let hash = word.to_lowercase().bytes().fold(7_usize, |hash, byte| {
                    hash.wrapping_mul(31).wrapping_add(byte as usize)
                });
                out[hash % DIMENSION] += 1.0;
            }
            let norm = out.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
            out.iter().map(|v| v / norm).collect()
        }
    }

    impl Embedder for BagOfWords {
        fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            Ok(texts.iter().map(|text| Self::vector(text)).collect())
        }
    }

    #[test]
    fn documents_become_searchable_by_words_and_meaning() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Documents");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("lease.md"),
            "Tenants may keep one cat. Other pets need the landlord's written consent.",
        )
        .unwrap();
        for n in 0..25 {
            std::fs::write(
                root.join(format!("notes{n}.md")),
                format!("Quarterly planning sync number {n}: budget, hiring, roadmap."),
            )
            .unwrap();
        }
        std::fs::write(
            root.join("numbers.txt"),
            "1 2 3 4 5 6 7 8 9 10 11 12 13 14 15",
        )
        .unwrap();
        std::fs::write(root.join("photo.jpg"), b"not decoded yet").unwrap();

        let store = FilesStore::open_in_memory().unwrap();
        let options = IndexOptions::default();
        sync(&store, std::slice::from_ref(&root), &options, dir.path()).unwrap();
        let run = index_pending(&store, &mut BagOfWords, &options, 100).unwrap();
        assert_eq!(
            run,
            IndexRun {
                done: 27,
                failed: 0,
                waiting: 1
            }
        );
        // The table of numbers has words for keyword search but no meaning vector.
        assert_eq!(store.counts().unwrap().vectors, 26);

        let searcher = Searcher::load(&store).unwrap();
        let query = BagOfWords::vector("landlord consent pets");
        let hits = searcher
            .search(
                &store,
                "landlord consent pets",
                Some(&query),
                &SearchOptions::default(),
            )
            .unwrap();
        assert!(hits[0].path.ends_with("lease.md"), "{hits:?}");
        assert_eq!(hits[0].loc, Some(1.0));
        assert!(hits[0].excerpt.as_deref().unwrap().contains("consent"));

        // A second sync with nothing changed queues nothing; a deleted file leaves.
        std::fs::remove_file(root.join("notes0.md")).unwrap();
        sync(&store, std::slice::from_ref(&root), &options, dir.path()).unwrap();
        assert_eq!(store.pending(100).unwrap().len(), 1); // the photo, still waiting
        assert_eq!(store.counts().unwrap().files, 27);
    }

    #[test]
    fn an_unreadable_document_fails_with_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("broken.docx"), b"not a zip").unwrap();
        let store = FilesStore::open_in_memory().unwrap();
        sync(
            &store,
            &[dir.path().to_path_buf()],
            &IndexOptions::default(),
            Path::new("/nonexistent-home"),
        )
        .unwrap();
        let run = index_pending(&store, &mut BagOfWords, &IndexOptions::default(), 10).unwrap();
        assert_eq!(run.failed, 1);
        let detail: String = store
            .db
            .query_row("SELECT detail FROM files", [], |row| row.get(0))
            .unwrap();
        assert!(detail.starts_with("unreadable"), "{detail}");
    }
}
