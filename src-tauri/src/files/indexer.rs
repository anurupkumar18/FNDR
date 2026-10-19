//! Keeps the Files index in step with the chosen folders: [`sync`] records
//! what the crawl found and drops what is gone; [`index_pending`] reads queued
//! files into segments; [`continue_long_files`] reads the rest of long PDFs
//! and documents after everything else is in. Ported from DigUp's `Indexer`.
//!
//! Meaning comes from an [`Embedder`] and pictures, pages, sound and video
//! from a [`Reader`], so the real helpers and test fakes share one path.
//! What a reader writes goes in a scratch folder per file, removed after.

use super::crawl::{self, CrawlReport};
use super::extract;
use super::reader::{PageRange, Reader};
use super::store::{FileState, FilesStore, NewSegment, PendingFile};
use super::{prompts, search_text, FileKind, IndexOptions, Modality, SegmentKind};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Vision soft-token budgets, as DigUp measured them: photos 140, screenshots
/// and PDF pages 280 (small text), video frames 70 (many of them).
pub const PHOTO_BUDGET: u32 = 140;
pub const SCREENSHOT_BUDGET: u32 = 280;
pub const PAGE_BUDGET: u32 = 280;
pub const FRAME_BUDGET: u32 = 70;

/// Turns text, pictures and sound into 768-d, L2-normalized vectors.
pub trait Embedder {
    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;

    fn embed_image(&mut self, _path: &Path, _budget: u32) -> Result<Vec<f32>, String> {
        Err("this embedder reads text only".into())
    }

    fn embed_audio(&mut self, _path: &Path) -> Result<Vec<f32>, String> {
        Err("this embedder reads text only".into())
    }
}

/// What reading needs: the model, macOS's readers when there are any, and a
/// folder for what they write.
pub struct Tools<'a> {
    pub embedder: &'a mut dyn Embedder,
    pub reader: Option<&'a mut dyn Reader>,
    pub scratch: &'a Path,
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
    pub partial: usize,
    pub skipped: usize,
    pub failed: usize,
    /// Queued files of kinds no reader here can open (pictures, PDFs, sound and video need macOS's).
    pub waiting: usize,
}

/// How reading one file ended.
enum Outcome {
    Done(Value),
    Partial(Value),
    Skipped(String),
    Waiting,
}

/// Reads up to `max_files` queued files into segments, in the order people look for them most.
pub fn index_pending(
    store: &FilesStore,
    tools: &mut Tools<'_>,
    options: &IndexOptions,
    max_files: usize,
) -> rusqlite::Result<IndexRun> {
    let files = store.pending(max_files)?;
    read_files(store, tools, options, files, false)
}

/// Reads the next step of up to `max_files` long files read part way.
pub fn continue_long_files(
    store: &FilesStore,
    tools: &mut Tools<'_>,
    options: &IndexOptions,
    max_files: usize,
) -> rusqlite::Result<IndexRun> {
    let files = store.partial(max_files)?;
    read_files(store, tools, options, files, true)
}

fn read_files(
    store: &FilesStore,
    tools: &mut Tools<'_>,
    options: &IndexOptions,
    files: Vec<PendingFile>,
    later: bool,
) -> rusqlite::Result<IndexRun> {
    let mut run = IndexRun::default();
    for file in files {
        let scratch = tools.scratch.join(file.id.to_string());
        let _ = std::fs::create_dir_all(&scratch);
        let result = read_file(store, tools, options, &file, &scratch, later);
        let _ = std::fs::remove_dir_all(&scratch);
        match result {
            Ok(Outcome::Done(info)) => {
                store.set_state(file.id, FileState::Done, None, Some(&info))?;
                run.done += 1;
            }
            Ok(Outcome::Partial(info)) => {
                store.set_state(file.id, FileState::Partial, None, Some(&info))?;
                run.partial += 1;
            }
            Ok(Outcome::Skipped(why)) => {
                store.set_state(file.id, FileState::Skipped, Some(&why), None)?;
                run.skipped += 1;
            }
            Ok(Outcome::Waiting) => run.waiting += 1,
            Err(Failure::Store(error)) => return Err(error),
            Err(Failure::Read(why)) if later => {
                // A long file that can't be read further keeps what it has and counts as done.
                let mut info = store.info(file.id)?;
                strip_progress(&mut info);
                store.set_state(
                    file.id,
                    FileState::Done,
                    Some(&format!("not read further: {why}")),
                    Some(&info),
                )?;
                run.done += 1;
            }
            Err(Failure::Read(why)) => {
                store.set_state(file.id, FileState::Failed, Some(&why), None)?;
                run.failed += 1;
            }
        }
    }
    Ok(run)
}

enum Failure {
    Read(String),
    Store(rusqlite::Error),
}

impl From<String> for Failure {
    fn from(why: String) -> Self {
        Failure::Read(why)
    }
}

impl From<rusqlite::Error> for Failure {
    fn from(error: rusqlite::Error) -> Self {
        Failure::Store(error)
    }
}

fn read_file(
    store: &FilesStore,
    tools: &mut Tools<'_>,
    options: &IndexOptions,
    file: &PendingFile,
    scratch: &Path,
    later: bool,
) -> Result<Outcome, Failure> {
    let path = Path::new(&file.path);
    if file.kind == FileKind::Doc {
        return read_document(store, tools, options, file, later);
    }
    let Some(reader) = tools.reader.as_deref_mut() else {
        return Ok(Outcome::Waiting);
    };
    match file.kind {
        FileKind::Image | FileKind::Screenshot => {
            let screenshot = file.kind == FileKind::Screenshot;
            let picture = reader.picture(path, screenshot, options.min_image_side, scratch)?;
            let Some(prepared) = picture.path.filter(|_| !picture.too_small) else {
                return Ok(Outcome::Skipped(format!(
                    "small image {}x{}",
                    picture.width, picture.height
                )));
            };
            let budget = if screenshot {
                SCREENSHOT_BUDGET
            } else {
                PHOTO_BUDGET
            };
            let vector = tools.embedder.embed_image(&prepared, budget)?;
            store.add_segment(
                file.id,
                &segment(
                    SegmentKind::Image,
                    Modality::Image,
                    None,
                    None,
                    None,
                    None,
                    Some(&vector),
                ),
            )?;
            // The words on screen are for keyword search: codes, names and errors typed exactly.
            // Meaning comes from the picture alone; a vector of the words made screenshots
            // outrank better matches on DigUp's evals.
            if !picture.ocr.trim().is_empty() {
                let excerpt = search_text::excerpt(&picture.ocr, 240);
                store.add_segment(
                    file.id,
                    &segment(
                        SegmentKind::Ocr,
                        Modality::Text,
                        None,
                        None,
                        Some(&excerpt),
                        Some(&picture.ocr),
                        None,
                    ),
                )?;
            }
            let mut info = json!({ "width": picture.width, "height": picture.height });
            if let Some(taken) = picture.taken {
                info["taken"] = json!(taken);
            }
            if screenshot {
                info["ocr_chars"] = json!(picture.ocr.chars().count());
            }
            Ok(Outcome::Done(info))
        }
        FileKind::Pdf => read_pdf(store, tools, options, file, scratch, later),
        FileKind::Audio => {
            let sound = reader.audio(path, scratch)?;
            if sound.pieces.is_empty() {
                return Ok(Outcome::Skipped("silent or shorter than 2 s".into()));
            }
            for piece in &sound.pieces {
                let vector = tools.embedder.embed_audio(&piece.path)?;
                store.add_segment(
                    file.id,
                    &segment(
                        SegmentKind::Audio,
                        Modality::Audio,
                        Some(piece.start),
                        Some(piece.end),
                        None,
                        None,
                        Some(&vector),
                    ),
                )?;
            }
            Ok(Outcome::Done(
                json!({ "duration": round1(sound.duration), "sound_windows": sound.pieces.len() }),
            ))
        }
        FileKind::Video => {
            let video = reader.video(path, scratch)?;
            if video.frames.is_empty() && video.sound.is_empty() {
                return Ok(Outcome::Skipped("no frames or sound".into()));
            }
            for frame in &video.frames {
                let vector = tools.embedder.embed_image(&frame.path, FRAME_BUDGET)?;
                store.add_segment(
                    file.id,
                    &segment(
                        SegmentKind::Frame,
                        Modality::Image,
                        Some(frame.start),
                        Some(frame.end),
                        None,
                        None,
                        Some(&vector),
                    ),
                )?;
            }
            for piece in &video.sound {
                let vector = tools.embedder.embed_audio(&piece.path)?;
                store.add_segment(
                    file.id,
                    &segment(
                        SegmentKind::Audio,
                        Modality::Audio,
                        Some(piece.start),
                        Some(piece.end),
                        None,
                        None,
                        Some(&vector),
                    ),
                )?;
            }
            Ok(Outcome::Done(json!({
                "duration": round1(video.duration), "frames": video.frames.len(), "sound_windows": video.sound.len(),
            })))
        }
        FileKind::Doc | FileKind::Code => Ok(Outcome::Waiting),
    }
}

/// Pages of a PDF: all of a short one, a step of a long one. A long one stays
/// partial with `read_to` and `read_end` (pages, from 0) until its last step.
fn read_pdf(
    store: &FilesStore,
    tools: &mut Tools<'_>,
    options: &IndexOptions,
    file: &PendingFile,
    scratch: &Path,
    later: bool,
) -> Result<Outcome, Failure> {
    let reader = tools
        .reader
        .as_deref_mut()
        .ok_or_else(|| "no PDF reader".to_string())?;
    let mut info = if later {
        store.info(file.id)?
    } else {
        json!({})
    };
    let range = if later {
        let first = progress(&info, "read_to").unwrap_or(0);
        let end = progress(&info, "read_end").unwrap_or(first + options.pdf_step);
        PageRange {
            first: Some(first),
            end: Some((first + options.pdf_step).min(end)),
        }
    } else {
        PageRange::default()
    };
    let read = reader.pdf(Path::new(&file.path), range, options, scratch)?;
    let end = read.total.min(options.max_pages);
    let unread = read.end < end;
    if read.pages.is_empty() && !later && !unread {
        return Ok(Outcome::Skipped("no pages".into()));
    }
    let stem = Path::new(&file.path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    let title = read
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .unwrap_or(stem);
    let document =
        |text: &str| prompts::document(Some(title), &text.chars().take(4000).collect::<String>());

    // A page that is mostly digits (a table, a listing) gets keyword search only.
    let worded: Vec<usize> = (0..read.pages.len())
        .filter(|index| {
            read.pages[*index].image.is_none() && search_text::has_words(&read.pages[*index].text)
        })
        .collect();
    let texts: Vec<String> = worded
        .iter()
        .map(|index| document(&read.pages[*index].text))
        .collect();
    let mut text_vectors: Vec<Option<Vec<f32>>> = vec![None; read.pages.len()];
    if !texts.is_empty() {
        for (index, vector) in worded.iter().zip(tools.embedder.embed_texts(&texts)?) {
            text_vectors[*index] = Some(vector);
        }
    }
    let mut scanned = 0;
    for (index, page) in read.pages.iter().enumerate() {
        let number = Some(page.number as f64);
        if let Some(garbled) = &page.garbled {
            // Meaning only: garbled words are never shown or matched against what is typed.
            let vector = tools.embedder.embed_texts(&[document(garbled)])?.pop();
            store.add_segment(
                file.id,
                &segment(
                    SegmentKind::Page,
                    Modality::Text,
                    number,
                    None,
                    None,
                    None,
                    vector.as_deref(),
                ),
            )?;
        }
        let excerpt = (!page.text.trim().is_empty()).then(|| search_text::excerpt(&page.text, 240));
        let words = (!page.text.trim().is_empty()).then_some(page.text.as_str());
        match &page.image {
            Some(image) => {
                scanned += 1;
                let vector = tools.embedder.embed_image(image, PAGE_BUDGET)?;
                store.add_segment(
                    file.id,
                    &segment(
                        SegmentKind::PageImage,
                        Modality::Image,
                        number,
                        None,
                        excerpt.as_deref(),
                        words,
                        Some(&vector),
                    ),
                )?;
            }
            None => {
                store.add_segment(
                    file.id,
                    &segment(
                        SegmentKind::Page,
                        Modality::Text,
                        number,
                        None,
                        excerpt.as_deref(),
                        words,
                        text_vectors[index].as_deref(),
                    ),
                )?;
            }
        }
    }
    info["pages"] = json!(read.total);
    info["scanned_pages"] = json!(progress(&info, "scanned_pages").unwrap_or(0) + scanned);
    if unread {
        info["read_to"] = json!(read.end);
        info["read_end"] = json!(end);
        Ok(Outcome::Partial(info))
    } else {
        strip_progress(&mut info);
        Ok(Outcome::Done(info))
    }
}

/// Passages of a document: all of a short one, a step of a long one, read the
/// way PDFs are. The passages are cut the same way every time, so a later step
/// goes on where the last one stopped.
fn read_document(
    store: &FilesStore,
    tools: &mut Tools<'_>,
    options: &IndexOptions,
    file: &PendingFile,
    later: bool,
) -> Result<Outcome, Failure> {
    let path = Path::new(&file.path);
    let text = extract::document_text(path).map_err(|error| error.to_string())?;
    let chunks = extract::chunks(&text, 1800, 200, options.max_chunks + 1);
    if chunks.is_empty() {
        return Ok(Outcome::Skipped("no text".into()));
    }
    let end = chunks.len().min(options.max_chunks);
    let mut info = if later {
        store.info(file.id)?
    } else {
        json!({})
    };
    let first = if later {
        progress(&info, "read_to").unwrap_or(0)
    } else {
        0
    };
    let last = (first + options.doc_step).min(end);
    let title = path.file_stem().and_then(|stem| stem.to_str());
    let step = &chunks[first.min(end)..last];
    // Text that is mostly digits (a data dump saved as .txt) gets keyword search only.
    let worded: Vec<usize> = (0..step.len())
        .filter(|index| search_text::has_words(&step[*index]))
        .collect();
    let inputs: Vec<String> = worded
        .iter()
        .map(|index| prompts::document(title, &step[*index]))
        .collect();
    let vectors = if inputs.is_empty() {
        Vec::new()
    } else {
        tools.embedder.embed_texts(&inputs)?
    };
    if vectors.len() != inputs.len() {
        return Err(Failure::Read(
            "the model returned the wrong number of vectors".into(),
        ));
    }
    let mut by_index: Vec<Option<Vec<f32>>> = vec![None; step.len()];
    for (index, vector) in worded.into_iter().zip(vectors) {
        by_index[index] = Some(vector);
    }
    for (offset, chunk) in step.iter().enumerate() {
        let excerpt = search_text::excerpt(chunk, 240);
        let place = Some((first + offset + 1) as f64);
        store.add_segment(
            file.id,
            &segment(
                SegmentKind::Chunk,
                Modality::Text,
                place,
                None,
                Some(&excerpt),
                Some(chunk),
                by_index[offset].as_deref(),
            ),
        )?;
    }
    info["chunks"] = json!(end);
    info["chars"] = json!(text.chars().count());
    if last < end {
        info["read_to"] = json!(last);
        info["read_end"] = json!(end);
        Ok(Outcome::Partial(info))
    } else {
        strip_progress(&mut info);
        Ok(Outcome::Done(info))
    }
}

fn segment<'a>(
    kind: SegmentKind,
    modality: Modality,
    loc: Option<f64>,
    loc_end: Option<f64>,
    excerpt: Option<&'a str>,
    text: Option<&'a str>,
    vector: Option<&'a [f32]>,
) -> NewSegment<'a> {
    NewSegment {
        kind,
        modality,
        loc,
        loc_end,
        excerpt,
        text,
        vector,
    }
}

fn progress(info: &Value, key: &str) -> Option<usize> {
    info.get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
}

fn strip_progress(info: &mut Value) {
    if let Some(object) = info.as_object_mut() {
        object.remove("read_to");
        object.remove("read_end");
    }
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::super::reader::{Page, PdfRead, Picture, Piece, SoundRead, VideoRead};
    use super::super::searcher::{SearchOptions, Searcher};
    use super::super::DIMENSION;
    use super::*;

    /// Words hashed into directions: texts sharing words point the same way.
    /// Pictures and sound are "embedded" by their file name, so a test can
    /// name a picture after what it shows.
    pub(crate) struct BagOfWords;

    impl BagOfWords {
        pub(crate) fn vector(text: &str) -> Vec<f32> {
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

        fn embed_image(&mut self, path: &Path, _budget: u32) -> Result<Vec<f32>, String> {
            Ok(Self::vector(
                &path
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .replace('_', " "),
            ))
        }

        fn embed_audio(&mut self, path: &Path) -> Result<Vec<f32>, String> {
            self.embed_image(path, 0)
        }
    }

    /// Stands in for macOS's readers: a PDF of `pdf_pages` pages, each saying
    /// "page N", with page 2 a scan; pictures pass through; a recording has
    /// two windows; a video one frame and one window.
    struct FakeReader {
        pdf_pages: usize,
    }

    impl Reader for FakeReader {
        fn picture(
            &mut self,
            path: &Path,
            screenshot: bool,
            _min: u32,
            _out: &Path,
        ) -> Result<Picture, String> {
            if path.to_string_lossy().contains("tiny") {
                return Ok(Picture {
                    too_small: true,
                    width: 10,
                    height: 10,
                    ..Picture::default()
                });
            }
            Ok(Picture {
                path: Some(path.to_path_buf()),
                width: 1200,
                height: 800,
                taken: Some("2026:10:01 09:41:00".into()),
                too_small: false,
                ocr: if screenshot {
                    "payment declined error K7Q2LM".into()
                } else {
                    String::new()
                },
            })
        }

        fn pdf(
            &mut self,
            _path: &Path,
            range: PageRange,
            options: &IndexOptions,
            out: &Path,
        ) -> Result<PdfRead, String> {
            let total = self.pdf_pages;
            let first = range.first.unwrap_or(0);
            let end = range
                .end
                .unwrap_or(if total > options.big_pdf_pages {
                    options.pdf_step
                } else {
                    total
                })
                .min(total);
            let pages = (first..end)
                .map(|index| Page {
                    number: index + 1,
                    text: format!(
                        "Lease clause page {} about tenants pets and parking.",
                        index + 1
                    ),
                    image: (index == 1).then(|| out.join("scanned_signature_page.png")),
                    ocr: index == 1,
                    garbled: None,
                })
                .collect();
            Ok(PdfRead {
                total,
                first,
                end,
                pages,
                title: Some("Apartment lease".into()),
            })
        }

        fn audio(&mut self, _path: &Path, out: &Path) -> Result<SoundRead, String> {
            Ok(SoundRead {
                pieces: vec![
                    Piece {
                        path: out.join("intro_music.wav"),
                        start: 0.0,
                        end: 30.0,
                    },
                    Piece {
                        path: out.join("talk_about_sleep.wav"),
                        start: 25.0,
                        end: 55.0,
                    },
                ],
                duration: 60.04,
            })
        }

        fn video(&mut self, _path: &Path, out: &Path) -> Result<VideoRead, String> {
            Ok(VideoRead {
                frames: vec![Piece {
                    path: out.join("zebra_on_grass.jpg"),
                    start: 12.0,
                    end: 20.0,
                }],
                sound: vec![Piece {
                    path: out.join("wind_noise.wav"),
                    start: 0.0,
                    end: 20.0,
                }],
                duration: 20.0,
            })
        }
    }

    fn write(root: &Path, name: &str, body: &str) {
        std::fs::write(root.join(name), body).unwrap();
    }

    fn search(store: &FilesStore, query: &str) -> Vec<super::super::searcher::FileHit> {
        let searcher = Searcher::load(store).unwrap();
        searcher
            .search(
                store,
                query,
                Some(&BagOfWords::vector(query)),
                &SearchOptions::default(),
            )
            .unwrap()
    }

    #[test]
    fn documents_become_searchable_by_words_and_meaning() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Documents");
        std::fs::create_dir_all(&root).unwrap();
        write(
            &root,
            "lease.md",
            "Tenants may keep one cat. Other pets need the landlord's written consent.",
        );
        for n in 0..25 {
            write(
                &root,
                &format!("notes{n}.md"),
                &format!("Quarterly planning sync number {n}: budget, hiring, roadmap."),
            );
        }
        write(&root, "numbers.txt", "1 2 3 4 5 6 7 8 9 10 11 12 13 14 15");
        write(&root, "photo.jpg", "not decoded here");

        let store = FilesStore::open_in_memory().unwrap();
        let options = IndexOptions::default();
        sync(&store, std::slice::from_ref(&root), &options, dir.path()).unwrap();
        let scratch = dir.path().join("scratch");
        let mut tools = Tools {
            embedder: &mut BagOfWords,
            reader: None,
            scratch: &scratch,
        };
        let run = index_pending(&store, &mut tools, &options, 100).unwrap();
        assert_eq!(
            run,
            IndexRun {
                done: 27,
                waiting: 1,
                ..IndexRun::default()
            }
        );
        // The table of numbers has words for keyword search but no meaning vector.
        assert_eq!(store.counts().unwrap().vectors, 26);

        let hits = search(&store, "landlord consent pets");
        assert!(hits[0].path.ends_with("lease.md"), "{hits:?}");
        assert_eq!(hits[0].loc, Some(1.0));
        assert!(hits[0].excerpt.as_deref().unwrap().contains("consent"));

        // A second sync with nothing changed queues nothing; a deleted file leaves.
        std::fs::remove_file(root.join("notes0.md")).unwrap();
        sync(&store, std::slice::from_ref(&root), &options, dir.path()).unwrap();
        assert_eq!(store.pending(100).unwrap().len(), 1); // the photo, waiting for a reader
        assert_eq!(store.counts().unwrap().files, 27);
    }

    #[test]
    fn pictures_screenshots_sound_and_video_are_read_with_a_reader() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Media");
        std::fs::create_dir_all(&root).unwrap();
        for name in [
            "dog_on_the_beach.jpg",
            "Screenshot checkout.png",
            "tiny.png",
            "podcast.m4a",
            "safari.mov",
        ] {
            write(&root, name, "x");
        }
        for n in 0..25 {
            write(
                &root,
                &format!("filler{n}.md"),
                &format!("Grocery list {n}: apples, bread, milk."),
            );
        }
        let store = FilesStore::open_in_memory().unwrap();
        let options = IndexOptions::default();
        sync(&store, std::slice::from_ref(&root), &options, dir.path()).unwrap();
        let scratch = dir.path().join("scratch");
        let mut reader = FakeReader { pdf_pages: 0 };
        let mut tools = Tools {
            embedder: &mut BagOfWords,
            reader: Some(&mut reader),
            scratch: &scratch,
        };
        let run = index_pending(&store, &mut tools, &options, 100).unwrap();
        assert_eq!(
            (run.done, run.skipped, run.failed, run.waiting),
            (29, 1, 0, 0),
            "{run:?}"
        );

        let hits = search(&store, "dog beach");
        assert!(hits[0].path.ends_with("dog_on_the_beach.jpg"), "{hits:?}");
        // A screenshot's words are for exact lookups, quoted from its OCR.
        let hits = search(&store, "K7Q2LM");
        assert!(
            hits[0].path.ends_with("Screenshot checkout.png"),
            "{hits:?}"
        );
        assert!(hits[0]
            .excerpt
            .as_deref()
            .unwrap()
            .contains("\u{AB}K7Q2LM\u{BB}"));
        // A recording and a video point at their moment.
        let hits = search(&store, "talk about sleep");
        assert!(hits[0].path.ends_with("podcast.m4a"), "{hits:?}");
        assert_eq!((hits[0].loc, hits[0].loc_end), (Some(25.0), Some(55.0)));
        let hits = search(&store, "zebra grass");
        assert!(hits[0].path.ends_with("safari.mov"), "{hits:?}");
        assert_eq!(hits[0].segment, SegmentKind::Frame);
        assert_eq!(hits[0].loc, Some(12.0));
        assert_eq!(hits[0].info["duration"], json!(20.0));
        assert!(!scratch.join("1").exists(), "scratch folders are removed");
    }

    #[test]
    fn a_long_pdf_is_read_in_steps_and_a_scan_is_a_picture() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "lease.pdf", "x");
        let store = FilesStore::open_in_memory().unwrap();
        let options = IndexOptions::default();
        sync(
            &store,
            &[dir.path().to_path_buf()],
            &options,
            Path::new("/nonexistent-home"),
        )
        .unwrap();
        let scratch = dir.path().join("scratch");
        let mut reader = FakeReader { pdf_pages: 70 };
        let mut tools = Tools {
            embedder: &mut BagOfWords,
            reader: Some(&mut reader),
            scratch: &scratch,
        };

        let run = index_pending(&store, &mut tools, &options, 10).unwrap();
        assert_eq!(run.partial, 1);
        let info = store.info(1).unwrap();
        assert_eq!(
            (
                info["read_to"].as_u64(),
                info["read_end"].as_u64(),
                info["pages"].as_u64()
            ),
            (Some(30), Some(70), Some(70))
        );
        assert_eq!(store.counts().unwrap().done, 1); // partial counts as searchable

        assert_eq!(
            continue_long_files(&store, &mut tools, &options, 10)
                .unwrap()
                .partial,
            1
        );
        let run = continue_long_files(&store, &mut tools, &options, 10).unwrap();
        assert_eq!(run.done, 1);
        let info = store.info(1).unwrap();
        assert!(info.get("read_to").is_none());
        assert_eq!(info["scanned_pages"].as_u64(), Some(1));

        let hits = search(&store, "scanned signature page");
        assert_eq!(
            (hits[0].segment, hits[0].loc),
            (SegmentKind::PageImage, Some(2.0)),
            "{hits:?}"
        );
        let pages: i64 = store
            .db
            .query_row(
                "SELECT count(*) FROM segments WHERE kind IN ('page', 'page_image')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(pages, 70);
    }

    #[test]
    fn a_long_document_is_read_in_steps() {
        let dir = tempfile::tempdir().unwrap();
        let text = (0..5000)
            .map(|n| format!("Sentence {n} of the long manual about engines."))
            .collect::<Vec<_>>()
            .join(" ");
        write(dir.path(), "manual.txt", &text);
        let store = FilesStore::open_in_memory().unwrap();
        let options = IndexOptions {
            doc_step: 40,
            ..IndexOptions::default()
        };
        sync(
            &store,
            &[dir.path().to_path_buf()],
            &options,
            Path::new("/nonexistent-home"),
        )
        .unwrap();
        let scratch = dir.path().join("scratch");
        let mut tools = Tools {
            embedder: &mut BagOfWords,
            reader: None,
            scratch: &scratch,
        };
        assert_eq!(
            index_pending(&store, &mut tools, &options, 10)
                .unwrap()
                .partial,
            1
        );
        let total = store.info(1).unwrap()["read_end"].as_u64().unwrap();
        while store.partial(10).unwrap().len() == 1 {
            continue_long_files(&store, &mut tools, &options, 10).unwrap();
        }
        let chunks: i64 = store
            .db
            .query_row(
                "SELECT count(*) FROM segments WHERE kind = 'chunk'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(chunks as u64, total);
        let places: Vec<f64> = {
            let mut statement = store
                .db
                .prepare("SELECT loc FROM segments WHERE kind = 'chunk' ORDER BY loc")
                .unwrap();
            statement
                .query_map([], |row| row.get(0))
                .unwrap()
                .map(Result::unwrap)
                .collect()
        };
        assert!(
            places.windows(2).all(|pair| pair[1] == pair[0] + 1.0),
            "passages numbered once each"
        );
    }

    #[test]
    fn an_unreadable_document_fails_with_a_reason() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "broken.docx", "not a zip");
        let store = FilesStore::open_in_memory().unwrap();
        sync(
            &store,
            &[dir.path().to_path_buf()],
            &IndexOptions::default(),
            Path::new("/nonexistent-home"),
        )
        .unwrap();
        let scratch = dir.path().join("scratch");
        let mut tools = Tools {
            embedder: &mut BagOfWords,
            reader: None,
            scratch: &scratch,
        };
        let run = index_pending(&store, &mut tools, &IndexOptions::default(), 10).unwrap();
        assert_eq!(run.failed, 1);
        let detail: String = store
            .db
            .query_row("SELECT detail FROM files", [], |row| row.get(0))
            .unwrap();
        assert!(detail.starts_with("unreadable"), "{detail}");
    }
}
