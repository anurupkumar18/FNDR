# Trust controls, synthetic verification (2026-10-15)

This lane was checked with synthetic memory records and temporary directories. No owner vault, browser history, mail, capture, or database was opened.

| Requirement | Evidence | Limit |
| --- | --- | --- |
| Default exclusions | 12 synthetic tests cover banking hosts and titles, password-manager apps, System Settings, private-window title markers, and safe lookalikes. The capture safety gate applies these before storage. | A browser that gives a private window no recognizable title marker cannot be identified by this gate. |
| Recording indicator and pause shortcut | A synthetic state test proves the user pause preference survives relaunch and toggles back. A state test checks REC, PAUSED, PRIVATE and IDLE copy. The Tauri binary check includes startup registration of a separate menu bar item and Control+Alt+P. | The live menu bar item and shortcut were not exercised because launching capture against this Mac would violate the synthetic-only rule. |
| Delete by time range | Three store-backed synthetic tests cover preview without deletion, exclusive end time, v4 and v5 parent rows, child chunks, task references, legacy and insight graph links, shared graph nodes, old Lance versions, and delete-all graph cleanup. A separate test covers plaintext and sealed screenshot artifact removal. | Imported rows need A's tombstone call before range deletion at integration. |
| JSONL and Markdown archive | Five tests cover a sealed file round trip, provenance and text preservation, visibility filtering, built-in exclusion filtering, idempotent import, invalid format and duplicate rejection, and captured HTML escaping in Markdown. The archive uses a fixed format header and preserves full memory fields except screenshot paths. | The Keychain key stays on this Mac, so another Mac cannot reopen the files without a key-transfer design. Graph tables and child chunks are not in the archive. A's imported-source rows need its admission path on restore at integration. |
| File sealing and FileVault | The real macOS Keychain provider passed an ignored test under a disposable service and account; the test deleted its key. An ignored host probe ran `/usr/bin/fdesetup status` and reported FileVault On. Unit tests cover wrong-key rejection, different ciphertext for identical bytes, file replacement, and a recursive image sweep. | FNDR's memory database is not encrypted by FNDR. FileVault status reports disk protection only. Normal capture usually writes no durable screenshot. The background sweep attempts to seal older images in the app's capture folders; other exports such as PDFs can be plain. |
| Privacy Proof and first run | 39 focused frontend tests passed across Privacy Proof, onboarding, IPC drift and memory-read drift. Typecheck and production build passed. First run shows FileVault status and the encryption boundary; Privacy activity also lists backend-owned default exclusions. | A live native UI run remains for integration with a seeded profile. |

## Verification commands

- `cargo test --lib --offline privacy::`: 53 passed, 0 failed, 2 ignored.
- `cargo test --lib --offline privacy::export::tests`: 5 passed after a regression first failed with 3 restored rows instead of 1.
- `cargo test --lib --offline ipc::commands::privacy::erasure_tests`: 3 passed.
- `cargo test --lib --offline ipc::commands::stats::capture_pause_persistence_tests::shortcut_toggle_persists_pause`: passed.
- `cargo check --bin fndr --offline --message-format short`: passed after rebuilding the local build script. One prior attempt used another lane's shared build-script artifact and failed on its absent Foundation helper.
- `npx vitest run src/domains/privacy-proof/PrivacyProof.test.tsx src/domains/workspace/Onboarding.test.tsx src/shared/ipc/ipcDrift.test.ts src/dev/memoryReadDrift.test.ts`: 39 passed.
- `npm run typecheck`, `npm run build`, `git diff --check`, and rustfmt checks on changed Rust files: passed.

The Rust tests used synthetic data and a static encryption key except for the isolated real Keychain test. The FileVault probe queried the real host setting. The frontend tests used IPC fakes. No live FNDR capture or native picker run was made.
