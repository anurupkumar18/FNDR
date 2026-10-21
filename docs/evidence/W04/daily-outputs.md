# Daily outputs, synthetic evidence

Date: 2026-10-15. Workstream F.

## Scope checked

The synthetic Atlas and Birch fixtures in `src-tauri/src/outputs/` cover standup, weekly status and timesheet composition. Each document line must point to a memory id. Activity events retain their event id and name the underlying memory id in markdown, plain text and HTML source lists. A line without a memory id is removed before display.

The gather tests exclude imported rows and assistant notes, retain only events whose source memories are eligible, and use the shared visibility checks for blocklist, soft delete and quality. The model sees only a finished line. Tests reject an added name, number, file, instruction, narrator or changed action, and retain the original line and citations when rewording fails.

## Sends checked

The Slack and Linear tests use a local HTTP server and synthetic tokens. They assert the JSON body, method, path, authorization header, error handling and egress ledger entry. The Slack target check rejects a changed host or path after approval. Approval tests cover the kill switch, expiry, decline, replay and a changed tool. The mail tests check URL encoding, recipient capture and header injection. File tests use a temporary directory.

The destination allow list is tested with an in-memory secret store. It matches a digest of the exact target and supports removal. A one-use success token can add an entry only after a send completed. The panel tests show the exact body and destination before the first send, and offer the allow option only after a successful approved send.

## Verification

| Check | Result | Evidence type |
|---|---:|---|
| `cargo test --lib outputs:: --offline` | 62 passed | Unit tests and local HTTP fake |
| `cargo test --lib inference::prompts::tests --offline` | 4 passed | Unit tests |
| Focused Vitest for outputs, routing, IPC drift, memory reader drift and Privacy Proof | 26 passed | UI and registry tests |
| `npm run typecheck` and `npm run build` | Passed | Frontend compile and bundle |
| `cargo check --bin fndr --offline` | Passed | Rust library and Tauri handler compile |

No real vault, account token, Slack endpoint, Linear workspace, mail recipient or Keychain item was read or changed. The real service and Keychain check belongs to integration with the owner's go-ahead.
