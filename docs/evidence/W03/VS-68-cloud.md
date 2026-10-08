# VS-68 a safe slice of agent write-back: `fndr.remember` (cloud)

Spec: `docs/product/fndr-remember-spec.md` (VS-35). Corpus: `src-tauri/tests/fixtures/agent_notes/injected-notes.json`. This branch builds on train G (VS-61).

## What the slice does

1. **Token auth.** A note needs the bearer token.
   - With `FNDR_MCP_REQUIRE_AUTH=0`, every call answers `auth_required_for_writes`, with or without a token.
   - A `[initialize, tools/call]` batch without a token gets HTTP 401 (VS-41's per-item rule).
2. **Write gate.**
   - It runs in `call_tool` before dispatch, through `risk_policy::decide_mcp_write`: kill switch (`actions_off`), then the new `agent_notes_enabled` setting (`notes_disabled`, default off).
   - `fndr.remember` is in a separate write list. `decide` would confirm every MCP call, and no approval card exists.
3. **Rate limits**, sliding windows that count only allowed calls:
   - 10 per minute and 200 per day per client;
   - 30 per minute and 500 per day for all clients together.

   A refusal carries `retry_after_seconds` and names the binding limit.
4. **Arguments and text**, checked before anything is read from the text:
   - unknown fields: `unknown_argument`, so no caller can set `source_type`, `app_name`, `url`, or a time;
   - `invalid_kind`;
   - `invalid_characters`: C0 and C1 controls, DEL, bidi overrides and isolates, tag characters;
   - `empty_text`;
   - `text_too_long`: 4,000 characters, counted as characters, never cut;
   - title and project limits;
   - related ids: at most 10, and each must exist.
5. **Secrets and blocklist.** The capture detector (`safety_gate::evaluate` on title plus text) gives `sensitive_content`. A blocklist substring gives `blocklisted_content`. No message repeats the note.
6. **Embedding.**
   - The shared capture embedder must be the real model; otherwise the answer is `embedder_unavailable` and nothing is stored.
   - The document is composed the way capture composes it: primary, snippet, and support texts.
7. **A leaf row.** One memory with:
   - `source_type = "agent"`, app "Agent note", and title `title` or "Decision from Claude Code";
   - the client in `related_agents`, and `raw_evidence.agent_provenance` (client, name source, tool, kind, time);
   - session key `agent_note:<id>`, so it is its own card;
   - no URL and no reopen target, and full text, since compaction exempts agent notes;
   - `enrichment_status = "agent_note"`.

   Memory review (worker, backfill, and daily) skips it. The handler never calls `sync_memory_record`, so no graph, task, ledger, or project context comes from note text.
8. **Client name.** It comes from `clientInfo.name` at `initialize`, kept in a 64-entry session map and returned as `Mcp-Session-Id`. "FNDR..." names and missing names become "Unknown client". Arguments cannot set it.

The defaults follow VS-35's recommended answers to its open questions:
- notes are off by default;
- there is no per-note confirmation;
- notes are not in Resume (nothing feeds them there);
- the numbers are as written.

## Tests

The storage, memory review, risk-policy, and mock-embedder tests were written first and failed before their changes. The handler and HTTP tests were written alongside the handler.

| Test | Where | Covers |
|---|---|---|
| `remember_rejects_without_token` | `mcp::remember_http_tests` | HTTP 401 and -32001, no row (no token, and a wrong token) |
| `remember_in_batch_after_handshake_requires_token` | same | `[initialize, tools/call]` from loopback without a token: 401, no row |
| `remember_refused_when_auth_disabled` | same | `auth_required_for_writes`, with and without a token |
| `remember_refused_when_notes_disabled_or_kill_switch_on` | same | `notes_disabled` and `actions_off`, answered before an unknown argument is read |
| `remember_fails_closed_when_embedder_unavailable` | same | `embedder_unavailable`, no zero-vector row |
| `remember_stores_agent_provenance_and_the_note_is_findable` | same | every field above, embedding manifest present, no model loaded, found by `search_ranked_results` |
| `remember_client_name_comes_from_session_not_arguments` | same | no session or no name gives "Unknown client"; a `client` argument is `unknown_argument` |
| `corpus_preserves_all_invariants` | same | all 30 corpus cases over HTTP (below) |
| `rate_limit_returns_retry_after`, `global_daily_cap_holds_across_client_names` | `mcp::remember` | windows, `retry_after_seconds`, refused calls not counted, rotating names |
| `remember_text_limit_counts_characters_not_bytes`, `remember_rejects_invisible_characters`, `arguments_refuse_in_spec_order_and_never_echo`, `client_names_are_cleaned_and_fndr_names_refused` | `mcp::remember` | argument and text rules |
| `mcp_writes_refuse_on_the_kill_switch_then_the_notes_setting`, `remember_gate_runs_before_dispatch` | `agent::risk_policy` | gate order, and the write gate placed before dispatch in `call_tool` |
| `remember_keeps_full_note_text` | `storage` | 4,000 characters kept whole; a word in the last 40 characters found by keyword search; own session key; no reopen target |
| `memory_review_skips_agent_notes`, `daily_review_never_sends_an_agent_note_to_the_model` | `memory_review` | skip reason, backfill, and daily review |
| `mock_embeds_multibyte_words_without_panicking` | `embedding::onnx` | the corpus bug below |

**The corpus**, over real HTTP:
- 30 cases: 20 stored and 10 refused, each with the expected code (`invalid_characters`, `unauthorized` twice, `invalid_kind`, `unknown_argument`, `text_too_long`, `sensitive_content` twice, `rate_limited` twice).
- an-029 stored 10 calls and refused the 11th. an-030 stored 500 across six client names and refused the 501st. Both refusals carried `retry_after_seconds` of at least 1.
- Every stored note round-trips byte-identical, as `agent`, with no URL.
- `sensitive_content` happened exactly when the capture detector flags the text.
- After the run:
  - the seed memories are byte-identical;
  - the row count equals seeds plus stored notes;
  - config, `tools/list`, the risk-policy table, and the derived artifacts are unchanged: graph nodes and edges, activity events, ledger, packs, pages, tasks.

**Lib:** 945 passed, 0 failed. The corpus test takes about 65 s in a debug build, almost all of it an-030's 501 real inserts.

**Retrieval gate** (`make qa-retrieval-check`, `TZ=America/Denver`): knowledge-worker and office-pm PASS, no per-query rank changed. Nothing in the persona corpora is an agent note, so the gate checks that the normalization changes leave every other row alone.

**A bug the corpus found.** The mock embedder sliced token prefixes by bytes. A chunk of an-008's French text starts mid-word ("otée"), and the slice panicked inside "é". Only the mock fallback (`FNDR_ALLOW_MOCK_EMBEDDER=1`) and tests use that code. It now slices by characters, and ASCII vectors are unchanged (pinned by the test).

## Run log: a real server and the real MiniLM model

This was a scratch harness, not committed: `mcp::start` over a temporary store, the token file in a temporary home, and the shared embedder pointed at all-MiniLM-L6-v2. `fndr.remember` only accepts the real backend, so every stored note below was embedded by MiniLM. The client was curl, with the token masked.

```
$ initialize (clientInfo.name = "Claude Code"), no token
HTTP header Mcp-Session-Id: <36-character session id>

$ tools/list, no token: the fndr.remember listing
fndr.remember required: ['kind', 'text'] kinds: ['note', 'decision', 'summary', 'todo']

$ tools/call fndr.remember without a token
{"error":{"code":-32001,"message":"Unauthorized: valid Bearer token required"},"id":3,"jsonrpc":"2.0"} [HTTP 401]

$ tools/call fndr.remember with the token: a decision
{"added_by": "Claude Code", "char_count": 117, "kind": "decision", "memory_id": "c283a23b-...",
 "remaining": {"this_minute": 9, "today": 199}, "source_type": "agent", "status": "stored"} [HTTP 200]

$ tools/call fndr.remember with a credential-shaped string
structuredContent: {"error":"sensitive_content","message":"This note looks like it contains a password, key, or token, so FNDR did not save it. Remove the secret and try again."}

$ tools/call fndr.remember claiming to be a screen capture (source_type, app_name arguments)
structuredContent: {"error":"unknown_argument","message":"fndr.remember accepts only kind, text, title, related_memory_ids, and project."}

$ tools/call memory.search_full_context "default text embedder M1 run"
{'app_name': 'Agent note', 'window_title': 'Decision from Claude Code', 'snippet': 'Decided to keep MiniLM as the default text embedder until the EmbeddingGemma M1 run (VS-48) is in.'}

$ more notes in the same minute (three calls already counted: one stored, two refused)
calls 1 to 7: stored
call 8: rate_limited, per_client_per_minute, retry_after_seconds 59

$ restarted with no embedding model on disk
structuredContent: {"error":"embedder_unavailable","message":"FNDR's embedding model is not available, so the note was not saved."}
```

## Not in this slice

**For local (contract notes; these files are local-owned):**
- **Capture merges.** Capture's merge candidates should skip `source_type == "agent"` explicitly (`capture/mod.rs`, spec section 9 item 1). Today only the app-name filter keeps them apart: no captured app is named "Agent note". The spec's `agent_notes_never_merge_into_screen_memories` test needs that change.
- **The Claude Code demo.** Turn the setting on (`agent_notes_enabled = true` in `config.toml`), then call `fndr.remember` from Claude Code and find the note in Search. The run log above uses curl against the same server code.

**Cloud can take these next if wanted:**
- The Settings toggle "Let assistants add notes". Today the setting is `config.toml` only.
- The "Added by" badge and the "Added" provenance strip.
- The Vault filter, and "Delete all notes from client".
- `source_type` and `added_by` on read results (spec section 4 items 3 and 4, `mcp_read_reports_agent_source_type`). Reads show the app "Agent note" and the title.
- Privacy Activity lines (needs VS-37).
- The blocklist cleanup deleting notes by text (`blocklist_add_deletes_matching_agent_notes`).
- Gating `fndr_remember_decision` the same way (VS-35 open question 3).
- The Ask prompt label test (`ask_prompt_labels_agent_note_evidence`).

**Additions to the spec's codes:**
- `invalid_arguments`: not an object, a wrong field type, a duplicate related id, or arguments over 32 KiB.
- `project_too_long`.
- `storage_failed`: the store refused the write.
