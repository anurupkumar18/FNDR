# Import core and admission, 2026-10-15

Workstream A delivers the import contract, admission, runner, storage shape, retention, and IPC. Connectors are registered by later workstreams. This evidence uses synthetic items and a fake source only. Owner decision 12 defers any run on a real source to integration.

## Contract and storage

`src-tauri/src/import/mod.rs` defines six source kinds, typed permissions and errors, `ImportItem`, cursors, batches, and the read-only `ImportSource` trait. `ImportRunner` is the writer. It forces the kind, caps title at 300 characters and body at 20,000, checks retention by item timestamp, calls `admit`, deduplicates by stable `(kind, external_id)` row id and admitted content hash, embeds, then stores an ordinary memory row with `source_type = import:<kind>`. Imported rows preserve title, body, app label, item timestamp, source provenance, and only typed reopen fields. `ImportError::advice()` gives permission-specific next steps.

Admission applies the capture safety gate and current blocklist to app, URL, title, body and people. It skips banking, medical, sign-in, password-manager and private contexts, credential paths, and navigation pages. It redacts secret-looking body lines and hidden characters. A later blocklist change is applied at retrieval to imported body text. Imported text cannot supply a legacy `Reopen:` target; only explicit HTTPS, file path or deep-link provenance can.

A row with `source_type = import:*` is a leaf. Capture does not merge it with screen records. Compaction preserves its body; review, workset ranking and derived context do not promote it. Search cards keep it separate and do not synthesize a story from it. If an imported item enters answer context, it is labeled as untrusted evidence.

## Synthetic checks

| Check | Method | Result |
|---|---|---|
| Fake source to store to keyword search | Store-backed Rust test with mock embedder | Search found the admitted `import:mail` row; retrieval visibility passed |
| Sensitive items | Synthetic banking URL, 1Password app, secret body | Banking and password-manager items skipped; secret line became `[redacted]` |
| Rerun and update | Same and changed fake items | Same content stored zero new rows; changed content updated one stable row |
| Deletion | Fake row plus synthetic chunk and graph link | Forget removed rows, chunks, graph node and edge, cursor and tombstones |
| Erasure | Tombstone before simulated range delete | Rerun skipped erased id; Forget cleared tombstone |
| Cancel | Gated fake source | Cancel before batch return stored zero rows |
| Retention | Items 20 and 91 days old | 91-day item skipped; screen sweep kept 20-day import; import sweep removed it and its graph link |
| IPC | Tauri compile and mocked TypeScript wrapper test | Five commands registered; payloads and return types checked |
| Architecture | Source-directory test | Connector files cannot name the Lance store |

`cargo test --lib import::` passed 29 tests. `cargo test --lib imported_` passed 3 focused leaf tests, with one unrelated model-weight test ignored. `cargo test --lib agent_note` passed 18 existing behavior tests. `cargo check --bin fndr`, `npm run typecheck`, `npm run build`, import wrapper and IPC drift tests, docs drift, and memory-read drift passed. The first binary check failed because `dist` was absent; it passed after `npm run build`. The initial import test run found a comment false positive in the architecture test, which was fixed.

`make qa-retrieval-check PERSONA=knowledge-worker` passed after reseeding the synthetic profile. Recall@5 stayed at 1.000 for search, ask and retrieve, with no change from reference. MRR@10 was 0.909 versus 0.966 reference on each path; the gate does not fail on that measure. The run reported one weak real query out of 35 and zero strong no-match errors out of eight. This gate contains no imported rows, so the fake-source tests are the evidence for import retrieval.

## Integration limits

`import/sources/mod.rs::registered()` is empty until B, C and D connectors land. The first-run source consent UI and per-source settings belong to E; no source is live in this isolated lane. H's range delete should call `import::tombstone_records(&state_store, &records)` before deleting imported rows, and A's Forget path should be routed through H's shared erase helper at integration. A patch for blocked `mcp/mod.rs` is in `~/FNDR-exchange/reports/A.md`.
