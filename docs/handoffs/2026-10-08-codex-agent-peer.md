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

The first two boxes are complete. The remaining boxes are ordered gates, not a claim that transport or delegation is implemented. If Card validation exposes a networking or identity dependency, finish its tests and decision before starting the draft.

## Current state

- `main` at `4dbcc95` was pushed to GitLab and GitHub and both remote `main` tips were verified at that SHA. The Codex commits are `aeb8a05` (grant and ADR), `2fbb581` (documentation correction), and `4dbcc95` (raw evidence restriction).
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

The MCP test starts a real localhost server with a disposable profile. It checks four-tool discovery, method and action refusals, current sharing consent, restart behavior, raw-evidence refusal, and the full-token path. This is not a full Hermes gateway or GUI test. No real owner vault contents were used.

## Next actions

1. Read ADR 025 and the A2A 1.0 Agent Card schema before implementing a parser. Choose the supported binding and required extension policy explicitly. A Card is untrusted input and cannot name a new destination outside the configured origin.
2. Add a failing boundary test before each validator or fetch behavior. Reject non-HTTPS production URLs, credentials in URL, redirects, private/link-local destination addresses and mismatched interface origins. Bound response size and time. A loopback fixture is test-only.
3. Keep new peer code out of Claude's active briefing/task/agent-surface files. If shared command registration is needed, stage only owned hunks and inspect the index before committing.
4. Do not claim peer tasks, cancellation or privacy accounting until the later ADR 025 gates pass. Keep external messages/artifacts as untrusted evidence.

## Sources

- [A2A 1.0 specification](https://a2a-protocol.org/v1.0.0/specification/) for Agent Card, Task, Message and Artifact contracts.
- [A2A and MCP comparison](https://a2a-protocol.org/v1.0.0/topics/a2a-and-mcp/) for the peer-task versus tool/data split.
