# Calendar and Mail import evidence

Date: 2026-10-15

## Scope

Calendar imports EventKit events from the past `window_days` through seven days ahead. It requests Calendar access only when `scan` starts, reads the selected calendars or all nonsubscribed calendars by default, skips declined and cancelled events, and returns event fields as capped plain text. Its stable key includes the EventKit identifier and occurrence start. EventKit does not expose a reliable private-calendar designation, so an unselected calendar is skipped only through explicit calendar selection. EventKit does not provide a supported `calendar://` event reopen URL, so the target is `None`.

Mail imports local Apple Mail `.emlx` files under `V*/Mailboxes`, dropped `.mbox` files, and dropped `.eml` files. Mailbox discovery counts filenames and does not open messages. By default, Apple Mail scanning includes Inbox, Sent, and Sent Messages within `window_days`; another mailbox must be selected explicitly. Messages with `List-Unsubscribe` and a promotions or marketing category are skipped unless bulk mail is enabled. The parser decodes MIME transfer encodings and charsets, prefers plain text over HTML, strips common reply and signature markers, and records attachment names only. Files over 16 MiB are skipped to bound memory use, and each item text is capped at 20,000 Unicode characters. Apple Mail permission errors return a typed Full Disk Access message.

## Verification

- Synthetic unit fixtures: `.eml`, `.emlx`, `.mbox`, quoted-printable, base64, ISO-8859-1, HTML-only, attachment, reply/signature, promotional mail, selected mailbox, malformed input, empty mailbox, large body, Unicode, and permission mapping.
- Focused command: `cargo test --locked --lib import::sources -- --nocapture` with the specified SDKROOT, CXXFLAGS, CARGO_BUILD_JOBS=2 and shared CARGO_TARGET_DIR. Result: 9 passed, 0 failed.
- Existing calendar regression command: `cargo test --locked --lib calendar -- --nocapture`. Result: 14 passed, 0 failed, 1 real-calendar test ignored. The newly compiled test binary ran `import::sources::mail::tests` again: 6 passed, 0 failed.
- `rustfmt --edition 2021 --check` on changed Rust files and `git diff --check`: passed after the final test pass.

The temporary import contract stub must be discarded when workstream A lands.

## TDD summary and limits

- The first focused run failed while the new fixture referenced `map_mail_io` before its implementation. After implementation, the connector suite passed, then the edge-case pass and existing calendar tests passed.
- `mailparse 0.16.1` is the only new direct crate. No MIME parser was already declared in `Cargo.toml`; it handles multipart structure, quoted-printable, base64, and charset decoding.
- This is connector-only code. The source runner must honor the owner's per-source consent decision, pass the default 30 day scan window, enforce 90 day retention, and expose Forget. Mail stays off in the recommended one-tap set.
- No real Calendar or Mail store was read. EventKit compiled and linked on macOS, but real permission, source data, and `message://` reopening were not exercised. HTML reply stripping covers common blockquotes and line markers, not every mail client layout.
