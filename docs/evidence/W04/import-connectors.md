# Import connector integration

The connectors return bounded `ImportItem`s. A's `ImportRunner` applies one admission gate, embeds admitted items, and stores them as searchable leaf rows. Registration constructs readers only; scans start after a separate import request. The runner receives the requested window, defaulting to 30 days and clamped by the configured 90 day retention.

| Source | Reads | Connector skip or cap |
| --- | --- | --- |
| Browser | Local Chromium-family and Safari history copies | Search, sign-in and internal pages; bounded history window and batches |
| Recent files | Selected local roots | Secret-bearing paths, build folders, symlinks and oversized files; bounded text extraction |
| Calendar | EventKit calendars selected by ID | Declined, cancelled, subscribed by default; event notes capped at 20,000 characters |
| Mail | Apple Mail `.emlx` or chosen `.mbox` and `.eml` files | Trash, Junk, Spam, Drafts, bulk promotions by default, quoted replies, attachments and symlinks; body capped at 20,000 characters |
| Slack | User-selected export folder or ZIP | Unsafe paths, huge JSON files, attachment bodies and dates outside the window; channel-day text capped at 20,000 characters |
| Git | Selected local repository roots, optionally `gh` CLI | Symlinks, build folders, commits by other authors, prompt-driven CLI access; item text capped at 20,000 characters |

The runner additionally skips banking and sign-in URLs, password managers, private windows and credential paths, then redacts secret-looking body lines. Synthetic store-backed cases cover admission, search, rerun idempotence and Forget for all six source kinds. Connector unit tests cover empty and corrupt inputs, large Unicode content and permission errors. Git tests use real `git` on temporary repositories; `gh` is fake-tested and stays disabled unless explicitly selected. No owner's browser, mail, calendar, Slack export, repository, vault or token was read.

## Verification

`cargo check --locked --offline --lib` and `cargo check --locked --offline --bin fndr` passed. `cargo test --locked --offline --lib import:: --no-default-features -- --test-threads=1` passed 71 tests, including all six source unit groups and six synthetic store-backed connector cases. A second pass of 11 focused edge-case tests passed for empty input, corrupt data, large files, Unicode, permissions and cursor behavior. The compiled test binary's `calendar` filter passed 15 tests with one real-calendar test ignored. `npm run typecheck`, `npm run build`, `rustfmt --edition 2021 --check` on changed Rust files, and `git diff --check` passed. The first registry run failed as intended before B was copied: 52 passed, 1 failed. A Git store test failed on a changed branch timestamp, then passed after stable timestamps replaced the scan clock. The shared target twice reused another worktree's build script and looked for an absent Foundation helper; a temporary copy of that helper source allowed Cargo to verify this worktree and was removed before handoff.

## Not run

No real data import. Owner decision 12 reserves that run for the owner on a copy of their data. The full `make test` sweep was not run; focused connector and import tests cover this slice.

`registered_with(SourceOptions)` accepts roots, mailbox choices, an export path and a separate `gh` toggle. The default registry constructs Browser, Recent Files, Calendar, Apple Mail and Git readers; Slack and dropped mail files require configured paths. The runner receives window days when a scan starts. Persisted choices still need to feed `registered_with` before the global runner initializes. E owns backend consent and its UI; this integration does not change them.
