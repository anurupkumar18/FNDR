//! `fndr.remember`: an assistant saves a short note into the person's memory
//! (VS-68; contract in `docs/product/fndr-remember-spec.md`).
//!
//! The smallest safe slice. A write needs a valid token; the kill switch and
//! "Let assistants add notes" gate it (`risk_policy::decide_mcp_write`, run in
//! `call_tool` before dispatch). Then, cheapest first: rate limits, arguments,
//! text, the capture secret detector, the blocklist, and the capture
//! embedder. A note that passes becomes one leaf row with
//! `source_type = "agent"`: stored whole, found by the normal routes, never
//! merged, reviewed, reopened, or fed to derived artifacts. No refusal
//! message repeats the note, its title, or what matched.

use crate::embedding::{Embedder, EmbeddingBackend};
use crate::memory_compaction::{is_low_signal_embedding, mean_pool_embeddings};
use crate::memory_embedding_document::compose_memory_embedding_document;
use crate::privacy::safety_gate::{self, SafetyDecision};
use crate::storage::{MemoryRecord, AGENT_NOTE_SESSION_PREFIX, AGENT_NOTE_SOURCE_TYPE};
use crate::AppState;
use parking_lot::Mutex;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

pub(super) const TOOL_NAME: &str = "fndr.remember";
const APP_NAME: &str = "Agent note";
pub(super) const UNKNOWN_CLIENT: &str = "Unknown client";
const KINDS: [&str; 4] = ["note", "decision", "summary", "todo"];
const FIELDS: [&str; 5] = ["kind", "text", "title", "related_memory_ids", "project"];
const TEXT_MAX_CHARS: usize = 4_000;
const TITLE_MAX_CHARS: usize = 120;
const PROJECT_MAX_CHARS: usize = 80;
const RELATED_IDS_MAX: usize = 10;
const CLIENT_NAME_MAX_CHARS: usize = 64;
const SUMMARY_MAX_CHARS: usize = 240;
const ARGUMENTS_MAX_BYTES: usize = 32 * 1024;

pub(super) fn tool_listing() -> Value {
    json!({
        "name": TOOL_NAME,
        "description": "Save a short note, decision, summary, or to-do into the person's FNDR memory. FNDR stores it as a separate memory labeled as added by your client, never as something FNDR observed. Do not include secrets: notes that look like credentials are refused. Limits: 4000 characters per note, 10 notes per minute.",
        "inputSchema": {
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "kind": { "type": "string", "enum": KINDS },
                "text": { "type": "string", "minLength": 1, "maxLength": TEXT_MAX_CHARS },
                "title": { "type": "string", "maxLength": TITLE_MAX_CHARS },
                "related_memory_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "maxItems": RELATED_IDS_MAX,
                    "uniqueItems": true
                },
                "project": { "type": "string", "maxLength": PROJECT_MAX_CHARS }
            },
            "required": ["kind", "text"]
        }
    })
}

/// A refused call: a stable code, a fixed message, and nothing from the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Refusal {
    pub code: &'static str,
    message: String,
    retry_after_seconds: Option<u64>,
    limit: Option<&'static str>,
}

impl Refusal {
    pub(super) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retry_after_seconds: None,
            limit: None,
        }
    }

    pub(super) fn into_tool_result(self) -> Value {
        let mut structured = json!({ "error": self.code, "message": self.message });
        if let Some(seconds) = self.retry_after_seconds {
            structured["retry_after_seconds"] = json!(seconds);
        }
        if let Some(limit) = self.limit {
            structured["limit"] = json!(limit);
        }
        json!({
            "isError": true,
            "content": [{ "type": "text", "text": self.message }],
            "structuredContent": structured
        })
    }
}

/// The client name a session reported at `initialize`, cleaned: trimmed,
/// without control characters, at most 64 characters. A missing name, or one
/// that claims to be FNDR, becomes "Unknown client". The name is still
/// self-reported, since the token is shared.
pub(super) fn sanitize_client_name(raw: Option<&str>) -> String {
    let cleaned = raw
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control() && !is_invisible(*c))
        .collect::<String>();
    let cleaned = cleaned
        .trim()
        .chars()
        .take(CLIENT_NAME_MAX_CHARS)
        .collect::<String>();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() || cleaned.to_lowercase().starts_with("fndr") {
        UNKNOWN_CLIENT.to_string()
    } else {
        cleaned.to_string()
    }
}

/// Bidirectional overrides and isolates, and Unicode tag characters: text a
/// person cannot see but a model can read.
fn is_invisible(c: char) -> bool {
    matches!(c as u32, 0x202A..=0x202E | 0x2066..=0x2069 | 0xE0000..=0xE007F)
}

/// C0 controls other than tab, line feed, and carriage return; DEL; C1
/// controls; and the invisible characters above.
fn has_refused_characters(text: &str) -> bool {
    text.chars().any(|c| {
        let code = c as u32;
        (code < 0x20 && !matches!(c, '\t' | '\n' | '\r'))
            || code == 0x7F
            || (0x80..=0x9F).contains(&code)
            || is_invisible(c)
    })
}

#[derive(Debug, Clone, Copy)]
struct Window {
    name: &'static str,
    span_ms: i64,
    per_client: usize,
    global: usize,
}

const WINDOWS: [Window; 2] = [
    Window {
        name: "minute",
        span_ms: 60_000,
        per_client: 10,
        global: 30,
    },
    Window {
        name: "day",
        span_ms: 86_400_000,
        per_client: 200,
        global: 500,
    },
];

/// Remaining notes after a counted call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Remaining {
    pub this_minute: usize,
    pub today: usize,
}

/// Sliding-window limits per client name and for all clients together. The
/// global caps exist because names are self-reported: rotating names must
/// not multiply the budget. Only allowed calls are counted. A restart resets
/// the windows; the daily cap bounds damage, it is not a quota.
pub(super) struct RememberLimiter {
    clock: Box<dyn Fn() -> i64 + Send + Sync>,
    calls: Mutex<LimiterCalls>,
}

#[derive(Default)]
struct LimiterCalls {
    per_client: HashMap<String, VecDeque<i64>>,
    global: VecDeque<i64>,
}

impl RememberLimiter {
    pub(super) fn new() -> Self {
        Self::with_clock(|| chrono::Utc::now().timestamp_millis())
    }

    pub(super) fn with_clock(clock: impl Fn() -> i64 + Send + Sync + 'static) -> Self {
        Self {
            clock: Box::new(clock),
            calls: Mutex::new(LimiterCalls::default()),
        }
    }

    fn check_and_count(&self, client: &str) -> Result<Remaining, Refusal> {
        let now = (self.clock)();
        let day = WINDOWS[1].span_ms;
        let mut calls = self.calls.lock();
        calls.global.retain(|at| now - at < day);
        calls.per_client.retain(|_, list| {
            list.retain(|at| now - at < day);
            !list.is_empty()
        });
        let empty = VecDeque::new();
        let mine = calls.per_client.get(client).unwrap_or(&empty);

        // The binding limit is the one that frees a slot last.
        let mut refusal: Option<(u64, &'static str)> = None;
        for window in WINDOWS {
            for (list, limit, scope) in [
                (mine, window.per_client, "per_client"),
                (&calls.global, window.global, "global"),
            ] {
                let in_window = list
                    .iter()
                    .filter(|at| now - **at < window.span_ms)
                    .collect::<Vec<_>>();
                if in_window.len() < limit {
                    continue;
                }
                let frees_at = in_window[in_window.len() - limit] + window.span_ms;
                let seconds = (((frees_at - now) as f64) / 1000.0).ceil().max(1.0) as u64;
                if refusal.map_or(true, |(best, _)| seconds > best) {
                    refusal = Some((seconds, limit_label(scope, window.name)));
                }
            }
        }
        if let Some((seconds, label)) = refusal {
            let who = if label.starts_with("per_client") {
                "this client"
            } else {
                "assistants"
            };
            return Err(Refusal {
                code: "rate_limited",
                message: format!("Too many notes from {who}. Try again in {seconds} seconds."),
                retry_after_seconds: Some(seconds),
                limit: Some(label),
            });
        }

        calls.global.push_back(now);
        let mine = calls.per_client.entry(client.to_string()).or_default();
        mine.push_back(now);
        let used = |span: i64| mine.iter().filter(|at| now - **at < span).count();
        Ok(Remaining {
            this_minute: WINDOWS[0].per_client - used(WINDOWS[0].span_ms),
            today: WINDOWS[1].per_client - used(WINDOWS[1].span_ms),
        })
    }
}

fn limit_label(scope: &str, window: &str) -> &'static str {
    match (scope, window) {
        ("per_client", "minute") => "per_client_per_minute",
        ("per_client", _) => "per_client_per_day",
        (_, "minute") => "global_per_minute",
        _ => "global_per_day",
    }
}

/// Where the note's vectors come from. Production uses the shared capture
/// embedder and refuses when it is not the real model; tests pass one in.
#[derive(Clone)]
pub(super) enum NoteEmbedder {
    Shared,
    /// Tests: this embedder, mock or not; `None` is a missing model.
    #[cfg(test)]
    Given(Option<Arc<Embedder>>),
}

impl NoteEmbedder {
    fn with<T>(&self, f: impl FnOnce(&Embedder) -> Option<T>) -> Option<T> {
        match self {
            NoteEmbedder::Shared => crate::ipc::commands::common::shared_embedder()
                .ok()
                .filter(|embedder| matches!(embedder.backend(), EmbeddingBackend::Real))
                .and_then(|embedder| {
                    let result = f(embedder);
                    // Even with development mock fallback enabled, a failed
                    // real inference must not admit a mock-vector note.
                    if matches!(embedder.backend(), EmbeddingBackend::Real) {
                        result
                    } else {
                        None
                    }
                }),
            #[cfg(test)]
            NoteEmbedder::Given(embedder) => embedder.as_deref().and_then(f),
        }
    }
}

/// What the transport knows about the caller.
#[derive(Clone)]
pub(super) struct WriteCaller {
    /// The client name from this session's `initialize`, already sanitized.
    pub client: String,
    pub limiter: Arc<RememberLimiter>,
    pub embedder: NoteEmbedder,
}

struct NoteArgs {
    kind: &'static str,
    text: String,
    title: String,
    related_memory_ids: Vec<String>,
    project: String,
}

/// Runs one `fndr.remember` call whose transport auth and write gate already
/// passed. Returns a tool result: stored, or refused with a stable code.
pub(super) async fn run(app_state: Arc<AppState>, caller: &WriteCaller, arguments: Value) -> Value {
    match remember(&app_state, caller, arguments).await {
        Ok(stored) => super::tool_success(stored),
        Err(refusal) => {
            tracing::info!(
                client = %caller.client,
                code = refusal.code,
                "mcp:fndr_remember_refused"
            );
            refusal.into_tool_result()
        }
    }
}

async fn remember(
    app_state: &AppState,
    caller: &WriteCaller,
    arguments: Value,
) -> Result<Value, Refusal> {
    let remaining = caller.limiter.check_and_count(&caller.client)?;
    let args = parse_arguments(arguments)?;
    check_related_ids(app_state, &args.related_memory_ids).await?;

    // Every caller-supplied text field that is persisted and embedded must
    // pass the same secret and blocklist checks, including the project label.
    let combined = format!("{}\n{}\n{}", args.title, args.text, args.project);
    if safety_gate::evaluate(None, None, None, None, Some(&combined), &[]) != SafetyDecision::Allow
    {
        return Err(Refusal::new(
            "sensitive_content",
            "This note looks like it contains a password, key, or token, so FNDR did not save it. Remove the secret and try again.",
        ));
    }
    let blocklist = app_state.config.read().blocklist.clone();
    let lowered = combined.to_lowercase();
    if blocklist
        .iter()
        .map(|entry| entry.trim().to_lowercase())
        .any(|entry| !entry.is_empty() && lowered.contains(&entry))
    {
        return Err(Refusal::new(
            "blocklisted_content",
            "This note mentions a site or word on the person's blocklist, so FNDR did not save it.",
        ));
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    let mut record = build_record(&args, &caller.client, now_ms);
    embed(&mut record, &caller.embedder)?;
    app_state
        .store
        .add_batch_preserving_ids(std::slice::from_ref(&record))
        .await
        .map_err(|err| {
            tracing::warn!(%err, "mcp:fndr_remember_store_failed");
            Refusal::new("storage_failed", "FNDR could not save the note.")
        })?;
    app_state.invalidate_memory_derived_caches();
    tracing::info!(
        client = %caller.client,
        kind = args.kind,
        char_count = args.text.chars().count(),
        "mcp:fndr_remember_stored"
    );

    Ok(json!({
        "status": "stored",
        "memory_id": record.id,
        "source_type": AGENT_NOTE_SOURCE_TYPE,
        "added_by": caller.client,
        "kind": args.kind,
        "created_at": now_ms,
        "char_count": args.text.chars().count(),
        "remaining": { "this_minute": remaining.this_minute, "today": remaining.today }
    }))
}

fn parse_arguments(arguments: Value) -> Result<NoteArgs, Refusal> {
    let invalid = |what: &str| {
        Refusal::new(
            "invalid_arguments",
            format!("fndr.remember arguments are invalid: {what}."),
        )
    };
    if serde_json::to_vec(&arguments).map_or(true, |bytes| bytes.len() > ARGUMENTS_MAX_BYTES) {
        return Err(invalid("larger than 32 KiB"));
    }
    let Value::Object(map) = arguments else {
        return Err(invalid("expected an object"));
    };
    if map.keys().any(|key| !FIELDS.contains(&key.as_str())) {
        return Err(Refusal::new(
            "unknown_argument",
            "fndr.remember accepts only kind, text, title, related_memory_ids, and project.",
        ));
    }
    let kind = match map.get("kind").and_then(Value::as_str) {
        Some(kind) => KINDS.iter().copied().find(|known| *known == kind),
        None => None,
    }
    .ok_or_else(|| {
        Refusal::new(
            "invalid_kind",
            "kind must be one of note, decision, summary, or todo.",
        )
    })?;
    let related_memory_ids = match map.get("related_memory_ids") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| item.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| invalid("related_memory_ids must be strings"))?,
        Some(_) => return Err(invalid("related_memory_ids must be an array")),
    };
    if related_memory_ids.len() > RELATED_IDS_MAX {
        return Err(Refusal::new(
            "too_many_related_ids",
            "A note can link at most 10 memories.",
        ));
    }

    let text = optional_string(&map, "text").map_err(|_| invalid("text must be a string"))?;
    let title = optional_string(&map, "title").map_err(|_| invalid("title must be a string"))?;
    let project =
        optional_string(&map, "project").map_err(|_| invalid("project must be a string"))?;
    // Checked before trimming: `trim` would quietly drop a trailing NEL.
    if [text, title, project]
        .iter()
        .any(|value| has_refused_characters(value))
    {
        return Err(Refusal::new(
            "invalid_characters",
            "The note contains hidden or control characters, so FNDR did not save it.",
        ));
    }
    let (text, title, project) = (text.trim(), title.trim(), project.trim());
    if text.is_empty() {
        return Err(Refusal::new("empty_text", "A note needs text."));
    }
    if text.chars().count() > TEXT_MAX_CHARS {
        return Err(Refusal::new(
            "text_too_long",
            "A note can be at most 4,000 characters. FNDR does not shorten notes, so nothing was saved.",
        ));
    }
    if title.chars().count() > TITLE_MAX_CHARS || title.contains(['\n', '\r']) {
        return Err(Refusal::new(
            "title_too_long",
            "A title can be at most 120 characters on one line.",
        ));
    }
    if project.chars().count() > PROJECT_MAX_CHARS {
        return Err(Refusal::new(
            "project_too_long",
            "A project label can be at most 80 characters.",
        ));
    }
    Ok(NoteArgs {
        kind,
        text: text.to_string(),
        title: title.to_string(),
        related_memory_ids,
        project: project.to_string(),
    })
}

fn optional_string<'a>(map: &'a Map<String, Value>, key: &str) -> Result<&'a str, ()> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(""),
        Some(Value::String(value)) => Ok(value),
        Some(_) => Err(()),
    }
}

async fn check_related_ids(app_state: &AppState, ids: &[String]) -> Result<(), Refusal> {
    let unknown = || {
        Refusal::new(
            "unknown_related_memory_id",
            "A linked memory id does not exist.",
        )
    };
    let mut seen = std::collections::HashSet::new();
    for id in ids {
        if !seen.insert(id.as_str()) {
            return Err(Refusal::new(
                "invalid_arguments",
                "fndr.remember arguments are invalid: related_memory_ids must be unique.",
            ));
        }
        match app_state.store.get_memory_by_id(id).await {
            Ok(Some(memory)) if !memory.is_soft_deleted => {}
            _ => return Err(unknown()),
        }
    }
    Ok(())
}

fn build_record(args: &NoteArgs, client: &str, now_ms: i64) -> MemoryRecord {
    let id = uuid::Uuid::new_v4().to_string();
    let title = if args.title.is_empty() {
        let mut kind = args.kind.to_string();
        kind[..1].make_ascii_uppercase();
        format!("{kind} from {client}")
    } else {
        args.title.clone()
    };
    let summary = first_sentence(&args.text);
    let provenance = json!({
        "agent_provenance": {
            "version": 1,
            "client": client,
            "client_name_source": if client == UNKNOWN_CLIENT { "absent" } else { "mcp_initialize" },
            "tool": TOOL_NAME,
            "kind": args.kind,
            "created_at_ms": now_ms,
        }
    });
    MemoryRecord {
        id: id.clone(),
        timestamp: now_ms,
        timestamp_start: now_ms,
        timestamp_end: now_ms,
        day_bucket: chrono::Local::now().format("%Y-%m-%d").to_string(),
        source_type: AGENT_NOTE_SOURCE_TYPE.to_string(),
        related_agents: vec![client.to_string()],
        related_tools: vec![TOOL_NAME.to_string()],
        raw_evidence: provenance.to_string(),
        storage_outcome: "agent_note".to_string(),
        activity_type: args.kind.to_string(),
        tags: vec!["agent_note".to_string(), args.kind.to_string()],
        app_name: APP_NAME.to_string(),
        window_title: title,
        text: args.text.clone(),
        clean_text: args.text.clone(),
        memory_context: args.text.clone(),
        snippet: summary.clone(),
        display_summary: summary,
        related_memory_ids: args.related_memory_ids.clone(),
        project: args.project.clone(),
        session_key: format!("{AGENT_NOTE_SESSION_PREFIX}{id}"),
        enrichment_status: "agent_note".to_string(),
        confidence_score: 1.0,
        importance_score: 0.6,
        summary_source: "fallback".to_string(),
        ..MemoryRecord::default()
    }
}

fn first_sentence(text: &str) -> String {
    let sentence = text
        .split_inclusive(['.', '!', '?', '\n'])
        .next()
        .unwrap_or(text)
        .trim();
    sentence.chars().take(SUMMARY_MAX_CHARS).collect()
}

/// The capture embedder and the same document composition capture uses.
/// Refuses rather than store a note search cannot find by meaning.
fn embed(record: &mut MemoryRecord, embedder: &NoteEmbedder) -> Result<(), Refusal> {
    let document = compose_memory_embedding_document(record, None);
    let context = |text: &String| {
        (
            APP_NAME.to_string(),
            record.window_title.clone(),
            text.clone(),
        )
    };
    let inputs = document
        .text_embedding_inputs()
        .iter()
        .map(context)
        .collect::<Vec<_>>();
    let vectors = embedder
        .with(|embedder| embedder.embed_batch_with_context(&inputs).ok())
        .filter(|vectors| {
            vectors.len() == inputs.len() && !vectors.iter().any(|v| is_low_signal_embedding(v))
        })
        .ok_or_else(|| {
            Refusal::new(
                "embedder_unavailable",
                "FNDR's embedding model is not available, so the note was not saved.",
            )
        })?;
    record.embedding = vectors[0].clone();
    record.snippet_embedding = vectors[1].clone();
    record.support_embedding = if vectors.len() > 2 {
        mean_pool_embeddings(&vectors[2..])
    } else {
        vec![0.0; vectors[0].len()]
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    fn limiter_at(clock: Arc<AtomicI64>) -> RememberLimiter {
        RememberLimiter::with_clock(move || clock.load(Ordering::SeqCst))
    }

    #[test]
    fn rate_limit_returns_retry_after() {
        let clock = Arc::new(AtomicI64::new(1_000_000));
        let limiter = limiter_at(clock.clone());
        for call in 0..10 {
            clock.store(1_000_000 + call * 5_000, Ordering::SeqCst);
            limiter
                .check_and_count("Claude Code")
                .expect("within the limit");
        }
        clock.store(1_000_000 + 50_000, Ordering::SeqCst);
        let refused = limiter.check_and_count("Claude Code").unwrap_err();
        assert_eq!(refused.code, "rate_limited");
        assert_eq!(refused.limit, Some("per_client_per_minute"));
        assert_eq!(refused.retry_after_seconds, Some(10));
        // Another client is not limited by this one's minute.
        limiter.check_and_count("Cursor").expect("other client");
        // A refused call is not counted: once the oldest call ages out, one more fits.
        clock.store(1_000_000 + 60_000, Ordering::SeqCst);
        limiter
            .check_and_count("Claude Code")
            .expect("a slot freed");
    }

    #[test]
    fn global_daily_cap_holds_across_client_names() {
        let clock = Arc::new(AtomicI64::new(0));
        let limiter = limiter_at(clock.clone());
        let clients = ["A", "B", "C", "D", "E", "F"];
        for call in 0..500_i64 {
            clock.store(call * 86_400_000 / 501, Ordering::SeqCst);
            limiter
                .check_and_count(clients[call as usize % clients.len()])
                .unwrap_or_else(|refusal| panic!("call {call} refused: {refusal:?}"));
        }
        clock.store(500 * 86_400_000 / 501, Ordering::SeqCst);
        let refused = limiter.check_and_count("G").unwrap_err();
        assert_eq!(refused.limit, Some("global_per_day"));
        assert!(refused.retry_after_seconds.unwrap() >= 1);
    }

    #[test]
    fn client_names_are_cleaned_and_fndr_names_refused() {
        assert_eq!(sanitize_client_name(Some("  Claude Code ")), "Claude Code");
        assert_eq!(sanitize_client_name(Some("FNDR Capture")), UNKNOWN_CLIENT);
        assert_eq!(sanitize_client_name(Some("fndr")), UNKNOWN_CLIENT);
        assert_eq!(sanitize_client_name(Some("")), UNKNOWN_CLIENT);
        assert_eq!(sanitize_client_name(None), UNKNOWN_CLIENT);
        assert_eq!(
            sanitize_client_name(Some("Cur\u{0007}sor\u{202E}")),
            "Cursor"
        );
        assert_eq!(sanitize_client_name(Some(&"x".repeat(100))).len(), 64);
    }

    #[test]
    fn remember_text_limit_counts_characters_not_bytes() {
        let args = |text: String| json!({ "kind": "note", "text": text });
        let ascii = "a".repeat(4_000);
        let multibyte = "\u{00e9}".repeat(4_000);
        assert_eq!(
            parse_arguments(args(ascii)).unwrap().text.chars().count(),
            4_000
        );
        assert_eq!(
            parse_arguments(args(multibyte.clone())).unwrap().text,
            multibyte
        );
        let refused = parse_arguments(args("a".repeat(4_001))).err().unwrap();
        assert_eq!(refused.code, "text_too_long");
    }

    #[test]
    fn remember_rejects_invisible_characters() {
        for text in [
            "ok\u{E0049}\u{E0047}hidden",
            "right\u{202E}to left",
            "isolate\u{2066}x",
            "bell\u{0007}",
            "c1\u{0085}",
            "del\u{007F}",
        ] {
            let refused = parse_arguments(json!({ "kind": "note", "text": text }))
                .err()
                .unwrap_or_else(|| panic!("{text:?} was accepted"));
            assert_eq!(refused.code, "invalid_characters", "{text:?}");
        }
        let kept = parse_arguments(json!({ "kind": "note", "text": "tab\there\nline\r\nend" }));
        assert!(kept.is_ok());
    }

    #[test]
    fn arguments_refuse_in_spec_order_and_never_echo() {
        let code = |args: Value| parse_arguments(args).err().map(|refusal| refusal.code);
        assert_eq!(
            code(json!({ "kind": "bogus", "text": "x", "source_type": "screen" })),
            Some("unknown_argument")
        );
        assert_eq!(
            code(json!({ "kind": "policy", "text": "x" })),
            Some("invalid_kind")
        );
        assert_eq!(code(json!({ "text": "x" })), Some("invalid_kind"));
        assert_eq!(
            code(json!({ "kind": "note", "text": "   " })),
            Some("empty_text")
        );
        assert_eq!(code(json!({ "kind": "note" })), Some("empty_text"));
        assert_eq!(
            code(json!({ "kind": "note", "text": "x", "title": "two\nlines" })),
            Some("title_too_long")
        );
        assert_eq!(
            code(json!({ "kind": "note", "text": "x", "related_memory_ids": vec!["m"; 11] })),
            Some("too_many_related_ids")
        );
        assert_eq!(code(json!(["kind", "note"])), Some("invalid_arguments"));
        let refused = parse_arguments(json!({ "kind": "note", "text": "SECRETWORD", "zz": 1 }))
            .err()
            .unwrap();
        assert!(!refused
            .into_tool_result()
            .to_string()
            .contains("SECRETWORD"));
    }
}
