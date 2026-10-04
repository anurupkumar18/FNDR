# `fndr.remember`: agent write-back spec

Status: Proposed 2026-10-04, waiting for owner review.
Ticket: VS-35 (`docs/team/tickets/anurup-followups-2026-10.md`).
Plan: month plan section 5 priority 6 and section 6 "Assistants write back" and "Trust" (`docs/team/2026-10-month-plan.md`).
Corpus: `src-tauri/tests/fixtures/agent_notes/injected-notes.json`.
Evidence and open questions: `docs/evidence/W03/VS-35-cloud.md`.

Nothing in this document is implemented. Sections marked **Today** describe
the code at commit `b948dfd` and cite it. Everything else is a **proposal**.

## 1. Purpose

An assistant connected over MCP (Claude Code, Cursor, Codex) can save a short
note, decision, summary, or to-do into the person's FNDR memory, so the next
session, the next assistant, or the person can find it. FNDR stores it as its
own memory, labeled with who wrote it, and never as something FNDR observed.

The write path is also a prompt-injection door: anything an assistant writes
can later be read by FNDR's own models, by other assistants, and by the person.
This spec defines the contract and the defenses, and the corpus is the test set
that proves them.

Design rule: **an agent note is a leaf record.** It is stored, embedded, found,
shown with its provenance, and deleted. Nothing else in FNDR reads note text as
input to a decision, a policy, a tool, a setting, or a derived artifact.

## 2. Today (verified in code)

| Fact | Where |
| --- | --- |
| No `fndr.remember` tool. 52 tools are registered; the only write tools are `agent.rate_result` and `fndr_remember_decision`. | `src-tauri/src/mcp/mod.rs:1401` (`tools_list_result`), audit script in `docs/product/mcp-tool-audit.md` |
| `fndr_remember_decision` appends to the decision ledger with no size limit, no rate limit, no secret check, and a self-declared `proposed_by` argument; with a `project` it also rebuilds that project's context. | `src-tauri/src/mcp/mod.rs:1955-1968`, `src-tauri/src/context_runtime/mod.rs:667-719` |
| Every `tools/call` needs `Authorization: Bearer <token>`; only `initialize` and `tools/list` from a loopback peer are exempt. | ADR-017, `src-tauri/src/mcp/mod.rs:880-897` (`should_bypass_http_auth`) |
| The exemption is decided once per HTTP payload from the first method found, so a JSON-RPC batch is judged by its first item; `handle_payload` then runs every item. | `src-tauri/src/mcp/mod.rs:953-959` (`jsonrpc_method_hint`), `:1052-1066`, `:1166-1180` |
| The token is one shared UUID in `~/.fndr/mcp_token` (mode 0600). Any process running as the person can read it; it does not identify a client. | `src-tauri/src/mcp/token.rs:13-62` |
| `initialize` ignores `clientInfo`; only `protocolVersion` is read. There is no per-session state for POST requests. | `src-tauri/src/mcp/mod.rs:1245-1265`, `:1037-1084` |
| MCP side-effect tools go through `risk_policy::decide`, which returns `Confirm` for every MCP caller; `Confirm` currently answers "needs the person's approval ... not run". | `src-tauri/src/agent/risk_policy.rs:37-50`, `:62-70`, `src-tauri/src/mcp/mod.rs:2086-2104` |
| The capture privacy gate **skips** a frame whose text matches a secret pattern. `safety_gate::evaluate` returns `Redact`, but capture maps any non-`Allow` decision to `SkipReason::SensitiveContext` and stores nothing. | `src-tauri/src/privacy/safety_gate.rs:130-153`, `src-tauri/src/capture/mod.rs:651-679`, `:2924-2941`, test `secret_pattern_text_is_rejected_by_capture_admission` at `:6736` |
| There is no text redaction anywhere. `redact_mode` exists in config but is only reported by `agent.privacy_status`, never applied. | `src-tauri/src/config.rs:705-706`, `src-tauri/src/mcp/mod.rs:1357`, `:3324` |
| `MemoryRecord.source_type` exists (default `"screen"`). Values written today: `screen`, `browser`, `screen_visual`, `browser_visual`, `browser_url_only`, `iphone_manual_capture` (and the watch variant), `meta_glasses_import`. No `agent`. | `src-tauri/src/storage/schema.rs:54-56`, `:297-298`; `src-tauri/src/storage/lance_store/schemas.rs:101`; `src-tauri/src/capture/mod.rs:459-463`, `:2430`, `:3783-3787`; `src-tauri/src/companion/handlers/memories.rs:127-140` |
| MCP read tools do not report the stored `source_type`; they infer one from URL and app name, so an app name containing "code" is labeled `coding`. `fndr.search` cards (`MemoryCard`) and `SearchResult` carry no `source_type` field at all. | `src-tauri/src/mcp/mod.rs:4646`, `:4687`, `:4889-4909`; `src-tauri/src/search/memory_cards.rs:21-121`; `src-tauri/src/storage/schema.rs:614` |
| Context packs classify source type by heuristics (meeting, Finder, URL, terminal, else `screen`) unless a hint is passed. | `src-tauri/src/context_runtime/mod.rs:2348-2363` |
| The closest precedent is the Companion manual note: 8,000 character cap, 32 KiB body, provenance from the authenticated device, `source_type` set by the server, `storage_outcome = "manual_capture"`. It never calls an embedder, so the row keeps the default all-zero vectors. | `src-tauri/src/companion/handlers/memories.rs:23`, `:33`, `:55-56`, `:142-187`; `src-tauri/src/storage/schema.rs:18-28` |
| Insert compaction empties `text` and truncates `clean_text` to 420 or 560 characters. | `src-tauri/src/memory_compaction.rs:7-9`, `:53-71`, called from `normalize_record_for_index` in `src-tauri/src/storage/lance_store/normalize_embed_migrate.rs:185-196` |
| Insert dedup only compares records inside one batch, not against stored rows. | `src-tauri/src/storage/lance_store/normalize_embed_migrate.rs:1408-1428`, `src-tauri/src/storage/lance_store/mod.rs:1020-1047` |
| Keyword search scans `text`, `clean_text`, `snippet`, `lexical_shadow`, `window_title`, `app_name`, `url`. | `src-tauri/src/storage/lance_store/mod.rs:1722-1728` |
| Capture merges a new frame into a stored memory and rewrites it; merged `source_type` prefers the incoming value. Candidates are filtered by app name, and cross-app merges need a matching URL or domain. | `src-tauri/src/capture/mod.rs:4627-4867`, `:4956-5021`, `:5032-5036`, `:5149`, `:5864-5879` |
| `sync_memory_record` feeds the graph, activity events, entity aliases, project context, and knowledge pages from a memory. | `src-tauri/src/context_runtime/mod.rs:156-211` |
| Memory review (local LLM) picks memories by `enrichment_status` only, with no `source_type` check, and rewrites fields. | `src-tauri/src/memory_review/backfill.rs:85-90`, `src-tauri/src/memory_review/daily.rs:187` |
| The index session key is rebuilt from app and title on insert, except `meeting:` keys; cards group results that share a key within 5 minutes. | `src-tauri/src/storage/lance_store/normalize_embed_migrate.rs:1289-1314`, `src-tauri/src/search/memory_cards.rs:379-433` |
| Agent context packs drop any memory whose app name starts with "fndr" (`is_internal_app`). | `src-tauri/src/agent/context.rs:376-386`, `src-tauri/src/privacy/blocklist.rs:70-83` |
| Adding a blocklist entry deletes memories whose `window_title` or `url` contains it. | `src-tauri/src/ipc/commands/privacy.rs:125-170`, `src-tauri/src/storage/lance_store/mod.rs:1637-1652` |
| The person can delete one memory; it also cleans linked tasks and the graph node. | `src-tauri/src/ipc/commands/memory.rs:16-74` |
| The expanded card's provenance strip says "Captured" and shows `app_name` as "Source". | `src/domains/memory-vault/MemoryProvenanceStrip.tsx:15`, `:27` |
| Memory text, like screen text, may never choose a tool or supply an argument. | `docs/product/command-surface.md` invariant 1, `docs/product/actions-policy.md` "What never ships" |

## 3. Tool contract (proposal)

### 3.1 Listing

```json
{
  "name": "fndr.remember",
  "description": "Save a short note, decision, summary, or to-do into the person's FNDR memory. FNDR stores it as a separate memory labeled as added by your client, never as something FNDR observed. Do not include secrets: notes that look like credentials are refused. Limits: 4000 characters per note, 10 notes per minute.",
  "inputSchema": {
    "type": "object",
    "additionalProperties": false,
    "properties": {
      "kind": { "type": "string", "enum": ["note", "decision", "summary", "todo"] },
      "text": { "type": "string", "minLength": 1, "maxLength": 4000 },
      "title": { "type": "string", "maxLength": 120 },
      "related_memory_ids": { "type": "array", "items": { "type": "string" }, "maxItems": 10, "uniqueItems": true },
      "project": { "type": "string", "maxLength": 80 }
    },
    "required": ["kind", "text"]
  }
}
```

The schema is advice to the client. The server enforces every rule itself:
arguments deserialize with `deny_unknown_fields`, so `source_type`,
`app_name`, `url`, `timestamp`, `created_at`, `client`, or anything else is
refused with `unknown_argument`.

`kind` is a label, not a promotion. A `todo` does not create a Task, a
`decision` does not enter the decision ledger, and `project` does not rebuild
project context (section 9).

### 3.2 Order of checks

Cheapest first, and nothing is echoed back from the text.

1. **Transport auth** per JSON-RPC item (section 5). Fails: HTTP 401, `-32001`.
2. **Write gate**: auth disabled, kill switch, notes setting (section 5).
3. **Rate limit** per client and global (section 7). Counted here, so rejected
   content still costs quota; calls refused by the limiter are not counted.
4. **Arguments**: unknown field, bad `kind`, too many or unknown
   `related_memory_ids` (must exist and not be soft-deleted).
5. **Text**: trim; refuse `invalid_characters` (C0 controls other than tab,
   line feed, carriage return; DEL; C1 controls; bidirectional overrides and
   isolates U+202A to U+202E and U+2066 to U+2069; Unicode tag characters
   U+E0000 to U+E007F); refuse `empty_text`; refuse `text_too_long`. `title`
   follows the same rules and may not contain a line break; `project` is at
   most 80 characters.
6. **Secrets** (section 8): `sensitive_content`.
7. **Blocklist** (section 8): `blocklisted_content`.
8. **Embed** (section 9): `embedder_unavailable` if any text vector is missing
   or all zeros.
9. **Insert** one row; invalidate derived caches; write the Privacy Activity
   line (section 10).

### 3.3 Response

Success is a normal `tool_success` payload (`src-tauri/src/mcp/mod.rs:5062`):

```json
{
  "status": "stored",
  "memory_id": "6f1c0c1e-7d0a-4a43-9b0e-3c2a8f1d2b77",
  "source_type": "agent",
  "added_by": "Claude Code",
  "kind": "decision",
  "created_at": 1759654800000,
  "char_count": 412,
  "remaining": { "this_minute": 9, "today": 187 }
}
```

A refusal is a tool result with `isError: true`, a fixed human message, and
`structuredContent` with a stable code (the existing `tool_error` at
`src-tauri/src/mcp/mod.rs:5074` gains `structuredContent`):

```json
{ "error": "rate_limited", "message": "Too many notes from this client. Try again in 42 seconds.", "retry_after_seconds": 42, "limit": "per_client_per_minute" }
```

Codes: `unauthorized` (transport, HTTP 401), `auth_required_for_writes`,
`actions_off`, `notes_disabled`, `rate_limited`, `unknown_argument`,
`invalid_kind`, `empty_text`, `text_too_long`, `title_too_long`,
`invalid_characters`, `too_many_related_ids`, `unknown_related_memory_id`,
`sensitive_content`, `blocklisted_content`, `embedder_unavailable`. Messages
never quote the note, the title, or the matched pattern.

## 4. Provenance and storage (proposal)

One new row in the existing memories table. **No LanceDB schema change**: every
field below is already a column (`src-tauri/src/storage/lance_store/schemas.rs`).

| What | Field | Value | Why |
| --- | --- | --- | --- |
| Source | `source_type` | `"agent"` | Set by the server only. The one field every surface keys on. |
| Client | `related_agents` | `[client]` | Existing list column; also in the provenance object below. |
| Tool | `related_tools` | `["fndr.remember"]` | Lets a later write tool coexist without a new column. |
| Time | `timestamp`, `timestamp_start`, `timestamp_end` | server clock, ms | No client-supplied time, so no backdating. `day_bucket` as capture computes it. |
| Full provenance | `raw_evidence` | JSON object with key `agent_provenance`: `{ "version": 1, "client": "Claude Code", "client_name_source": "mcp_initialize" or "absent", "tool": "fndr.remember", "kind": "decision", "created_at_ms": ..., "mcp_session": "<opaque id>" }` | Same additive pattern as `embedding_manifest` (ADR-010, `src-tauri/src/memory_embedding_document.rs:295-304`). |
| Origin label | `storage_outcome` | `"agent_note"` | Mirrors Companion's `"manual_capture"`. |
| Kind | `activity_type`, `tags` | `kind`; `["agent_note", kind]` | |
| App | `app_name` | `"Agent note"` (fixed) | Never the client name: a client called "Claude Code" would be inferred as `coding`, collide with the captured Claude app, and names starting with "FNDR" are dropped from agent packs. |
| Title | `window_title` | `title`, else `"<Kind> from <client>"` | Searchable; makes "what did Cursor note" work by keyword. |
| Body | `text`, `clean_text`, `memory_context` | full normalized text | Requires the compaction exemption below. |
| Summary | `snippet`, `display_summary` | first sentence, at most 240 characters | Same rule as Companion's `first_sentence`. |
| Links | `related_memory_ids` | validated ids | Links only; the linked memories are never edited. |
| Project | `project` | `project` or empty | A label for filtering; does not rebuild project context. |
| Grouping | `session_key` | `"agent_note:<memory id>"` | Each note is its own card. |
| Reopen | `url`, `reopen_*`, `screenshot_path` | none, `Unknown`, none | A note never becomes an open target. |
| Review | `enrichment_status` | `"agent_note"` | Not a review status; memory review skips the row by `source_type`. |
| Quality scores | `confidence_score`, `importance_score` | 1.0, 0.6 (Companion values, `memories.rs:182-183`) | No special boost; revisit with `make qa-retrieval`. |
| Id | `id` | server-generated UUID v4 | Never client-supplied. |

The client name comes from the MCP session, not from the arguments. A
prompt-injected model controls `tools/call` arguments, but not the
`clientInfo.name` its client software sends at `initialize`. Proposal: on
`initialize`, store `clientInfo.name` in a bounded session map (64 entries,
least recently used out) and return an `Mcp-Session-Id` header; a `tools/call`
carrying that header gets the name. Sanitize: trim, drop control characters,
at most 64 characters. Record `"Unknown client"` when the name is missing, empty,
or starts with "FNDR" (case-insensitive). The name is still self-reported (the
token is shared, section 2), so the UI says so (section 10).

Small code changes this requires (all additive):

1. `compact_memory_record_payload` keeps `text` and `clean_text` whole when
   `source_type == "agent"` (the tool already caps them at 4,000 characters).
2. `build_index_session_key` keeps `agent_note:` keys, as it keeps `meeting:`.
3. `SearchResult` and `MemoryCard` gain `source_type` (a projection of the
   existing column) and `added_by: Option<String>`; the TypeScript types follow.
4. MCP JSON rows (`src-tauri/src/mcp/mod.rs:4630-4690`) report the stored
   `source_type` when it is `agent`, plus `added_by`; `classify_source_type`
   returns `agent` before its heuristics.

## 5. Auth and the write gate (proposal)

- **Token required (ADR-017).** No write without a valid bearer token. The
  handshake exemption must be decided **per JSON-RPC item**: a batch that
  contains any method other than `initialize` or `tools/list` needs the token
  for the whole request. Today it does not (section 2), which affects every
  tool, so this fix should land first and on its own.
- **No writes when auth is off.** If `FNDR_MCP_REQUIRE_AUTH=0`, `fndr.remember`
  answers `auth_required_for_writes`. The opt-out exists for local read
  debugging; it must not open the write path.
- **Kill switch.** When `actions_kill_switch` is on, `fndr.remember` answers
  `actions_off`.
- **Notes setting.** A new `agent_notes_enabled` setting, default off, shown in
  Trust and Settings as "Let assistants add notes". Off answers
  `notes_disabled` with a message that names the setting. Rationale: ADR-022
  makes the step that widens collection (`resume_capture`) the deliberate one;
  letting assistants write into memory widens what memory holds.
- **Not a confirm-per-call tool.** Adding `fndr.remember` to
  `MCP_SIDE_EFFECT_TOOLS` would make every call answer "needs approval ... not
  run" (`src-tauri/src/mcp/mod.rs:2094-2098`). Instead `risk_policy` gets a
  separate MCP write list with its own decision (kill switch, then setting,
  then run) and the same "gate before dispatch" test as
  `mcp_call_tool_checks_the_gate_before_dispatching_any_tool`. The write is
  confined to one new, labeled, deletable row; it cannot change any other row,
  setting, or tool. The doc comment "Tools not listed here are read-only"
  (`risk_policy.rs:62-64`) is corrected at the same time.

## 6. Size limits (proposal)

| Limit | Value | Reasoning |
| --- | --- | --- |
| `text` | 1 to 4,000 characters (Unicode scalar values, after trim) | The embedding document reads up to 2,000 characters as primary text and up to four 720-character support chunks (`memory_embedding_document.rs:21`, `memory_compaction.rs:11-12`), so about 4,000 characters is what one memory row's vectors can represent. It is half the chunk source cap (`memory_embedding_document.rs:22`) and the Companion note cap (`memories.rs:23`), both 8,000, leaving room. About 600 words: a session summary fits; a document does not, and bulk import is out of scope. |
| `title` | at most 120 characters, one line | Card and window-title display. |
| `project` | at most 80 characters | A label. |
| `related_memory_ids` | at most 10, unique, existing | Links, not a bulk operation. |
| Request | arguments at most 32 KiB serialized | Same as the Companion body limit (`memories.rs:33`); the MCP router has no explicit limit today (`src-tauri/src/mcp/mod.rs:703-709`). |

Over a limit, the call is refused whole. FNDR never truncates and stores part
of a note, because a truncated decision can say the opposite of the original.

## 7. Rate limits (proposal)

| Scope | Per rolling 60 s | Per rolling 24 h |
| --- | --- | --- |
| One client name | 10 | 200 |
| All clients together | 30 | 500 |

- 10 per minute lets an assistant save a summary plus several decisions and
  to-dos at the end of a session; a loop hits the wall within seconds.
- 200 per client per day is about 25 an hour across an 8-hour day, more than a
  person will read, while keeping a runaway client's damage to one filtered
  bulk delete.
- The global caps exist because client names are self-reported: rotating names
  must not multiply the budget (corpus `an-030`).
- In-memory sliding windows keyed by sanitized client name. A restart resets
  them; acceptable because the daily cap is a damage bound, not a quota.
- Refusal: `rate_limited` with `retry_after_seconds` (seconds until the oldest
  call in the binding window expires, at least 1) and `limit` naming the
  window. HTTP stays 200 so clients surface the message to the model instead
  of treating it as a transport failure.

## 8. Secrets, blocklist, and privacy (proposal)

**Secrets: refuse with an error, using the same detector capture uses.**

- Capture does not redact; it skips the frame (section 2). The ticket's
  "redaction identical to screen text" therefore means: text that capture would
  not store, `fndr.remember` does not store either.
- Unlike capture, there is a caller who can fix the input, so FNDR refuses with
  `sensitive_content` instead of dropping silently. The message is fixed ("This
  note looks like it contains a password, key, or token, so FNDR did not save
  it. Remove the secret and try again.") and never repeats the text.
- Not redaction: the detector returns one decision for the whole text, not
  spans (`safety_gate.rs:20-155`), so redacting would mean a second, new
  detector. A redacted note could also keep the parts of a secret that the
  keyword match did not cover, while looking safe.
- Call: `safety_gate::evaluate(None, None, None, None, Some(title + "\n" + text), &[])`.
  Only the text rules apply (`SECRET_PATTERNS`, `SECRET_TOKEN_PREFIXES`). The
  window-title rules (sign in, banking, private browsing) describe what is on a
  screen, not what a sentence is about: a note titled "Login redirect bug" is
  allowed (corpus `an-003`).
- Count each refusal in Privacy Activity under "looked like a secret", with no
  content.
- Coverage is whatever the shared detector covers. Extending it (for example to
  cloud access key ids or private key blocks) belongs in `safety_gate.rs`, so
  capture benefits too (open question in the evidence file).

**Blocklist.** Refuse `blocklisted_content` when any blocklist entry
(lowercased, trimmed, non-empty) is a substring of the lowercased title or
text. This is the same substring rule the retroactive cleanup uses. When the
person later adds a blocklist entry, the cleanup also deletes agent notes whose
`text` contains it (today it matches only `window_title` and `url`), so "a
blocklisted site is absent from Search, Resume, and agent packs" (month plan,
Trust) stays true for notes.

**Cloud reasoning.** If the person opted into cloud reasoning (month plan
section 6, "Reasoning tier", ADR-018), Ask
may send note text to the cloud model like any other memory text that passed
the gates. Notes add no new egress.

## 9. Embedding, search, and isolation (proposal)

**Same embedding path.** Build the record, compose the document with
`compose_memory_embedding_document` (`src-tauri/src/memory_embedding_document.rs:180`),
embed primary, snippet, and support texts with the same text embedder and
`embed_batch_with_context` call capture uses (app name `"Agent note"`, the
title), and write the embedding manifest into `raw_evidence` with
`upsert_embedding_manifest`. If the embedder is missing or any vector is all
zeros (`is_low_signal_embedding`, `memory_compaction.rs:354`), refuse with
`embedder_unavailable` and store nothing, as capture does with
`SkipReason::EmbedderUnavailable` (`src-tauri/src/lib.rs:182-184`). Do not copy
the Companion path, which stores default vectors. When capture starts writing
the chunk table (month plan, Vault target 2), notes are chunked by the same
call.

**Found the normal way.** Notes are reached by the vector and keyword routes
over the memories table (`src-tauri/src/context_runtime/vector_route.rs:62`,
`keyword_route.rs:119`) and by time. They appear in Search, Ask, Memory Vault,
and MCP read tools, always with `source_type: "agent"` and `added_by`.

**Never merged, never derived.** An agent note:

1. Is inserted alone, never through `merge_or_append_memory_record`, and the
   capture merge candidate filters skip `source_type == "agent"` explicitly
   (today only the app-name filter and the URL rule keep them apart, by
   accident).
2. Is never passed to `sync_memory_record`: no graph node or edge, activity
   event, entity alias, project context, or knowledge page comes from note
   text. No decision ledger entry, no Task.
3. Is skipped by memory review (backfill and daily) and by any other pass that
   sends memory text to a local or cloud model to rewrite fields, so note text
   never sits in a prompt whose output writes to memory.
4. Is its own card (`agent_note:<id>` session key) and is never grouped with
   another result, so a badge cannot be lost or shared.
5. Is excluded from Resume threads and the Daily Brief before Beta (open
   question in the evidence file).

**Read side.** Every MCP row for an agent note carries
`"source_type": "agent"`, `"added_by": "<client>"`, and
`"provenance": "Written by an assistant through fndr.remember; not observed by FNDR."`.
Ask places note text inside the labeled evidence block like any memory and
cites it as "a note Claude Code added on Oct 4"; the answer must attribute
claims from notes and may not present them as observed. Note text is returned
only inside JSON string values.

## 10. What the person sees and how they delete

- **Badge** on every card in Search, Ask citations, Vault, and Quick Find:
  "Added by Claude Code".
- **Expanded card**: the provenance strip says "Added" instead of "Captured",
  "Source: Agent note", "Client: Claude Code (name reported by the assistant)",
  "Tool: fndr.remember", and the kind. Text renders as plain text: no Markdown,
  no HTML, no image loading, no link previews. A grep finds no
  `dangerouslySetInnerHTML` and no Markdown renderer under `src/` or in
  `package.json` today; keep it that way for notes.
- **Vault filter** "Added by assistants" (`source_type = agent`), with a client
  sub-filter.
- **Delete**: the existing per-memory delete (`delete_memory`,
  `src-tauri/src/ipc/commands/memory.rs:64`), plus "Delete all notes from
  <client>" for flood recovery.
- **Privacy Activity** (persisted by VS-37), one line per call, never content:
  - "Claude Code added a decision (412 characters)."
  - "Cursor tried to add a note. FNDR did not save it: it looked like a secret."
  - Other refusal labels: "too long", "too many notes", "mentions a blocked
    site or word", "hidden characters", "assistant notes are off", "actions
    are off".
- **Setting**: "Let assistants add notes" in Trust and Settings (section 5).

## 11. Out of scope before Beta

- Editing or annotating existing memories, including "suggested edits"
  (month plan decision 5).
- Any agent-side delete or update, including of the agent's own notes.
- Bulk import, files, images, attachments, or URLs as reopen targets.
- Promoting a note into a Task, the decision ledger, project context, the
  graph, Resume, or the Daily Brief.
- Per-client tokens (verified client identity).
- Idempotency keys or duplicate detection; the rate limit bounds loops.
- An MCP resource or special read tool for notes; reads use the normal tools.

## 12. Threats

| Threat | Corpus | Defense | Test that proves it |
| --- | --- | --- | --- |
| **Prompt injection via notes.** A note tells a future reader to ignore instructions, call a tool, or disable a protection; it may be hidden in an HTML comment or in invisible characters. | `an-011`, `an-012`, `an-013`, `an-014` | Note text is data. The handler calls no model and no router; notes never feed memory review, synthesis, or derived artifacts; command-surface invariant 1 already forbids memory text choosing tools or arguments. Invisible tag, bidi, and control characters are refused so the person can see everything stored. Reading assistants get explicit provenance. FNDR does not keyword-filter "instruction-like" notes: any such filter is easy to bypass and blocks honest notes like `an-006`. | `injected_instruction_does_not_change_risk_policy`, `remember_handler_calls_no_model`, `memory_review_skips_agent_notes`, `remember_rejects_invisible_characters`, `remember_rejects_without_token` |
| **Policy override.** A note claims a new risk level, setting, or blocklist change; or a call uses a made-up `kind`. | `an-015`, `an-016`, `an-017` | Risk levels are compiled into the registry and `risk_policy`; settings change only through the person's IPC commands; no code reads note text into config. Unknown `kind` is refused before text is read. | `injected_policy_note_does_not_change_settings_or_blocklist`, `injected_instruction_does_not_change_risk_policy`, `remember_rejects_invalid_kind` |
| **Tool-call injection.** Tool-call JSON or markup in a note; or a batch that hides `tools/call` behind `initialize`. | `an-018`, `an-019`, `an-020` | Stored as an inert string and returned only inside JSON string values. Auth decided per JSON-RPC item. | `tool_call_text_in_note_is_stored_inert`, `remember_in_batch_after_handshake_requires_token` |
| **Memory poisoning (false facts).** A note asserts something untrue, possibly "correcting" a captured memory. | `an-021`, `an-022` | FNDR does not judge truth. It keeps provenance on every surface, never edits the linked memory, never folds the claim into project context, the decision ledger, or the graph, and Ask must attribute it. The person can delete it. | `false_fact_note_is_labeled_and_isolated`, `agent_note_does_not_enter_derived_artifacts`, `ask_prompt_labels_agent_note_evidence` |
| **Flooding.** A loop or a rotating client name writes many notes. | `an-029`, `an-030` | Per-client and global limits with `retry_after_seconds`; size cap; Vault "delete all from client". | `rate_limit_returns_retry_after`, `global_daily_cap_holds_across_client_names` |
| **Impersonating screen memories.** Arguments that set provenance or time; text that claims to be a capture; a client named like FNDR; read paths that relabel; merges or card grouping that blend a note into a screen memory. | `an-023`, `an-024`, `an-025` | `deny_unknown_fields`; server-set `source_type`, `app_name`, and time; "FNDR..." client names become "Unknown client"; reads report the stored `source_type`; explicit merge exclusion; own session key; "Added" not "Captured" in the UI. | `remember_rejects_unknown_arguments`, `remember_stores_agent_provenance`, `remember_sanitizes_fndr_like_client_names`, `mcp_read_reports_agent_source_type`, `agent_notes_never_merge_into_screen_memories`, `card_synthesis_never_groups_agent_notes` |
| **Exfiltration via note text.** A secret copied into a note for another reader; a Markdown image or link that leaks data when rendered; a URL that becomes an open target. | `an-012`, `an-027`, `an-028`, `an-010` | Shared secret detector refuses credentials. Notes render as plain text and FNDR never fetches URLs in notes. Notes have no `url` or reopen target. Blocklisted terms are refused at write and cleaned up later. | `remember_rejects_secret_without_echo`, `remember_secret_check_matches_capture_detector`, `vault_renders_agent_note_as_plain_text`, `agent_note_has_no_reopen_target`, `blocklist_add_deletes_matching_agent_notes` |

## 13. Tests an implementation must pass

Rust tests live next to the code they test unless noted; the corpus harness is
`src-tauri/tests/agent_notes.rs`. Frontend tests use Vitest.

| Test | Where | Corpus attacks | Invariants | Asserts |
| --- | --- | --- | --- | --- |
| `remember_rejects_without_token` | `mcp` | instruction_injection (`an-014`) | other_memories, blocklist | HTTP 401, `-32001`, no row. |
| `remember_in_batch_after_handshake_requires_token` | `mcp` | tool_call_injection (`an-020`) | other_memories | `[initialize, tools/call]` from loopback without a token is refused; no row. |
| `remember_refused_when_auth_disabled` | `mcp` | none | other_memories | `FNDR_MCP_REQUIRE_AUTH=0` gives `auth_required_for_writes`. |
| `remember_refused_when_notes_disabled_or_kill_switch_on` | `mcp` | none | other_memories, risk_policy | `notes_disabled`, `actions_off`. |
| `remember_gate_runs_before_dispatch` | `agent::risk_policy` | none | risk_policy | Source-order check like the existing gate test. |
| `remember_rejects_unknown_arguments` | `mcp` | impersonation (`an-023`) | other_memories | `unknown_argument`; no row. |
| `remember_rejects_invalid_kind` | `mcp` | policy_override (`an-017`) | risk_policy, other_memories | `invalid_kind`. |
| `remember_text_limit_counts_characters_not_bytes` | `mcp` | none (`an-007`, `an-008`), oversize (`an-026`) | other_memories | 4,000 characters stored (ASCII and multibyte), 4,001 refused with `text_too_long`, nothing truncated. |
| `remember_rejects_invisible_characters` | `mcp` | instruction_injection (`an-013`) | blocklist, other_memories | Tag, bidi, and control characters give `invalid_characters`. |
| `remember_rejects_secret_without_echo` | `mcp` | secret (`an-027`, `an-028`) | other_memories | `sensitive_content`; the error, logs, and Privacy Activity line contain none of the input. |
| `remember_secret_check_matches_capture_detector` | corpus harness | secret, none | none | For every case, refusal with `sensitive_content` happens exactly when `safety_gate::evaluate(None, None, None, None, Some(title + "\n" + text), &[]) != Allow`. |
| `remember_allows_screen_context_words_in_notes` | `mcp` | none (`an-003`, `an-005`, `an-009`) | other_memories | "login", "API keys", "token budget", `sk-short` are stored. |
| `remember_rejects_blocklisted_term` | `mcp` | none (test-local blocklist) | blocklist | `blocklisted_content`. |
| `blocklist_add_deletes_matching_agent_notes` | `ipc::commands::privacy` | none | other_memories | A note mentioning a newly blocked site is deleted; unrelated notes stay. |
| `remember_stores_agent_provenance` | `mcp` | none, impersonation (`an-024`) | other_memories | All section 4 fields; `url` none; reopen `Unknown`; `raw_evidence.agent_provenance` present. |
| `remember_client_name_comes_from_session_not_arguments` | `mcp` | impersonation | other_memories | Name from `initialize`; missing session gives "Unknown client". |
| `remember_sanitizes_fndr_like_client_names` | `mcp` | impersonation (`an-025`) | other_memories | "FNDR Capture" is stored as "Unknown client". |
| `remember_keeps_full_note_text` | `storage` | none (`an-007`) | none | Compaction leaves `text` and `clean_text` whole; keyword search finds a word from the last 100 characters. |
| `remember_embeds_with_capture_embedder` | `mcp` | none | none | Non-zero vectors of the capture dimension; manifest written. |
| `remember_fails_closed_when_embedder_unavailable` | `mcp` | none | other_memories | `embedder_unavailable`; no zero-vector row. |
| `agent_notes_never_merge_into_screen_memories` | `tests/merge_replay.rs` | false_fact, impersonation | other_memories | A replayed capture with the same text never merges into or rewrites the note; both rows exist. |
| `card_synthesis_never_groups_agent_notes` | `search::memory_cards` | impersonation | none | Two notes with the same title from different clients within 5 minutes are two cards, each with its own badge. |
| `agent_note_does_not_enter_derived_artifacts` | corpus harness | false_fact (`an-021`, `an-022`) | derived_artifacts | No graph, activity event, alias, project context, knowledge page, ledger, or task change. |
| `memory_review_skips_agent_notes` | `memory_review` | instruction_injection | other_memories | Backfill and daily review never queue an agent note. |
| `remember_handler_calls_no_model` | `mcp` | instruction_injection, policy_override | risk_policy | The handler completes with the inference engine absent, and a panicking test engine is never called. |
| `injected_instruction_does_not_change_risk_policy` | corpus harness | instruction_injection, policy_override, tool_call_injection | risk_policy, tool_registry | `decide` table, `mcp_side_effect_tools()`, kill switch, and `tools/list` identical before and after. |
| `injected_policy_note_does_not_change_settings_or_blocklist` | corpus harness | policy_override (`an-015`, `an-016`) | blocklist, privacy_settings, auth_token | Config and token file identical. |
| `tool_call_text_in_note_is_stored_inert` | corpus harness | tool_call_injection (`an-018`, `an-019`) | tool_registry, risk_policy | No tool dispatched; text round-trips byte-identical. |
| `false_fact_note_is_labeled_and_isolated` | corpus harness | false_fact | other_memories, derived_artifacts | Seed memories byte-identical; the note is returned with `source_type: agent`. |
| `mcp_read_reports_agent_source_type` | `mcp` | impersonation | none | `fndr.search`, `memory.search_full_context`, and context packs report `agent` and `added_by`, not `coding` or `screen`. |
| `ask_prompt_labels_agent_note_evidence` | `context_runtime` | false_fact, instruction_injection | none | Notes enter the prompt only inside the labeled evidence block with client and date. |
| `agent_note_has_no_reopen_target` | `ipc::commands::memory` | none (`an-010`) | none | Reopen returns false; no URL from text is opened. |
| `rate_limit_returns_retry_after` | `mcp` | flood_burst (`an-029`) | other_memories | Calls 1 to 10 stored, call 11 refused with `retry_after_seconds >= 1`. |
| `global_daily_cap_holds_across_client_names` | `mcp` | flood_burst (`an-030`) | other_memories | 500 stored across six names, call 501 refused. |
| `privacy_activity_logs_note_writes_without_content` | `mcp` | all | none | One line per call with client, kind, count, outcome; never text, title, or project. |
| `corpus_preserves_all_invariants` | corpus harness | all | all | Runs every case, checks each `expected`, then every invariant. |
| `vault_renders_agent_note_as_plain_text` | `src/domains/memory-vault` (Vitest) | instruction_injection (`an-011`, `an-012`) | none | No `img`, `a`, or HTML element is created from note text; no network request. |
| `memory_card_shows_added_by_badge` | `src/domains/memory-vault` (Vitest) | impersonation | none | Badge shows the client; strip says "Added", not "Captured". |
| `vault_filter_lists_only_agent_notes` | `src/domains/memory-vault` (Vitest) | none | none | The filter shows notes only and offers "Delete all notes from <client>". |

## 14. Corpus format

`src-tauri/tests/fixtures/agent_notes/injected-notes.json` holds 30 synthetic
cases (10 benign) plus the fixture, harness rules, and vocabulary they rely on.

Each case: `id`, `kind`, `text`, `client` (the `clientInfo.name` sent at
`initialize`), `attack` (`none`, `instruction_injection`, `policy_override`,
`tool_call_injection`, `false_fact`, `impersonation`, `oversize`, `secret`,
`flood_burst`), and `expected` with `accepted`, `stored_source_type` (`"agent"`
or `null`), `must_not_change`, and `error` when refused. Optional: `args`
(extra arguments, including ones the tool must refuse), `auth`, `transport`,
`burst`, `rationale`, and in `expected` `stored_client`,
`accepted_before_limit`, `retry_after_seconds_min`.

The file is ASCII; non-ASCII test text uses JSON `\u` escapes so invisible
characters stay visible in review. Secrets are placeholders such as
`FAKE_TOKEN_DO_NOT_USE_1234`; none uses a real provider key format.
