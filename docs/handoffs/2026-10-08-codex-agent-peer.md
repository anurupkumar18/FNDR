# Handoff: Codex agent peer lane

## Goal and five-hour sequence

Keep this lane on FNDR's agent interoperability while the two Claude sessions finish briefing/task quality and agent surfaces in the shared checkout. Use one measurable slice at a time.

| Time box | Output | Gate |
| --- | --- | --- |
| 0:00–1:00 | Audit current MCP/Hermes boundary and decide A2A/MCP roles | ADR 025 and current-code evidence |
| 1:00–2:00 | Give embedded Hermes a server-enforced read grant | Real localhost MCP positive/negative tests |
| 2:00–3:00 | Define a configured peer and validate an A2A 1.0 Agent Card | Fixture and negative tests for card schema, URL, version, binding and required extension |
| 3:00–4:00 | Build a delegation draft with an exact source attachment preview | Current durable-record authorization; excluded/missing source negatives |
| 4:00–5:00 | Review quality, run focused suites, commit only owned hunks, write next checkpoint | Remote tip and clean ownership check |

The first two boxes are complete. The third has a validated backend boundary but still needs a saved peer choice and Agent UI entry point. The remaining boxes are ordered gates, not a claim that transport or delegation is implemented.

## Current state

- `main` includes Codex commit `d1a575f` for a bounded A2A 1.0 Agent Card validator/fetch. Earlier Codex commits are `aeb8a05` (grant and ADR), `2fbb581` (documentation correction), and `4dbcc95` (raw evidence restriction).
- `agent/peer.rs` accepts a same-origin HTTPS JSON-RPC 1.0 Card with anonymous or Bearer authentication and no required extension. It checks DNS answers before egress, pins one vetted address, disables proxies and redirects, and bounds time and response bytes. It is an internal engine seam only: no saved peer, credential binding, task send or UI exists yet.
- ADR 025 decides outbound A2A client first, explicit reviewable delegation, current source checks before egress, durable peer-task and egress records, and no automatic trust in remote artifacts. A2A transport is unbuilt.
- The shared checkout remains dirty with the Claude sessions' Rust changes. In particular, unstaged formatting/test movement in `src-tauri/src/mcp/mod.rs` belongs to the other session. Do not stash, reset, or stage that entire file.

## Verification

| Command | Result |
| --- | --- |
| `CARGO_BUILD_JOBS=1 cargo test --lib mcp::tests::` | 39 passed |
| `CARGO_BUILD_JOBS=1 cargo test --lib mcp::remember_http_tests::` | 15 passed |
| `CARGO_BUILD_JOBS=1 cargo test --lib ipc::commands::hermes_codex::tests::mcp_block_quotes_the_endpoint_and_token` | 1 passed |
| `CARGO_BUILD_JOBS=1 cargo test --lib mcp::tests::hermes_token_is_limited_by_the_server_to_its_four_read_tools` | Red before raw guard, then 1 passed after guard |
| `git diff --cached --check` | Passed before each Codex commit |
| `CARGO_BUILD_JOBS=1 cargo test --lib agent::peer::tests::` | Red for the parser and fetch first; then 7 passed. The final IANA IPv6 special-use correction passed its focused test. |

The MCP test starts a real localhost server with a disposable profile. It checks four-tool discovery, method and action refusals, current sharing consent, restart behavior, raw-evidence refusal, and the full-token path. This is not a full Hermes gateway or GUI test. No real owner vault contents were used.

## Next actions

1. Add a saved peer choice and Agent UI entry point that accepts only a person-entered Card URL. Reinspect and revalidate the Card before any later task request; a saved endpoint or DNS answer is never enough. Bind Bearer credentials out of band, without putting them in the Card URL or message.
2. Build the reviewable delegation draft against `context_runtime::context_source_memories`, using current durable records at send time. Hidden, missing, deleted and stale sources must block; aliases must resolve to the current canonical record. The preview must match the exact bytes eventually sent.
3. Keep new peer code out of Claude's active briefing/task/agent-surface files where possible. If shared command registration is needed, stage only owned hunks and inspect the index before committing.
4. Do not claim peer tasks, cancellation or privacy accounting until the later ADR 025 gates pass. Keep external messages/artifacts as untrusted evidence.

## Sources

- [A2A 1.0 specification](https://a2a-protocol.org/v1.0.0/specification/) for Agent Card, Task, Message and Artifact contracts.
- [A2A and MCP comparison](https://a2a-protocol.org/v1.0.0/topics/a2a-and-mcp/) for the peer-task versus tool/data split.
