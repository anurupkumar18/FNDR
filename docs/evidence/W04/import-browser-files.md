# Browser history and recent files import evidence

Workstream B, 2026-10-15. This note covers connector reads only. Workstream A owns admission, storage, consent, and deletion.

## Browser history

- Chromium profiles are discovered under Chrome, Brave, Edge, Arc, Dia, and Vivaldi application support folders. Safari uses `~/Library/Safari/History.db`.
- The connector copies each SQLite database and its WAL and SHM sidecars into a temporary directory before opening the copy read-only. It never queries the original connection.
- Chromium visit times convert from microseconds since 1601. Safari visit times convert from seconds since 2001. Visits within the requested window are grouped by browser, profile, and cleaned URL. Each item contains the latest visit time and a visit count. Scan pages run oldest first so a later visit remains after the saved cursor.
- Only HTTP and HTTPS URLs are admitted by the connector. File URLs, browser internal schemes, extension URLs, fragments, and tracking query parameters are skipped or removed. Item text contains the title, domain, visit count, and search terms where Chromium stores them. No page body is available in history, and the connector makes no page request.
- A denied Safari database read maps to a typed FullDiskAccess permission error. The product should explain that the person can grant FNDR Full Disk Access in System Settings > Privacy & Security > Full Disk Access, then retry.

## Recent files

- A person can choose scan roots. The default roots are Documents, Desktop, and Downloads. The scanner walks directories without Spotlight and does not follow symlinks. Scan pages run oldest first so a later file change remains after the saved cursor.
- Hidden names and dotfiles, `.env*`, private keys, keychains, `id_rsa*`, `node_modules`, `target`, `.git`, build output, and every path under Library are excluded before content is read. Files over 5 MB are skipped.
- Plain text, Markdown, source code, CSV, JSON, and RTF are read as text. DOCX text is extracted from `word/document.xml` with a 5 MB decompression cap. Each item text is capped at 20,000 Unicode characters. PDFs contain the title field and absolute path only. Cargo.toml had no direct pure Rust PDF extractor suitable for this slice, so no PDF body is read.
- Each item's reopen target is an absolute path. The connector does not write, embed, or index files.

## Verification

The tests build synthetic Chromium and Safari databases and synthetic files under temporary directories. They cover the two time epochs, Unicode titles and content, duplicate visits, tracking URL cleanup, copied reads while the source database is locked, corrupt database handling, DOCX and RTF extraction, PDF metadata only, text caps, oversized files, secret names, Library exclusion, a symlink loop, cursor resume after a new item, and live sources scanned after construction. `cargo test --lib import::` passed with 21 tests on 2026-10-15. No owner's browser history or files were read.

The temporary `contract_stub.rs` follows A's compiled exchange contract and must be removed when A's real contract is integrated. The connector imports then change from `super::contract_stub::*` to `crate::import::*`. Workstream A's `sources::registered()` must construct the live sources after per-source consent is enforced.
