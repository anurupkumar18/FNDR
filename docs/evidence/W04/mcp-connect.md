# MCP connect evidence

Date: 2026-10-15. Workstream G.

## Scope

Setup can connect Claude Code, Claude Desktop and Cursor to FNDR's local MCP server. The four new recall tools are `fndr.recent_activity`, `fndr.search_memories`, `fndr.standup` and `fndr.list_sources`. Their rows carry source, app, time, memory id and reopen provenance. The server checks current visibility, blocklist and privacy before returning them.

## Checks completed

| Check | Boundary | Result |
|---|---|---|
| `npm run typecheck` | TypeScript | Passed after Setup and IPC edits. |
| `npm run build` | Frontend production bundle | Passed. Vite reported its existing large-chunk warning. |
| `npx vitest run src/domains/setup src/shared/ipc src/dev` | Setup, IPC drift and memory read drift | Passed, 47 tests after the strict privacy setting. |
| `cargo test --lib mcp::` | Rust unit tests | Passed, 133 tests and 1 ignored real-client test. |
| `cargo test --lib mcp::connect_http_tests::setup_connection_test_completes_a_real_http_handshake_and_refuses_missing_auth` | Real FNDR router over ephemeral HTTP with a synthetic store | Passed. Handshake, tool listing, authenticated call and missing-token refusal. |
| `cargo test --lib mcp::connect_http_tests::recall_over_http_reports_origin_and_hides_blocked_memory` | Real FNDR router over ephemeral HTTP with synthetic rows | Passed. Screen and import provenance, reopen target, blocklist, all four recall tools. |
| `cargo test --lib mcp::connect_http_tests::hide_more_setting_changes_what_the_http_tools_return` | Real FNDR router with the saved privacy setting | Passed. Ordinary development text is visible by default, true credentials are withheld, and the strict opt-in withholds both. |
| `cargo test --lib mcp::connect_http_tests::real_mcp_remote_client_calls_a_recall_tool -- --ignored --nocapture` | Official MCP client 2.3.1 over the real `mcp-remote` stdio bridge | Passed. `fndr.recent_activity` returned synthetic memory `bridge-1` and FNDR server identity. |
| Claude Code 2.1.301 `mcp add`, `mcp get`, `mcp remove` | Real CLI in a temporary home | Passed config registration and removal. `mcp get` reported connection refused because the synthetic port had no server. No owner config or token was read. |
| `rustfmt --edition 2021 --check` on lane Rust files and `git diff --check` | Source format | Passed. Whole-file rustfmt on `config.rs` still reports an unrelated pre-existing `neural_voice` block at line 891. |
| `git apply --check docs/evidence/W04/mcp-mod-registration.patch` | Blocked-file handoff | Passed after restoring `mcp/mod.rs` to its original content. |

The HTTP tests create temporary directories and an ephemeral loopback listener, abort the server task and clean up the store. They do not read a real user profile. The real client test installs the official MCP client into a temporary home and gives the bridge a temporary npm cache. The Claude Code CLI check used a synthetic token and an isolated temporary home, which was removed afterward. No owner config, token or vault was read.

The first raw stdio harness failed: the Rust test selected a broken Homebrew Node binary, then its hand-written pipe exchange received no response. The official client transport completed the handshake and tool call through the same bridge. This establishes the real client boundary without making the harness part of the product.

## Registration handoff

`src-tauri/src/mcp/mod.rs` is blocked by the shared-checkout plan. Apply `docs/evidence/W04/mcp-mod-registration.patch` when merging this lane. It registers the three new MCP modules, their HTTP tests and the four recall tools. The worktree leaves the blocked file at its original content after verification, so the patch is required before compiling the integrated code.

## Config compatibility

FNDR writes only the `fndr` entry under `mcpServers`. The JSON writer preserves values but re-sorts object keys because `serde_json` does not preserve insertion order in this build. It backs up the file before changing it and refuses malformed JSON or a foreign `fndr` entry. The owner chose this behavior in `FNDR-exchange/DECISIONS.md`, item 4.
