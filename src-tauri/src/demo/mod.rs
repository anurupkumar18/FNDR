//! Deterministic demo dataset for evaluation and recovery demos.

use crate::embed::{Embedder, EMBEDDING_DIM};
use crate::store::MemoryRecord;
use chrono::Local;

pub const DEMO_ID_PREFIX: &str = "fndr-demo-";

/// Canonical string for scripted semantic search demos.
pub const INJECT_TEST_TEXT: &str =
    "OAuth redirect URI mismatch on localhost 5174";

struct SeedItem {
    app: &'static str,
    title: &'static str,
    text: &'static str,
}

fn seed_table() -> Vec<SeedItem> {
    vec![
        SeedItem {
            app: "Safari",
            title: "OAuth error page",
            text: "OAuth redirect URI mismatch on localhost 5174 — check the registered callback URL in the developer console.",
        },
        SeedItem {
            app: "Google Chrome",
            title: "React docs",
            text: "Semantic search finds meaning: embeddings map paraphrases to the same neighborhood in vector space.",
        },
        SeedItem {
            app: "Google Chrome",
            title: "localhost dev",
            text: "Vite dev server listening on port 5174; ensure firewall allows loopback traffic.",
        },
        SeedItem {
            app: "Safari",
            title: "Course notes",
            text: "LanceDB stores vectors locally; hybrid search blends keyword and semantic scores.",
        },
        SeedItem {
            app: "Visual Studio Code",
            title: "settings.json",
            text: "Tauri IPC uses typed commands between the React shell and the Rust core.",
        },
        SeedItem {
            app: "Visual Studio Code",
            title: "README.md",
            text: "Blocklist excludes password managers and System Settings from screen capture.",
        },
        SeedItem {
            app: "Terminal",
            title: "zsh",
            text: "cargo test && npm run build — CI runs format, clippy, and frontend checks on macOS.",
        },
        SeedItem {
            app: "Terminal",
            title: "git",
            text: "Feature branches merge via GitLab MR with review and passing pipeline.",
        },
        SeedItem {
            app: "Slack",
            title: "team — FNDR",
            text: "Demo mode seeds memories so grading does not depend on live capture.",
        },
        SeedItem {
            app: "Mail",
            title: "Inbox",
            text: "Retention policy trims records older than the configured day window.",
        },
        SeedItem {
            app: "Notes",
            title: "Demo script",
            text: "Pause capture during sensitive meetings; resume when back to normal work.",
        },
        SeedItem {
            app: "Safari",
            title: "GitLab",
            text: "Issues track owner, estimate, and labels for Agile visibility.",
        },
        SeedItem {
            app: "Google Chrome",
            title: "Linear",
            text: "Vector embeddings approximate text similarity for retrieval-augmented search.",
        },
        SeedItem {
            app: "Xcode",
            title: "Signing",
            text: "Apple Vision OCR extracts text from captured frames on macOS.",
        },
        SeedItem {
            app: "Finder",
            title: "Documents",
            text: "Screenshots stored under the app data directory with day-based folders.",
        },
        SeedItem {
            app: "Safari",
            title: "RFC",
            text: "Privacy-first design: embeddings and text never leave the machine.",
        },
        SeedItem {
            app: "Google Chrome",
            title: "Stack Overflow",
            text: "Deduplication skips near-duplicate frames using perceptual hashing.",
        },
        SeedItem {
            app: "Visual Studio Code",
            title: "Cargo.toml",
            text: "Optional VLM path can fail gracefully and fall back to OCR plus summarization.",
        },
        SeedItem {
            app: "Terminal",
            title: "npm",
            text: "Typecheck the UI with tsc; Vitest covers critical components.",
        },
        SeedItem {
            app: "Music",
            title: "Now Playing",
            text: "Background capture samples the frontmost window at a configurable frame rate.",
        },
        SeedItem {
            app: "Calendar",
            title: "Sprint review",
            text: "Each teammate explains one module: UI, capture, embeddings, vector store.",
        },
        SeedItem {
            app: "Safari",
            title: "Wikipedia",
            text: "Incognito mode pauses indexing without deleting stored memories.",
        },
        SeedItem {
            app: "Google Chrome",
            title: "GitHub",
            text: "Merge requests require green CI: fmt, clippy, tests, frontend build.",
        },
        SeedItem {
            app: "Preview",
            title: "diagram.png",
            text: "Timeline groups results by day and shows window titles with relevance scores.",
        },
        SeedItem {
            app: "Zoom",
            title: "Meeting",
            text: "Experimental meeting recorder may require ffmpeg; core search does not.",
        },
        SeedItem {
            app: "Safari",
            title: "Apple Developer",
            text: "Screen Recording permission is required for live capture; demo data bypasses it.",
        },
        SeedItem {
            app: "Terminal",
            title: "curl",
            text: "Health panel surfaces OCR, model, datastore, and disk checks on launch.",
        },
        SeedItem {
            app: "Visual Studio Code",
            title: "App.tsx",
            text: "Evaluation UI hides experimental tools behind a build flag for stable demos.",
        },
        SeedItem {
            app: "Google Chrome",
            title: "Notion",
            text: "README, DEMO.md, and TESTING.md document setup and the five-minute walkthrough.",
        },
        SeedItem {
            app: "Notes",
            title: "Rubric",
            text: "One filter demonstration: restrict results to the browser app used in the script.",
        },
        SeedItem {
            app: "Safari",
            title: "MDN",
            text: "Empty search shows guidance; errors show a banner without crashing the shell.",
        },
        SeedItem {
            app: "Finder",
            title: "Downloads",
            text: "Deterministic demo rows use the fndr-demo- id prefix for reset and re-seed.",
        },
        SeedItem {
            app: "Mail",
            title: "Draft",
            text: "Semantic query: that oauth localhost error — should return the OAuth mismatch note.",
        },
        SeedItem {
            app: "Terminal",
            title: "make",
            text: "make demo runs npm install and tauri dev for a one-command startup.",
        },
        SeedItem {
            app: "Google Chrome",
            title: "localhost:5174",
            text: "Redirect loops often mean the registered URI does not match the running port.",
        },
        SeedItem {
            app: "Safari",
            title: "Bookmarks",
            text: "Prototype phase prioritizes capture-to-search pipeline and typed IPC integration.",
        },
    ]
}

/// Build the fixed demo corpus with embeddings for search.
pub fn build_demo_records(embedder: &Embedder) -> Result<Vec<MemoryRecord>, String> {
    let now = Local::now();
    let day_bucket = now.format("%Y-%m-%d").to_string();
    let session = format!("{}-demo", now.format("%Y%m%d"));

    let mut out = Vec::new();
    for (i, item) in seed_table().into_iter().enumerate() {
        let id = format!("{DEMO_ID_PREFIX}{i:03}");
        let text = item.text.to_string();
        let embedding = embedder
            .embed_batch(&[text.clone()])
            .ok()
            .and_then(|mut v| v.pop())
            .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]);
        let image_embedding = vec![0.0f32; 512];
        let record = MemoryRecord {
            id,
            timestamp: now.timestamp_millis() - (i as i64) * 60_000,
            day_bucket: day_bucket.clone(),
            app_name: item.app.to_string(),
            bundle_id: None,
            window_title: item.title.to_string(),
            session_id: session.clone(),
            text: text.clone(),
            snippet: text.chars().take(200).collect::<String>(),
            embedding,
            image_embedding,
            screenshot_path: None,
            url: None,
        };
        out.push(record);
    }
    Ok(out)
}

/// Single injected test row for scripted queries.
pub fn build_inject_record(embedder: &Embedder) -> Result<MemoryRecord, String> {
    let now = Local::now();
    let day_bucket = now.format("%Y-%m-%d").to_string();
    let text = INJECT_TEST_TEXT.to_string();
    let embedding = embedder
        .embed_batch(&[text.clone()])
        .ok()
        .and_then(|mut v| v.pop())
        .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]);
    Ok(MemoryRecord {
        id: format!("{DEMO_ID_PREFIX}inject"),
        timestamp: now.timestamp_millis(),
        day_bucket,
        app_name: "Safari".to_string(),
        bundle_id: None,
        window_title: "Injected test — OAuth".to_string(),
        session_id: format!("{}-inject", now.format("%Y%m%d")),
        text: text.clone(),
        snippet: text.clone(),
        embedding,
        image_embedding: vec![0.0f32; 512],
        screenshot_path: None,
        url: Some("http://localhost:5174/callback".to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_table_has_expected_size() {
        assert_eq!(seed_table().len(), 36);
    }

    #[test]
    fn inject_text_is_stable() {
        assert!(INJECT_TEST_TEXT.contains("OAuth"));
    }
}
