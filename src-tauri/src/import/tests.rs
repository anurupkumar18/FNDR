//! Architecture and end-to-end tests for the import pipeline.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::{
    ImportEmbedder, ImportItem, ImportKind, ImportRunner, ImportSource, Permission, RunState,
    ScanBatch,
};
use crate::config::Config;
use crate::graph::GraphStore;
use crate::storage::{StateStore, Store};
use crate::AppState;

struct FakeSource {
    items: parking_lot::Mutex<Vec<ImportItem>>,
}

impl ImportSource for FakeSource {
    fn kind(&self) -> ImportKind {
        ImportKind::Mail
    }
    fn label(&self) -> &'static str {
        "Synthetic mail"
    }
    fn permissions_needed(&self) -> Vec<Permission> {
        vec![Permission::None]
    }
    fn scan(
        &self,
        _: Option<super::Cursor>,
        _: u32,
        _: usize,
    ) -> Result<ScanBatch, super::ImportError> {
        Ok(ScanBatch {
            items: self.items.lock().clone(),
            next: None,
            done: true,
        })
    }
}

struct BlockingSource {
    entered: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
    item: ImportItem,
}

impl ImportSource for BlockingSource {
    fn kind(&self) -> ImportKind {
        ImportKind::Mail
    }
    fn label(&self) -> &'static str {
        "Synthetic mail"
    }
    fn permissions_needed(&self) -> Vec<Permission> {
        vec![Permission::None]
    }
    fn scan(
        &self,
        _: Option<super::Cursor>,
        _: u32,
        _: usize,
    ) -> Result<ScanBatch, super::ImportError> {
        self.entered.store(true, Ordering::SeqCst);
        while !self.release.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        Ok(ScanBatch {
            items: vec![self.item.clone()],
            next: None,
            done: true,
        })
    }
}

fn mail_item(id: &str, title: &str, text: &str) -> ImportItem {
    ImportItem {
        kind: ImportKind::Mail,
        external_id: id.into(),
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
        title: title.into(),
        text: text.into(),
        reopen_target: Some(format!("message://{id}")),
        app_label: "Mail".into(),
        people: Vec::new(),
        url: None,
    }
}

struct WithSensitiveItems(Arc<dyn ImportSource>);

impl ImportSource for WithSensitiveItems {
    fn kind(&self) -> ImportKind {
        self.0.kind()
    }
    fn label(&self) -> &'static str {
        self.0.label()
    }
    fn permissions_needed(&self) -> Vec<Permission> {
        self.0.permissions_needed()
    }
    fn scan(
        &self,
        since: Option<super::Cursor>,
        window_days: u32,
        limit: usize,
    ) -> Result<ScanBatch, super::ImportError> {
        let inject = since.is_none();
        let mut batch = self.0.scan(since, window_days, limit.saturating_sub(2))?;
        if inject && !batch.items.is_empty() {
            let mut bank = batch.items[0].clone();
            bank.external_id = "synthetic-bank".into();
            bank.url = Some("https://secure.chase.com/account".into());
            bank.reopen_target = bank.url.clone();
            let mut password = batch.items[0].clone();
            password.external_id = "synthetic-password".into();
            password.app_label = "1Password".into();
            batch.items.extend([bank, password]);
        }
        Ok(batch)
    }
}

async fn verify_connector(source: Arc<dyn ImportSource>, query: &str) {
    let first_batch = source.scan(None, 30, 200).unwrap();
    let allowed = first_batch
        .items
        .iter()
        .find(|item| item.title.contains(query) || item.text.contains(query))
        .unwrap()
        .clone();
    let kind = source.kind();
    let (_dir, app) = test_app().await;
    let runner = ImportRunner::new(
        vec![Arc::new(WithSensitiveItems(source))],
        ImportEmbedder::Given(Some(Arc::new(crate::embedding::Embedder::mock_for_tests()))),
    );
    let first = run_source_to_end(&runner, app.clone(), kind.as_str()).await;
    assert_eq!(first.state, RunState::Done, "{:?}", first.error);
    assert!(first.stored >= 1);
    assert!(first.skipped >= 2);
    let row = app
        .store
        .get_memory_by_id(&allowed.record_id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.source_type, kind.source_type());
    assert!(row.is_leaf_row());
    assert!(app
        .store
        .keyword_search(query, 10, None, None)
        .await
        .unwrap()
        .iter()
        .any(|result| result.id == row.id));
    let second = run_source_to_end(&runner, app.clone(), kind.as_str()).await;
    assert_eq!(second.state, RunState::Done, "{:?}", second.error);
    assert_eq!(second.stored, 0);
    assert_eq!(second.updated, 0);
    assert!(second.duplicates >= 1);
    assert!(runner.forget(&app, kind.as_str()).await.unwrap() >= 1);
    assert!(app
        .store
        .get_memory_by_id(&allowed.record_id())
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn mail_connector_reaches_search_and_forget() {
    use super::sources::mail::{MailImport, MailInput};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.eml");
    let date = chrono::Utc::now().to_rfc2822();
    std::fs::write(&path, format!("From: Jo <jo@example.test>\nSubject: Orionmail brief\nDate: {date}\nMessage-ID: <orion@example.test>\n\nReviewed Orionmail milestones.\n")).unwrap();
    verify_connector(
        Arc::new(MailImport::new(vec![MailInput::Eml(path)], vec![], false)),
        "Orionmail",
    )
    .await;
}

struct SyntheticCalendar {
    now_ms: i64,
}

impl crate::proactive_signals::calendar::CalendarSource for SyntheticCalendar {
    fn status(&self) -> crate::proactive_signals::calendar::CalendarStatus {
        crate::proactive_signals::calendar::CalendarStatus::Granted
    }
    fn request_access(&self) -> crate::proactive_signals::calendar::CalendarStatus {
        self.status()
    }
    fn calendars(&self) -> Result<Vec<crate::proactive_signals::calendar::CalendarInfo>, String> {
        Ok(vec![crate::proactive_signals::calendar::CalendarInfo {
            id: "work".into(),
            name: "Work".into(),
            subscribed: false,
        }])
    }
    fn events(
        &self,
        _: &[String],
        _: i64,
        _: i64,
    ) -> Result<Vec<crate::proactive_signals::calendar::CalendarEvent>, String> {
        let now = self.now_ms;
        Ok(vec![crate::proactive_signals::calendar::CalendarEvent {
            id: "orion-event".into(),
            calendar_id: "work".into(),
            title: Some("Orioncalendar review".into()),
            starts_at: now - 3_600_000,
            ends_at: now,
            attendees: vec![],
            location: None,
            notes: Some("Reviewed synthetic milestones".into()),
            conference_url: None,
            all_day: false,
            declined: false,
        }])
    }
}

struct CalendarConnectorFixture {
    now_ms: i64,
}

impl ImportSource for CalendarConnectorFixture {
    fn kind(&self) -> ImportKind {
        ImportKind::Calendar
    }
    fn label(&self) -> &'static str {
        "Calendar"
    }
    fn permissions_needed(&self) -> Vec<Permission> {
        vec![Permission::Calendar]
    }
    fn scan(
        &self,
        since: Option<super::Cursor>,
        window_days: u32,
        limit: usize,
    ) -> Result<ScanBatch, super::ImportError> {
        super::sources::calendar::scan_calendar(
            &SyntheticCalendar {
                now_ms: self.now_ms,
            },
            &[],
            since,
            window_days,
            limit,
            self.now_ms,
        )
    }
}

#[tokio::test]
async fn calendar_connector_reaches_search_and_forget() {
    verify_connector(
        Arc::new(CalendarConnectorFixture {
            now_ms: chrono::Utc::now().timestamp_millis(),
        }),
        "Orioncalendar",
    )
    .await;
}

#[tokio::test]
async fn slack_connector_reaches_search_and_forget() {
    use super::sources::slack::SlackSource;
    let dir = tempfile::tempdir().unwrap();
    let channel = dir.path().join("project");
    std::fs::create_dir(&channel).unwrap();
    std::fs::write(
        dir.path().join("channels.json"),
        r#"[{"id":"C1","name":"project"}]"#,
    )
    .unwrap();
    let day = chrono::Utc::now().date_naive();
    let ts = chrono::Utc::now().timestamp();
    std::fs::write(
        channel.join(format!("{day}.json")),
        format!(r#"[{{"ts":"{ts}.000001","user":"U1","text":"Orionslack rollout notes"}}]"#),
    )
    .unwrap();
    verify_connector(
        Arc::new(SlackSource {
            path: dir.path().to_owned(),
        }),
        "Orionslack",
    )
    .await;
}

#[tokio::test]
async fn git_connector_reaches_search_and_forget() {
    use super::sources::git::GitSource;
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        assert!(Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .unwrap()
            .success());
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "me@example.test"]);
    run(&["config", "user.name", "Me"]);
    std::fs::write(dir.path().join("note.txt"), "synthetic note\n").unwrap();
    run(&["add", "note.txt"]);
    run(&["commit", "-qm", "Oriongit milestone"]);
    verify_connector(
        Arc::new(GitSource {
            roots: vec![dir.path().to_owned()],
            global_config_override: Some("/dev/null".into()),
            scan_gh: false,
        }),
        "Oriongit",
    )
    .await;
}

#[tokio::test]
async fn browser_connector_reaches_search_and_forget() {
    use super::sources::browser::BrowserHistorySource;
    let home = tempfile::tempdir().unwrap();
    let path = home
        .path()
        .join("Library/Application Support/Google/Chrome/Default/History");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT); CREATE TABLE visits (url INTEGER, visit_time INTEGER);").unwrap();
    db.execute(
        "INSERT INTO urls VALUES (1, 'https://example.com/orionbrowser', 'Orionbrowser discovery')",
        [],
    )
    .unwrap();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let visit_time = 11_644_473_600_000_000_i64 + (now_ms - 60_000) * 1000;
    db.execute("INSERT INTO visits VALUES (1, ?1)", [visit_time])
        .unwrap();
    drop(db);
    verify_connector(
        Arc::new(BrowserHistorySource::new(home.path().to_owned(), now_ms)),
        "Orionbrowser",
    )
    .await;
}

#[tokio::test]
async fn recent_files_connector_reaches_search_and_forget() {
    use super::sources::files::RecentFilesSource;
    let root = tempfile::Builder::new()
        .prefix("fndr_fixture")
        .tempdir()
        .unwrap();
    std::fs::write(
        root.path().join("orionfiles.txt"),
        "Orionfiles synthetic document",
    )
    .unwrap();
    let now_ms = chrono::Utc::now().timestamp_millis() + 10_000;
    verify_connector(
        Arc::new(RecentFilesSource::new(vec![root.path().to_owned()], now_ms)),
        "Orionfiles",
    )
    .await;
}

async fn test_app() -> (tempfile::TempDir, Arc<AppState>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let store_path = path.clone();
    let store = tokio::task::spawn_blocking(move || Arc::new(Store::new(&store_path).unwrap()))
        .await
        .unwrap();
    let state_path = path.clone();
    let state_store =
        tokio::task::spawn_blocking(move || Arc::new(StateStore::new(&state_path).unwrap()))
            .await
            .unwrap();
    let graph = GraphStore::new(store.clone());
    let mut config = Config::default();
    config.blocklist.clear();
    let app = Arc::new(AppState::new(path, config, store, state_store, graph, None));
    (dir, app)
}

async fn run_to_end(runner: &ImportRunner, app: Arc<AppState>) -> super::ImportStatus {
    run_source_to_end(runner, app, "mail").await
}

async fn run_source_to_end(
    runner: &ImportRunner,
    app: Arc<AppState>,
    source: &str,
) -> super::ImportStatus {
    let kind = ImportKind::parse(source).expect("known source kind");
    super::consent::set(&app.state_store, kind, true).unwrap();
    runner.start(app, source, 30).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let status = runner.status(source);
            if status.state != RunState::Running {
                return status;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("import finishes")
}

#[tokio::test]
async fn import_requires_persisted_source_consent_and_forget_clears_it() {
    let (_dir, app) = test_app().await;
    let runner = ImportRunner::new(
        vec![Arc::new(FakeSource {
            items: parking_lot::Mutex::new(vec![]),
        })],
        ImportEmbedder::Given(Some(Arc::new(crate::embedding::Embedder::mock_for_tests()))),
    );
    assert!(runner
        .start(app.clone(), "mail", 30)
        .unwrap_err()
        .contains("consent"));
    assert_eq!(runner.status("mail").state, RunState::Idle);
    super::consent::set(&app.state_store, ImportKind::Mail, true).unwrap();
    assert!(super::consent::get(&app.state_store, ImportKind::Mail).unwrap());
    let status = run_to_end(&runner, app.clone()).await;
    assert_eq!(status.state, RunState::Done);
    let fresh_runner = ImportRunner::new(
        vec![Arc::new(FakeSource {
            items: parking_lot::Mutex::new(vec![]),
        })],
        ImportEmbedder::Given(None),
    );
    let source = fresh_runner.list_sources(&app).await.remove(0);
    assert_eq!(source.status.window_days, 30);
    assert!(source.status.finished_at_ms.is_some());
    runner.forget(&app, "mail").await.unwrap();
    assert!(!super::consent::get(&app.state_store, ImportKind::Mail).unwrap());
    assert!(runner
        .start(app, "mail", 30)
        .unwrap_err()
        .contains("consent"));
}

#[tokio::test]
async fn fake_source_is_searchable_safe_idempotent_and_forgettable() {
    let (_dir, app) = test_app().await;
    let allowed = mail_item(
        "allowed",
        "Atlas migration meeting",
        "Reviewed the deployment checklist and recorded migration evidence.",
    );
    let mut banking = mail_item("bank", "Bank account", "Account details.");
    banking.url = Some("https://secure.chase.com/account".into());
    let mut password = mail_item("vault", "Password vault", "Vault details.");
    password.app_label = "1Password".into();
    let mut secret = mail_item(
        "secret",
        "Deployment notes",
        "Discussed the release.\nghp_abcdefghijklmnop1234 leaked\nAgreed on timing.",
    );
    secret.reopen_target = None;
    let source = Arc::new(FakeSource {
        items: parking_lot::Mutex::new(vec![allowed.clone(), banking, password, secret.clone()]),
    });
    let runner = ImportRunner::new(
        vec![source],
        ImportEmbedder::Given(Some(Arc::new(crate::embedding::Embedder::mock_for_tests()))),
    );
    let first = run_to_end(&runner, app.clone()).await;
    assert_eq!(first.state, RunState::Done, "{:?}", first.error);
    assert_eq!(first.stored, 2);
    assert_eq!(first.skipped, 2);
    let rows = app
        .store
        .keyword_search("migration", 10, None, None)
        .await
        .unwrap();
    assert!(rows
        .iter()
        .any(|row| row.id == allowed.record_id() && row.source_type == "import:mail"));
    let stored = app
        .store
        .get_memory_by_id(&allowed.record_id())
        .await
        .unwrap()
        .unwrap();
    assert!(crate::context_runtime::retrieve::memory_is_visible(
        &stored,
        &[]
    ));
    assert!(stored.embedding_text.contains("Atlas migration meeting"));
    assert!(stored.embedding_text.contains("Mail (mail)"));
    let chunk = crate::storage::MemoryChunkRecord {
        id: "synthetic-import-chunk".into(),
        memory_id: stored.id.clone(),
        text: "Synthetic child chunk".into(),
        ..Default::default()
    };
    app.store.upsert_memory_chunks(&[chunk]).await.unwrap();
    app.graph.ingest_memory(&stored).await.unwrap();
    let graph_id = format!("memory:{}", stored.id);
    assert_eq!(
        app.store
            .get_nodes_by_ids(&[graph_id.clone()])
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(app
        .store
        .get_all_edges()
        .await
        .unwrap()
        .iter()
        .any(|edge| edge.source == graph_id || edge.target == graph_id));
    let kept_secret = app
        .store
        .get_memory_by_id(&secret.record_id())
        .await
        .unwrap()
        .unwrap();
    assert!(!kept_secret.clean_text.contains("ghp_"));
    assert!(kept_secret.clean_text.contains("[redacted]"));
    let second = run_to_end(&runner, app.clone()).await;
    assert_eq!(second.state, RunState::Done, "{:?}", second.error);
    assert_eq!(second.stored, 0);
    assert_eq!(second.duplicates, 2);
    assert_eq!(runner.forget(&app, "mail").await.unwrap(), 2);
    assert_eq!(
        app.store
            .count_memories_by_source_type("import:mail")
            .await
            .unwrap(),
        0
    );
    assert!(app
        .store
        .list_chunks_for_memory(&allowed.record_id())
        .await
        .unwrap()
        .is_empty());
    assert!(app
        .store
        .get_nodes_by_ids(&[graph_id.clone()])
        .await
        .unwrap()
        .is_empty());
    assert!(!app
        .store
        .get_all_edges()
        .await
        .unwrap()
        .iter()
        .any(|edge| edge.source == graph_id || edge.target == graph_id));
    assert!(super::cursor::load(&app.state_store, ImportKind::Mail)
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn changed_item_updates_one_row_and_erased_item_stays_erased_until_forget() {
    let (_dir, app) = test_app().await;
    let original = mail_item("same-id", "Atlas plan", "Reviewed the original Atlas plan.");
    let source = Arc::new(FakeSource {
        items: parking_lot::Mutex::new(vec![original.clone()]),
    });
    let runner = ImportRunner::new(
        vec![source.clone()],
        ImportEmbedder::Given(Some(Arc::new(crate::embedding::Embedder::mock_for_tests()))),
    );
    assert_eq!(run_to_end(&runner, app.clone()).await.stored, 1);
    let mut edited = original.clone();
    edited.text = "Reviewed the revised Atlas plan.".into();
    *source.items.lock() = vec![edited.clone()];
    let update = run_to_end(&runner, app.clone()).await;
    assert_eq!(update.updated, 1);
    assert_eq!(
        app.store
            .count_memories_by_source_type("import:mail")
            .await
            .unwrap(),
        1
    );
    let record = app
        .store
        .get_memory_by_id(&edited.record_id())
        .await
        .unwrap()
        .unwrap();
    assert!(record.clean_text.contains("revised"));
    super::tombstone_records(&app.state_store, &[record]).unwrap();
    app.store
        .delete_memories_by_source_type("import:mail")
        .await
        .unwrap();
    let rerun = run_to_end(&runner, app.clone()).await;
    assert_eq!(rerun.stored, 0);
    assert_eq!(rerun.skip_reasons.get("erased_by_person"), Some(&1));
    assert_eq!(runner.forget(&app, "mail").await.unwrap(), 0);
    assert!(
        super::cursor::load_tombstones(&app.state_store, ImportKind::Mail)
            .unwrap()
            .is_empty()
    );
    assert_eq!(run_to_end(&runner, app.clone()).await.stored, 1);
}

#[tokio::test]
async fn import_retention_uses_item_time_and_ignores_the_screen_sweep() {
    let (_dir, app) = test_app().await;
    let mut twenty_days_old = mail_item(
        "twenty-days",
        "Old project note",
        "Reviewed the October project archive.",
    );
    twenty_days_old.timestamp_ms -= 20 * 86_400_000;
    let mut expired = mail_item("expired", "Expired note", "Reviewed an older archive.");
    expired.timestamp_ms -= 91 * 86_400_000;
    let source = Arc::new(FakeSource {
        items: parking_lot::Mutex::new(vec![twenty_days_old.clone(), expired]),
    });
    let runner = ImportRunner::new(
        vec![source],
        ImportEmbedder::Given(Some(Arc::new(crate::embedding::Embedder::mock_for_tests()))),
    );
    let status = run_to_end(&runner, app.clone()).await;
    assert_eq!(status.stored, 1);
    assert_eq!(status.skip_reasons.get("outside_retention"), Some(&1));
    app.store.delete_older_than(7).await.unwrap();
    assert!(app
        .store
        .get_memory_by_id(&twenty_days_old.record_id())
        .await
        .unwrap()
        .is_some());
    let record = app
        .store
        .get_memory_by_id(&twenty_days_old.record_id())
        .await
        .unwrap()
        .unwrap();
    app.graph.ingest_memory(&record).await.unwrap();
    let graph_id = format!("memory:{}", record.id);
    assert_eq!(app.store.delete_imports_older_than(7).await.unwrap(), 1);
    assert!(app
        .store
        .get_memory_by_id(&twenty_days_old.record_id())
        .await
        .unwrap()
        .is_none());
    assert!(app
        .store
        .get_nodes_by_ids(&[graph_id])
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn cancellation_before_a_batch_returns_writes_nothing() {
    let (_dir, app) = test_app().await;
    super::consent::set(&app.state_store, ImportKind::Mail, true).unwrap();
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let source = Arc::new(BlockingSource {
        entered: entered.clone(),
        release: release.clone(),
        item: mail_item(
            "cancelled",
            "Cancelled plan",
            "Reviewed a plan that should not be stored.",
        ),
    });
    let runner = ImportRunner::new(
        vec![source],
        ImportEmbedder::Given(Some(Arc::new(crate::embedding::Embedder::mock_for_tests()))),
    );
    runner.start(app.clone(), "mail", 30).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !entered.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("scan starts");
    assert!(runner.cancel("mail"));
    release.store(true, Ordering::SeqCst);
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while runner.status("mail").state == RunState::Running {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("scan stops");
    assert_eq!(runner.status("mail").state, RunState::Cancelled);
    assert_eq!(
        app.store
            .count_memories_by_source_type("import:mail")
            .await
            .unwrap(),
        0
    );
}

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn connectors_never_reach_the_store() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/import/sources");
    let mut files = Vec::new();
    rust_files(&dir, &mut files);
    assert!(!files.is_empty(), "sources/mod.rs must exist");
    for file in files {
        let text = std::fs::read_to_string(&file).expect("read connector");
        assert!(
            !text.to_lowercase().contains("lance_store"),
            "{} names lance_store; connectors return ImportItems and never write",
            file.display()
        );
    }
}

#[tokio::test]
async fn range_delete_tombstones_an_imported_row_so_a_rerun_does_not_bring_it_back() {
    let (_dir, app) = test_app().await;
    let item = mail_item("range-erased", "Atlas plan", "Reviewed the Atlas plan.");
    let source = Arc::new(FakeSource {
        items: parking_lot::Mutex::new(vec![item.clone()]),
    });
    let runner = ImportRunner::new(
        vec![source],
        ImportEmbedder::Given(Some(Arc::new(crate::embedding::Embedder::mock_for_tests()))),
    );
    assert_eq!(run_to_end(&runner, app.clone()).await.stored, 1);
    let report = crate::ipc::commands::erase_memories_in_range(
        &app.store,
        &app.graph,
        item.timestamp_ms - 1,
        item.timestamp_ms + 1,
        false,
        Some(&*app.state_store),
    )
    .await
    .unwrap();
    assert_eq!(report.deleted, 1);
    assert_eq!(
        app.store
            .count_memories_by_source_type("import:mail")
            .await
            .unwrap(),
        0
    );
    let rerun = run_to_end(&runner, app.clone()).await;
    assert_eq!(rerun.stored, 0);
    assert_eq!(rerun.skip_reasons.get("erased_by_person"), Some(&1));
}

#[tokio::test]
async fn forget_removes_an_imported_row_through_the_shared_erase_helper() {
    let (_dir, app) = test_app().await;
    let item = mail_item("forgotten", "Atlas plan", "Reviewed the Atlas plan.");
    let source = Arc::new(FakeSource {
        items: parking_lot::Mutex::new(vec![item.clone()]),
    });
    let runner = ImportRunner::new(
        vec![source],
        ImportEmbedder::Given(Some(Arc::new(crate::embedding::Embedder::mock_for_tests()))),
    );
    assert_eq!(run_to_end(&runner, app.clone()).await.stored, 1);
    assert_eq!(runner.forget(&app, "mail").await.unwrap(), 1);
    assert!(app
        .store
        .get_memory_by_id(&item.record_id())
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        app.store
            .count_memories_by_source_type("import:mail")
            .await
            .unwrap(),
        0
    );
}
