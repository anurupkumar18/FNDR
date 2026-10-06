# October 5 cloud integration: local verification

Status: automated integration and follow-on checks complete on `codex/integrate-cloud-oct5`. Native acceptance and production model migration remain pending.

## Inputs and review

Base: `c124957`. Reviewed exact cloud heads:

| Train | Head | Scope |
|---|---|---|
| G / PR 33 | `90bb70b` | VS-61 MCP authentication and token storage |
| F / PR 31 | `b95bcb9` | Retrieval, parser, BM25 indexing, Resume candidates, gate and chart |
| H / PR 34 | `1242b8b` | VS-47 inactive EmbeddingGemma contract and reference vectors |

Live GitHub reads showed successful checks at all three heads, including macOS Rust tests and the train-F retrieval gate. This is separate from local verification below. Three independent source reviews found the following integration corrections.

## Corrections and red/green evidence

| Boundary | Observed failure before correction | Correction and verification |
|---|---|---|
| MCP token storage | `an_existing_token_file_is_private_before_secret_bytes_are_written` observed mode 0644 before a secret write, expected 0600. | Restrict the opened descriptor before truncation/write; propagate permission errors. Existing token reads use the same secured descriptor. Local `cargo test --locked --lib mcp::`: 16 passed, including denied payloads through both POST routes. |
| Embedding contract | `embeddinggemma_contract_rejects_unsupported_dimensions` accepted dimension 0; unsupported sizes shared the 128-dimensional table identity. | Checked constructor accepts only 128/256/512/768. Local `cargo test --locked --lib embeddinggemma_contract`: 2 passed. Active v4 remains unchanged. |
| Date/app parsing | `overlapping_weekday_alias_does_not_hide_a_separate_app_filter` returned no app for "notes on Monday from Slack". | Skip conflicting candidates while continuing to search for another app. Local `cargo test --locked --lib context_runtime::query_filters::tests`: 10 passed. |
| Retrieval gate | Two new Python regressions showed rank 8 to 10 losses returning success despite no measured numerical-tie evidence. | Restore strict failure for every previously found query becoming a miss. Comparator: 29 tests passed after the two intended red failures. Remove unused warning machinery and align CI/Makefile descriptions. |

The Rust red failures were assertion failures after successful compilation, not inferred from source. A borrow-lifetime compile error in the parser correction was fixed before green verification.

Additional focused verification:

- `cargo test --locked --lib embedding::`: 27 passed, 1 ignored.
- `python3 -m unittest discover -s scripts/audit -p 'test_*.py'`: 98 tests run, 1 skipped, no failures.
- `git diff --check`: passed.
- Added-line public-diff scan: no personal home paths or credential prefixes. Bearer-header matches were test inputs and explanatory evidence, not real credentials. No added en/em dashes.
- New software-engineer persona JSON and the 20-item, 768-dimensional reference fixture were inspected as synthetic test data.

## Retrieval reproducibility investigation

The local MiniLM ONNX file matches the pinned SHA-256 `759c3cd2b7fe7e93933ad23c4c9181b7396442a2ed746ec7c1d46192c469c46e`.
The installed tokenizer hash is `d241a60d5e8f04cc1b2b3e9ef7a4921b27bf526d9f6050ab90f9267a1f9e5c66`; the pinned artifact hash is `da0e79933b9ed51798a3ae27893d3c5fa4a201126cef75586296df9b4d2c62a0`.
Parsed JSON differs only in padding and truncation: installed values are null; the pinned file pads/truncates to 128. The vocabulary is unchanged. A separate pinned copy was downloaded to scratch storage; the installed models were not changed. The runtime uses the tokenizer's encoding settings. On the same M1, a fresh office-pm profile using the installed tokenizer reproduced the reported lost query: "would clients recommend us, and did that improve" fell from rank 9 to a miss on Search, Ask and shared retrieval. MRR@10 fell from 0.661 to 0.613 while Recall@5 stayed 0.900. A fresh profile using the pinned tokenizer matched every reference rank. This establishes reproducible asset-dependent behavior on the same hardware, rather than evidence for a cross-platform numerical tolerance. Seeds used the same corpus on the same date, several minutes apart; elapsed-time effects were not held to an identical injected clock.

## Remaining gates and limits

- Full `CARGO_BUILD_JOBS=1 make test`: passed (TypeScript, 79 frontend files / 513 tests, production frontend build, 1,017 Rust tests passed / 17 ignored). Elapsed 384.68 seconds; maximum process RSS 2,329,395,200 bytes. This is build/test process memory, not the app footprint. The storage benchmark rewrote its historical report; this run was retained in scratch evidence and the historical file restored.
- Fresh synthetic retrieval gates on the M1: knowledge-worker, office-pm and software-engineer all passed with the pinned MiniLM model and tokenizer. All three reports say no per-query rank changed from their committed reference. Separate scratch profiles and explicit `FNDR_EMBED_MODEL_DIR` were used; the owner profile was untouched. The installed-tokenizer office-pm comparison failed as described above; this is an existing asset mismatch, not a newly accepted reference.
- Explicit q8 model-reference inference ran and failed the fp32 parity threshold at 768 dimensions (lowest cosine 0.992135, mean 0.994540; required 0.999). This quantized export is not numerically interchangeable with the fp32 reference. Ranking and standalone M1 cost measurements are recorded below; production prompt wiring and migration remain unvalidated. The test stopped at 768, so it did not evaluate 256.
- Native permission, capture, reopening and usefulness checks remain in the owner's batched QA session.
- Removing the always-empty graph route does not deliver connected memory. Persisted traversal and the VS-69 proposal still require the graph acceptance work in the month plan.
- BM25 index-on-write needs measured native write-cost evidence before making capture-budget claims.

## Follow-on fixes

- MiniLM bootstrap now reads filenames, revision URLs and SHA-256 values from the existing Rust model configuration, verifies before promotion, and refuses to overwrite a mismatched installed asset. No new download dependency. Four offline subprocess tests passed after three observed behavioral failures in the old script; shell syntax and verification against the real pinned constants also passed. Commit `7087728`.
- Empty or secure-only Accessibility web areas now return empty page text, allowing the existing capture path to use OCR instead of accepting browser chrome. Reused observed role counts; no new traversal layer. Two synthetic tests failed on the old behavior; all nine collector tests passed after the one-line correction. Commit `1831c50`. This does not prove native Chrome discovery or the 12-app matrix.

## EmbeddingGemma local measurements

The explicit fp32 reference test passed all 20 sentences at both dimensions: minimum/mean cosine 0.999978/0.999986 at 768 and 0.999980/0.999988 at 256. Invoked the built reference-test binary with isolated assets and `--ignored --nocapture`. Its process took 8.86 seconds, peak RSS 881,016,832 bytes; this combines both model dimensions and is not a per-model serving budget.

All assets use `onnx-community/embeddinggemma-300m-ONNX` revision `5090578d9565bb06545b4552f76e6bc2c93e4a66`:

| Asset | Bytes | SHA-256 |
|---|---:|---|
| fp32 model.onnx | 479,932 | `ea91fd315a7c152d427d231746f0f811a1ac93beaba656abfdf2b24e091265e4` |
| fp32 model.onnx_data | 1,234,521,088 | `ef835ae565d8695236652475903078e8ed794c7c35faf1164d78ec3238e8a88d` |
| q8 model_quantized.onnx | 567,874 | `172efde319fe1542dc41f31be6154910b05b78f7a861c265c4600eec906bd6d8` |
| q8 model_quantized.onnx_data | 308,890,624 | `705626e28e4c23c82ade34566b4197d97f534c12275fa406dfb71e9937d388c0` |
| Shared tokenizer.json | 20,323,312 | `4dda02faaf32bc91031dc8c88457ac272b00c1016cc679757d1c441b248b9c47` |

For q8 the isolated directory aliases the graph as `model.onnx`; its external data filename remains unchanged. The new opt-in `embedding_measure` example requires an explicit model directory, disables mock fallback and never opens a memory store. Two input-validation tests pass.

Reproduction shape (run the built executable so compile memory is excluded):

```sh
CARGO_BUILD_JOBS=1 cargo build --manifest-path src-tauri/Cargo.toml --example embedding_measure
FNDR_EMBED_MODEL_DIR=/path/to/isolated/pinned-assets /usr/bin/time -l \
  /path/to/cargo-target/debug/examples/embedding_measure 256 synthetic-inputs.json > result.json
```

Omit the input filename to use the 20 committed reference inputs. Repeat in a fresh process for 768 and for each precision. The 192-input corpus was exported using `embedding_bakeoff.load_persona`, `app_primary_text`, and `chunk_text(chunk_source_text(...))` for both existing personas, with all query and document rows in reference-shaped `{items:[{kind,text,id}]}` JSON. Scoring reuses `rollup_chunk_scores`, stable `rank_ids`, and `evaluate`; no reimplementation of the composer or metrics. Every input formed one distinct chunk.

| Export / dimension | Load plus probe ms | Corpus ms per input | Peak RSS MB | Record Recall@5 / MRR@10 | Chunk Recall@5 / MRR@10 |
|---|---:|---:|---:|---:|---:|
| fp32 / 256 | 3,213 | 95.37 | 866 | 0.976 / 0.907 | 0.976 / 0.875 |
| fp32 / 768 | 3,850 | 92.77 | 808 | 0.976 / 0.913 | 1.000 / 0.889 |
| q8 / 256 | 3,256 | 100.07 | 1,574 | 0.976 / 0.908 | 1.000 / 0.876 |
| q8 / 768 | 3,115 | 100.22 | 1,596 | 0.976 / 0.913 | 1.000 / 0.887 |

These are one-pass development-build observations on the M1 8 GB, decimal MB, batch size four. Corpus time includes preprocessing and mixes query/document inputs; it is neither single-query p95 nor representative long-chunk throughput. Model load includes a dimension probe. The public wrapper can clean, chunk and pool longer texts, with role prefixes applied before chunking; those long-text semantics need validation before migration. No repeated cache hits were timed. Native capture and model concurrency were not exercised.

q8 versus fp32 changed only these relevant ranks in this corpus: 256 record "Field Order 15" 7->6 and "the hiring debrief from yesterday" 2->1; 256 chunks "single sign-on" 6->5; 768 chunks "architecture slide" 3->4. Neither q8 dimension lost a prior top-ten result. q8 still fails exact fp32 parity, including minimum cosine 0.992948 at 256 in the separate measurement. ADR 019 now recommends fp32/256 for the next isolated prototype, with final activation/resource and distribution-term decisions pending.

## Resume Home and final follow-on verification

Resume now excludes low-signal/quarantined/internal-app and agent-source records before grouping. It reuses current read-side helpers and the existing agent-note contract. This is not a new policy for deleting historical memories when a blocklist changes. Two stored-record regressions failed on hidden-only threads and refreshed eligibility/deletion, then passed after the filter. An initial test setup attempted a nested Tokio runtime; that harness error was repaired before observing the behavioral red.

Home now shows up to three recent threads under search, their latest state, and one cited suggested step. Source buttons use the existing Vault navigation. Loading, empty, error/retry and explicit refresh states are provided. The section mounts only on unlocked active Home, cancels stale responses and uses no polling. Removed the unreachable duplicate Home search bar.

After these changes: TypeScript passed; all 80 frontend files / 521 tests passed; production build passed; Rust library 944 passed / 9 ignored; existing Resume integration 3 passed; measurement example 2 passed. The full pre-follow-on integration sweep is recorded above. Native QA remains deferred.

Browser verification used the real frontend at 1280x900 and 360x800 with synthetic Tauri IPC fixtures: populated threads, suggested-source navigation to the Vault dialog, Home reentry, error/retry, empty state, refresh, and light/dark presentation. At 360 px both document and body width were 360 px; no horizontal overflow. No browser console errors appeared. These checks establish frontend presentation/navigation only; exact source callback IDs are covered by component tests, while native Vault/reopening remains in batched QA. Screenshots are local scratch artifacts, not captured user data.

Independent final source review found no new blocking Resume/UI findings. Follow-on commits: `c6b12f1` (Resume eligibility), `124caa6` (Home presentation and dead branch removal), `25a95df` (isolated ONNX measurement and ADR update).

## Capture-source evidence (VS-45 / VS-46)

Reused persisted `raw_evidence.source_kind`; no database column or migration. New merges retain sorted, bounded `text_source_kinds` observation lineage. Missing or unrecognized methods remain unknown, including visual-only captures. A mixed label alone cannot reconstruct historical constituents. The array takes precedence when nonempty; arbitrary labels never enter the derived DTO or reports. This describes observed methods, not attribution of individual stored characters.

Search, Vault, grouped cards and MCP responses expose the same compact category. Expanded cards show "Text captured via" without fetching the debug inspector. Removed the duplicate MCP-to-search converter in favor of the existing shared converter. The health report and scoreboard show source count/share, median clean-text length and share under 200 characters. Python discards raw JSON after categorization rather than retaining another list of evidence blobs; the existing Arrow read is not a new streaming scanner.

Observed red then green at merge lineage, serialized cards/MCP and UI labels. Review also found Python Unicode case folding accepted a label Rust rejected: three new confusable-label cases failed before aligning Python to ASCII labels. Focused audit tests: 37 passed. Full validation before that parser-only correction: 952 Rust library tests passed / 9 ignored, 527 frontend tests passed, typecheck passed, and 104 audit tests run / 1 skipped. The real-store review persistence regression passed separately. Ticket-plan validation and whitespace checks passed.

Synthetic seeded-profile CLI proof: `vault-health` followed by `scoreboard` preserved unknown provenance for all 20 legacy rows (100%), median 188.5 characters, and 60% under 200 characters. This is report plumbing evidence, not a measured native capture-quality claim. No owner vault was read or changed.

Browser verification used synthetic IPC fixtures at 1280x900 and 360x800. The expanded card showed "Multiple sources"; viewport/document widths matched, no debug-inspector request occurred, and the console contained no errors. A missing graph-response mock was corrected before the final screenshots. Native capture and the 12-app matrix remain deferred to batched manual QA.

ANTI-BLOAT REVIEW
- Behavior delivered: trustworthy source labels and an aggregate quality breakdown.
- Complexity added: bounded JSON normalization and one derived DTO field; no database schema or query layer.
- Bloat risks: duplicated category rules across Rust/Python, covered with matching edge cases.
- Simplifications required: Unicode normalization aligned before delivery.
- Code to delete or merge: duplicate 80-line MCP converter removed.
- Interface improvements: compact labels shared by humans and agents; raw evidence stays internal.
- Testability gaps: native source distribution and historical lost lineage remain unproven.
- Verdict: approve. The focused Rust parser test passed after adding the matching Unicode cases; public-diff scan and whitespace checks passed.

## Agent write-back integration (VS-68)

Reviewed cloud PR 35 at `3a41b4e75b22e30701b32b28343908bc7effda9e`; its live checks passed, including macOS Rust tests. Merged locally at `d9b132f`, preserving both sides of the append-only cloud session log. The feature remains off by default and the owner configuration was not changed.

Local review found gaps outside the cloud corpus's immediate write assertions. Behavioral regressions observed before fixes:

| Boundary | Observed failure | Local correction |
|---|---|---|
| Project metadata | HTTP requests with a secret or blocked term only in `project` were stored. | Apply the existing detector and blocklist to title, body and project. |
| Other memories' review context | A recording provider received an agent note as a neighboring candidate while reviewing an ordinary capture. | Exclude notes before truncating the candidate set. |
| Capture merging | Four stored/batch tests selected the note or changed the screen ID to the note ID, including deliberately stale continuity anchors. | Exclude notes from semantic/lexical candidates, incoming story eligibility and both anchor shortcuts. |
| Reopening | Legacy `Reopen:` note text produced a URL target in cards and the actual command's resolver. | Guard both resolvers before any typed or legacy fallback; include URL, file and deep-link regressions. No OS open action was performed by these tests. |
| Read provenance | Stored SearchResult JSON omitted source type; MCP inferred `application`. | Project existing source/client fields through SearchResult and cards. Agent MCP rows report agent origin; existing non-agent MCP categories remain compatible. |
| Derived work context | Reading a note through a context pack created an activity record. | Keep notes out of activity/graph/project derivation, including direct sync; preserve the stored row unchanged. |
| Answer evidence | Ask presented note snippets without attribution. | Label note evidence with client, timestamp and memory ID in an explicit untrusted-data JSON block. |

Agent cards bypass generative synthesis and capture-specific text cleanup, preserving the note's own text. Frontend regressions also exposed capture icons/status and a hidden narration-filtered note. Notes now show Added, Title and client attribution, with neither reopen controls nor screen-similarity actions. The UI renders URL-like text as plain text. No new persistence schema or framework was introduced.

Browser checks at 1280x900 and 360x800 used synthetic fixtures, including a note containing `Reopen: https://example.com`. The final card showed Added and the client, no actionable link/reopen control and no screen-similarity button. Document width matched the viewport. The initial page opened without a Tauri mock and logged unavailable-IPC errors; after fixture initialization there were no further console errors. These are frontend checks, not a Claude Code native demo.

Additional review caught capture normalization flattening note whitespace/removing literal `[LOW_CONF]`, and first-sentence answer snippets omitting later qualifications. Their storage/answer regressions and final integration results are recorded at the next checkpoint below.

### Final local checkpoint

- The small normalization regression reproduced lost newlines/indentation and literal marker removal. Capture cleanup now skips authored note bodies. The 4,000-character real-store roundtrip includes code, indentation, a literal marker and a final-word keyword lookup.
- Batch persistence still dropped a screen record beside an identical note after merge isolation was fixed. A separate search regression returned one row instead of two notes plus their matching capture. Insert/search dedup now keys notes by their ID, preserving ordinary capture dedup behavior.
- The maximum-length storage test exposed an existing non-advancing chunk loop. A process sample located it in `chunk_by_chars_with_spans`: a whitespace match at offset zero left the start unchanged. The initial full gate was stopped after the test exceeded 90 seconds. Ignoring that zero-offset boundary restores forward progress; ASCII and multibyte regressions check complete span coverage, limits and preserved tails. The formerly hanging storage test passes.
- Ask now uses the full stored note body in its attributed JSON block. A two-sentence correction regression first reproduced the omitted qualification, then caught a duplicated first sentence when derived context was preferred over the body; the final test passes.
- Final `CARGO_BUILD_JOBS=1 make test`: typecheck, 80 frontend files / 534 tests, production frontend build, and 1,066 Rust tests passed / 17 ignored, zero failures. This includes the real HTTP injected-note corpus, capture fixtures, merge replay, retrieval and storage integration suites. The generated storage-index benchmark was saved in scratch evidence and the historical report restored.
- Final synthetic browser pass additionally checked multiline note text and indentation at 1280x900 and 360x800. Computed whitespace mode was `pre-wrap`, document/body widths matched the viewport, URL text produced no links, and this fresh fixture-first session logged no errors.
- Source inspection added a post-inference real-backend check: even when development mock fallback is enabled, a real-model failure must not admit a mock-vector note. The focused MCP rerun passed all 34 tests, including the HTTP corpus; no runtime ONNX-failure injection is claimed. Final whitespace and added-line public-data checks passed.

ANTI-BLOAT REVIEW
- Behavior delivered: independently stored, attributed, inert assistant notes and fixes for concrete integration failures.
- Complexity added: derived DTO fields and guards in existing capture, storage, review and presentation boundaries.
- Bloat risks: duplicated source heuristics were considered; the existing non-agent MCP category behavior was deliberately retained for compatibility.
- Simplifications required: use stored note bodies, existing source identity and current normalizers; no parallel note store or framework.
- Code to delete or merge: capture-only actions and cleanup are bypassed for notes; obsolete one-page status claims were replaced in place.
- Interface improvements: notes expose origin/client and cannot supply reopening targets or become derived work context.
- Testability gaps: native assistant demo, settings opt-in and the remaining product work below stay pending.
- Verdict: approve with the documented default-off scope and remaining product gates.

Still pending for this feature: Settings opt-in, Vault filtering/bulk client deletion, Privacy Activity, cleanup when a new blocklist rule matches note body text, and the native assistant-to-FNDR demo. The older decision-ledger write has its separate open policy question. These limits remain explicit in the cloud evidence and feature plan.


## EmbeddingGemma role prefixes and workload checkpoint (VS-47 / VS-48)

TDD SUMMARY
- Behavior implemented: the opt-in `EmbeddingInput` boundary chunks raw query/document inputs and composes document context before adding the contract prompt to every chunk. It reuses existing cache keys, batch inference and mean pooling. Legacy production wrappers and the active v4 contract remain unchanged.
- Tests added/updated: long queries/documents, mixed-role ordering and repeated-input cache reuse, contextual title placement, blank/low-signal vectors and unchanged v4/v5 wrappers. Both real-reference and measurement callers now use this raw-input boundary.
- Failing test observed: `cargo test --locked --lib role_prompt_reaches_every_long_document_chunk` reproduced later cached model inputs without the document prefix. After the change the regression passed; `cargo test --locked --lib embedding::` passed 34 tests with one intentional ignore.
- Measurement regression: a tokenizer configured for truncation/padding incorrectly reported `[8, 8]` rather than actual special-token-inclusive counts `[7, 3]`. The diagnostic tokenizer now disables both settings; all four `cargo test --locked --example embedding_measure` tests pass.
- Real-model verification: the explicitly enabled fp32 `embeddinggemma_reference` test passed all 20 reference inputs at both dimensions, with lowest cosine 0.999978 (768) and 0.999980 (256). Production assets and the owner vault were not changed.
- Files changed: existing `embedding/onnx.rs` and module exports, `examples/embedding_measure.rs`, and `tests/embeddinggemma_reference.rs`. No new runtime dependency, index or model manager.
- Remaining risk: existing raw-text fallback after chunk cleanup is retained; this slice does not establish noise suppression, token-aware chunk sizing, production cutover or concurrent capture performance. Per-chunk truncation/normalization and record-level mean pooling are unchanged.

The measurement example now offers `--single-input` with per-input latencies and rejects duplicate prepared inputs in that mode. Shared chunks still use the normal cache. Token diagnostics inspect the exact prepared strings with prompts and special tokens, after dropping the embedder and outside inference timing. Process peak RSS includes initialization and this diagnostic phase. These are debug-build process measurements, not an app latency or native-capture claim.

ANTI-BLOAT REVIEW
- Reused the existing chunker, prefix helpers, cache, inference batching and pooling; no second embedding pipeline or store.
- Removed whole-document prompting and duplicated chunk preparation from the two inactive harness callers. Kept legacy callers unchanged until a deliberate index migration.
- Exposed only the raw role input boundary and its prepared chunks for budget inspection. Added observable regression coverage instead of a generic harness framework.
- Independent read-only review approved the implementation and measurement boundaries. Full application gates were not repeated for an inactive Rust-only path; focused embedding tests, example tests and real-model parity cover this change. Native QA remains deferred.

### Repeated local workload measurements

M1 / 8 GB, pinned fp32 assets from the earlier checkpoint, serialized fresh processes with alternating dimension order. Query mode used 12 distinct single-chunk queries per process, three processes per dimension (36 timed queries). Document mode used 12 distinct synthetic documents per process, two processes per dimension. Documents span prose, numeric OCR/tables, code/logs and multilingual text, 3,051–6,716 Unicode characters each. All 104 document chunks were unique. This is a diagnostic workload without retrieval labels, not a quality score or concurrent native capture benchmark.

| Workload | Dimensions | Observed latency / throughput | Maximum process RSS, decimal MB |
|---|---:|---|---|
| Individual queries | 256 | 26.12 ms pooled median; 20.39–49.18 ms observed range | 760.0–831.0 |
| Individual queries | 768 | 25.60 ms pooled median; 20.36–31.17 ms observed range | 761.5–829.6 |
| 12 long documents / 104 chunks | 256 | 20.612–20.883 s per pass; 198.2–200.8 ms per unique chunk | 734.6–840.0 |
| 12 long documents / 104 chunks | 768 | 20.876–21.062 s per pass; 200.7–202.5 ms per unique chunk | 822.8–828.0 |

Initialization including its dimension probe took 3.134–4.868 seconds across these processes. Query inputs reached 24 actual tokens and document chunks reached 403, including prompts and special tokens; none exceeded the 2,048-token contract limit. Fresh process does not imply a cold filesystem cache. No percentile service objective or whole-app RAM guarantee is inferred from this small corpus.

Recommendation remains fp32/256 for the isolated migration prototype, with 768 as comparator. Its smaller stored vectors are the benefit here; these timings do not establish an inference speed or model-residency advantage from cutting the output dimensions. Reuse a loaded model for interactive queries and measure background embedding contention before production activation.

Local scratch evidence: `/tmp/fndr-integration-oct5/gemma-role-workload-summary.json`, per-process `gemma-role-{queries,documents}-{256,768}-r*.json` and `.stderr` (`/usr/bin/time -l`), and the focused red/green/reference logs in the same directory. Corpus files were retained locally rather than adding another large synthetic fixture to the repository: `gemma-queries.json` SHA-256 `aa9e3eaf5bccdd40539cde71121862658fe2ed1cd43f1d449038f86ddcd58530`; `gemma-long-documents.json` SHA-256 `c1d8908fd7d93fb9a18eb91829eced74982221d9244bdcf7c4d70a870e75f2c1`. These scratch paths are local evidence, not portable committed benchmark assets. Build with `cargo build --locked --example embedding_measure`, set `FNDR_EMBED_MODEL_DIR` to the separate pinned fp32 assets, and run the built example with `<dimension> <corpus.json>`, adding `--single-input` for the query corpus.


## Persisted Related memories

DIAGNOSIS REPORT
- Observed failure: Vault and MCP queried `record.text` instead of following saved links. Compaction cleared that field while `related_memory_ids` survived, so the restart fixture returned no related card.
- Smallest repro: `CARGO_BUILD_JOBS=1 cargo test --locked --lib related_memories_resolve_persisted_links_after_restart`. Observed red: `[]` instead of `["linked-target"]`; green after the shared resolver.
- Root cause and fix: both adapters duplicated similarity lookup. They now delegate to one existing-runtime resolver that follows persisted links, resolves consolidated aliases, deduplicates canonical IDs, excludes self, and applies current visibility to source and targets. It reads at most 64 distinct references and returns at most 12 cards, ordered by time then ID. Missing or hidden links do not trigger similarity replacement.
- Additional observed red: a later blocklist rule matching only an assistant note's body/project still exposed links. The resolver now checks the same retained title/body/project context as note admission; all four seed/target cases pass.
- Ordinary memories without links use surviving compacted text through existing hybrid retrieval, retaining ranked scores and route evidence. Unlinked notes remain leaves. Stored links have score zero and `stored_link` provenance, without invented semantic scores or graph paths.
- Verification: 6 focused related-memory regressions passed; the encompassing MCP module suite passed 20 tests. Retrieval integration passed 15 tests, including compacted-text fallback, seed exclusion and current app filtering. Expanded-card/panel tests passed 16 tests and typecheck passed. Commands: `cargo test --locked --lib related_memories_`, `cargo test --locked --lib mcp::tests::`, `cargo test --locked --test retrieve` (all with `CARGO_BUILD_JOBS=1`); `npm run typecheck`; `npm test -- src/domains/memory-vault/ExpandedMemoryCard.test.tsx src/domains/memory-vault/MemoryCardsPanel.test.tsx`.
- Browser evidence: synthetic IPC fixture at 1280 px and 360x800 showed stored-link labels, note authorship and wrapping long unbroken titles/client names without horizontal overflow. Clicking a related note opened its actual expanded-card UI and preserved attribution. Console contained only the React DevTools information message. Screenshots retained locally under `/tmp/fndr-integration-oct5/related-browser/`; mock browser evidence does not prove native capture or owner-vault quality.
- Temporary instrumentation removed: no production instrumentation added; owned browser and Vite server closed.
- Remaining risk: persisted graph traversal, timeline integration, native usefulness and global note cleanup on changed blocklist rules remain separate work. This slice checks current rules on related reads; it does not claim global deletion.

ANTI-BLOAT REVIEW
- Behavior delivered: saved relationships remain useful after restart for humans and agents, with truthful relationship labels and provenance.
- Complexity added: one bounded resolver in the existing context runtime and metadata in the existing card; no schema, dependency, service or new UI surface.
- Code removed/merged: duplicated IPC/MCP similarity logic replaced by shared resolution; existing storage lookup, privacy, card conversion and hybrid retrieval reused.
- Simplifications required: none after review. `parent_id`, unused legacy `related_ids` and consolidation aliases are not invented as relationship types.
- Testability gaps: native data quality and graph-scale costs remain unmeasured.
- Verdict: approve after independent read-only review; preserve teammate migration ownership and continue shared-model integration separately.


## Shared text-model session and initialization recovery

GRILL WITH DOCS RESULT
- Shared understanding: reduce duplicated local model residency while preserving caller-specific chunking, vector spaces and fallback behavior. This prepares VS-48/49 and does not implement Minh's EM-09 migration.
- Domain/interfaces: `Embedder` remains the caller wrapper; `RealEmbedder` owns the tokenizer/ONNX session. Canonical model directory plus full contract identifies a resident backend.
- Decisions resolved: share only the real backend; keep caches, chunkers and degradation flags separate. Use weak registry ownership, publish after the dimension probe, and serialize initialization. Search and meetings cache successful wrapper initialization only.
- Open boundaries: static wrappers still pin their session until exit; no idle unload or priority scheduler is claimed. Distinct model initialization serializes through one registry lock. In-place replacement of resident assets requires restart.
- Documentation: existing architecture overview, README and ADR 019 updated. No new domain term, plan or schema introduced. Next workflow: measured scheduling slice with TDD.

TDD SUMMARY
- Observed red: real MiniLM capture/search constructors produced different backend identities (`real_model_session_is_shared_without_changing_chunking`). Green after sharing the backend; vectors for the same short query match exactly while narrow capture chunking remains distinct.
- Lifecycle checks: four simultaneous initializers converge on one session; canonical directory aliases share; distinct directories and full contracts stay isolated; missing files can recover; final-owner drop releases the weakly registered session, followed by successful reload.
- Observed red: after a missing-model error the wrapper cache returned that same error instead of attempting the next initializer (`cached_embedder_retries_missing_assets_then_reuses_success`). Success-only caching now retries distinct failures and then reuses the initialized wrapper.
- Focused commands: `CARGO_BUILD_JOBS=1 cargo test --locked --lib embedding::` passed 35 tests, with three asset-dependent tests ignored. Explicit real-model run with `FNDR_EMBED_MODEL_DIR=/tmp/fndr-integration-oct5/models-minilm` and `cargo test --locked --lib real_model_ -- --ignored` passed both new asset-dependent lifecycle tests.
- Code reused: existing model resolver, contract, ONNX dimension probe and session mutex. No constructor call-site sweep, schema migration, dependency or new production module.

Contention evidence: the two fp32/256 runs in ADR 019 measured 778.75 to 807.79 ms query medians and maxima 1079.56 to 1103.17 ms while all 24 query starts overlapped the background call. Background duration was 19.621 to 19.652 seconds; second-wrapper construction about 0.062 ms; peak RSS 924.1 to 924.2 MB. This changes the next action to foreground admission and smaller background batches, with asynchronous retrieval kept responsive. It does not complete native capture or whole-app RAM acceptance.

Local scratch artifacts: `/tmp/fndr-integration-oct5/shared-session-{red,green,lifecycle}.log`, `shared-wrapper-red.log`, `shared-embedding-suite.log`, and `gemma-shared-contention-256-r{1,2}.{json,stderr}`. The temporary probe source is retained as `/tmp/fndr-integration-oct5/embedding_contention.rs`, SHA-256 `b2c2ab77751e3252b91aa84179049bf9d105012c2c171dfecc77fe0471a2d7f3`; its temporary repository copy was removed after building. It calls the public `embed_inputs` boundary with the same synthetic corpora recorded above and never opens a store. These local paths are diagnostic evidence, not committed portable benchmark assets. The ignored lifecycle tests are committed and reproducible with pinned MiniLM assets.

ANTI-BLOAT REVIEW
- Delivered behavior: one resident real backend per asset location/contract across existing callers, without erasing their preprocessing or fallback policies; missing-asset initialization errors no longer stick permanently in search/meetings.
- Complexity added: a small weak registry and one tested success-cache helper in the existing embedding module. Most added lines are lifecycle regressions.
- Simplifications: removed duplicated cached-error branches; no new model service or scheduler framework. Temporary measurement source removed from the checkout.
- Independent review: approved, no actionable race or fallback-isolation defect. Limits above remain explicit; resident assets are immutable and runtime mock recovery is separate.


Final broad validation for this slice: `CARGO_BUILD_JOBS=1 cargo test --locked` from `src-tauri/` passed **1,079 tests**, with 19 intentionally ignored, across 21 reported test targets including doc tests. Existing compiler warnings remain. No frontend code changed, so the previous frontend/browser evidence was not re-run. An initial invocation from the repository root stopped immediately because that directory has no Cargo manifest; the corrected command above completed successfully.

Next scheduling slice: keep `RouteCtx` borrowed, clone an owned embedding handle into `spawn_blocking`, and offload cold model initialization as well as warm query inference. Handle clones should share one wrapper's cache/degradation state while separately constructed wrappers remain independent. At the real backend, admit one background chunk at a time, give waiting queries bounded preference, and serve a waiting background request after a fixed foreground burst. Verify FIFO/fairness and error release deterministically; use a current-thread async test to prove executor progress; repeat the same contention workload and report queue/service/total time. A timed-out blocking task continues running, so a timeout alone cannot provide cancellation.


Real fp32 reference check also passed after the shared-session change: `FNDR_EMBED_MODEL_DIR=/tmp/fndr-integration-oct5/models-embeddinggemma-fp32 CARGO_BUILD_JOBS=1 cargo test --locked --test embeddinggemma_reference -- --ignored --nocapture`. Lowest cosine remained 0.999978 at 768 dimensions and 0.999980 at 256. The broad suite regenerated the unrelated storage-index timing report; that run was retained locally as `shared-session-storage-indexes.md` and the tracked historical report restored. No owner assets, vault data or active model contract changed.


## Interactive embedding admission and async retrieval

GRILL WITH DOCS RESULT
- Behavior: keep Search/Ask/MCP responsive while background text embedding runs, without changing model contracts, stored schemas, prompts or pooling. The dedicated Qwen worker serves a different model and was not reused as a text scheduler.
- Design: a private per-backend admission module provides FIFO within foreground/background queues and one active call. Waiting queries have preference, with background admitted after at most four foreground calls. Document/mixed calls yield after each prepared chunk. Existing caches bypass admission; no new queue service or dependency.
- Ownership: cloned embedding handles share one wrapper's cache and degradation flag, allowing safe `spawn_blocking` work. Separately constructed wrappers retain independent state. `RouteCtx` stays borrowed.
- Open limits: the current ONNX call is nonpreemptible; the gate provides scheduling-turn fairness, not an end-to-end deadline. Abandoning an async wait does not cancel already running blocking work. Cold initialization is off-thread but still awaited before unified route dispatch. Whole-app capture/VLM memory and latency remain separate gates.

TDD SUMMARY
- The actual current-thread vector-route regression failed before offloading: a held cache blocked the executor until its five-second watchdog released it. It now yields to the async release branch and still returns the seeded memory. Model initialization, vector inference and lazy BGE initialization/inference now run on blocking workers. Legacy MCP raw-search and mobile-search cold initialization use that boundary too.
- The new admission gate's initial FIFO baseline failed the foreground-overtaking and four-admission fairness cases (3 pass / 2 fail); changing only its selection policy made all five cases pass. Tests also verify class FIFO, one active threaded call and release after a backend Result error. The first attempted gate run hit temporarily missing cross-agent APIs; the runtime red above was observed after those APIs were available.
- Focused verification: `CARGO_BUILD_JOBS=1 cargo test --locked --lib embedding::` passed 41 tests / 3 ignored; `cargo test --locked --lib context_runtime::` passed 72 tests, including the executor regression. Legacy v4/v5 query text, cache sharing and separate-wrapper degradation state are checked explicitly.
- Real Gemma reference: fp32 passed at 768 and 256 dimensions after background batches changed to one chunk. Lowest cosines remain 0.999978 and 0.999980. Command: `FNDR_EMBED_MODEL_DIR=/tmp/fndr-integration-oct5/models-embeddinggemma-fp32 CARGO_BUILD_JOBS=1 cargo test --locked --test embeddinggemma_reference -- --ignored --nocapture`.
- Retrieval: freshly seeded profiles with pinned MiniLM assets passed `make qa-retrieval-check` for knowledge-worker (39 cases), office-pm (37) and software-engineer (34). Search, Ask and shared retrieval had **no per-query rank changes** against their accepted references. The previous pinned runs provide the before evidence; new outputs are under `/tmp/fndr-integration-oct5/scheduled-retrieval-<persona>/`. Seeded profiles are separate from the owner vault. The ranking gate raises route time budgets and does not prove production tail latency.

Repeatable local contention method and numbers are recorded in ADR 019. Same corpora and request cadence as the preceding probe: two wrappers, 104 document chunks, 12 distinct queries, 200 ms gap before each query, two fresh processes. All 24 query starts overlapped background embedding. Query medians fell from 778.75–807.79 ms to 166.72–200.60 ms; maxima from 1079.56–1103.17 ms to 223.79–312.80 ms. Background completion stayed 19.319–19.827 s versus 19.621–19.652 s before. Scheduled process peak RSS was 731.9–823.6 MB versus 924.1–924.2 MB before; do not extrapolate these few samples to a whole-app budget. Exactly 12 foreground and 104 background admissions were observed per run. Query service medians were 26.05–26.16 ms; queue medians 140.45–173.82 ms.

Artifacts: `/tmp/fndr-integration-oct5/embedding-scheduler-summary.json`, `gemma-scheduled-contention-256-r{1,2}.{json,stderr}`, `query-executor-red.log`, `embedding-admission-red.log`, `embedding-scheduler-green.log`, `embedding-executor-green.log`, and `scheduled-gemma-reference.log`. Temporary probe source `embedding_contention_scheduled.rs` is retained in that scratch directory (SHA-256 `105ac905ccf2e911e7eccb9b3939533b2be2029e0a891f4339189f27df26c8ba`); only debug timing initialization differs from the previous probe. Its temporary checkout copy was removed after measurement. These local measurement files are not committed portable fixtures; policy/executor regressions and reference tests are committed.

ANTI-BLOAT REVIEW
- Added one small private synchronization module because admission has an independently testable policy and lifecycle, separate from tokenization/ONNX. Reused the model/session, cache, runtime metrics, blocking executor and retrieval route interfaces.
- No scheduler framework, settings switch, new dependency, schema change or UI surface. Timing labels contain no captured text. Five admission tests protect the concurrency policy rather than duplicating model behavior.
- Independent review found no actionable locking or fallback-isolation defect. Cache locks are released before admission; the construction-only dimension probe runs before backend publication. Native workload quality, cancellation and cross-model scheduling remain explicit gaps.


Final broad validation: `CARGO_BUILD_JOBS=1 cargo test --locked` from `src-tauri/` passed **1,086 tests**, with 19 intentionally ignored across 21 reported targets including doc tests. Both real MiniLM lifecycle checks also passed explicitly (`FNDR_EMBED_MODEL_DIR=/tmp/fndr-integration-oct5/models-minilm CARGO_BUILD_JOBS=1 cargo test --locked --lib real_model_ -- --ignored`). Logs: `scheduled-full-rust.log` and `scheduled-real-lifecycle.log` in the scratch directory. Existing compiler warnings remain. The suite-generated storage-index report was retained as `scheduled-storage-indexes.md`; the tracked historical report was restored. No frontend code changed, so frontend/browser checks were not repeated.

Next experiment: reuse `inference::tests::extraction_fits_default_token_budget` and its eight synthetic captures for fresh-process Qwen-only and Qwen-plus-Gemma runs. Preserve production extraction limits; record parse/repair outcomes, token and latency traces, actual overlap, query queue/service time, CPU, RSS and physical footprint through existing telemetry. Verify the resolved model path before running: `InferenceEngine::new(Some(dir), ...)` can fall back outside that directory. The text engine retains its model for process lifetime, and existing real-model tests note Metal teardown aborts; report measurements and exit status separately. The worker's 90-second timeout currently logs unload without releasing the model. This experiment will establish a text-path budget only; pixel inference owns a separate runtime/context and needs its own later gate. No owner capture or store is needed.


## Combined Qwen extraction and Gemma contention

PROTOTYPE REPORT
- Learning question: how does the real Qwen text-extraction path coexist with inactive fp32/256 Gemma queries and background documents on the M1 8 GB? Does successful parsing establish useful grounded context?
- Location: disposable `examples/combined_model_probe.rs`, built with `CARGO_BUILD_JOBS=1 cargo build --locked --example combined_model_probe`, then removed from the checkout. Source retained at `/tmp/fndr-integration-oct5/combined_model_probe.rs`, SHA-256 `a9bd0536b1568b845b190398c9a5a240150a1fec74bf29622bbe6ac3048a15b0`. No production code, dependency, configuration or database changed.
- Inputs: existing eight `tests/fixtures/extraction_cases.json` cases, the preceding synthetic 12-document/104-chunk Gemma corpus, and 32 distinct queries at a 200 ms gap after each response. Actual query text is recorded in the retained source. All inputs are synthetic; no native capture or owner vault was opened.
- Models: installed Qwen3VL-2B-Instruct-Q4_K_M GGUF, 1,107,409,952 bytes, SHA-256 `089d75c52f4b7ffc56ba998ffc50aae89fcafc755f9e7208aacca281dca6c2ae`; separately pinned fp32 Gemma assets already recorded above. An isolated output directory symlinked only the existing GGUF. The resolver path was checked against its canonical installed path before loading and again after engine construction. Existing production extraction limits remained 4,096 context / 640 output tokens / 4,000 input characters.
- Alternatives: one fresh Qwen-only process, then one fresh Qwen-plus-Gemma process. The latter overlaps Qwen extraction, Gemma foreground queries and one background document batch. Existing system telemetry sampled at approximately 500 ms plus sampling overhead; `/usr/bin/time -l` supplied process maxima. No other root build or model benchmark ran concurrently.

| Diagnostic | Qwen only | Qwen + Gemma |
|---|---:|---:|
| Parsed extractions | 8/8 | 8/8, identical structured outputs |
| Repair calls / output-cap hits | 0 / 0 | 0 / 0 |
| Extraction median / maximum | 12.736 / 15.607 s | 14.541 / 16.084 s |
| Total extraction time | 101.630 s | 106.057 s |
| Query median / maximum | not run | 227.958 / 944.900 ms |
| Query queue / service median | not run | 189.067 / 41.275 ms |
| Background 104-chunk completion | not run | 29.366 s |
| Process maximum RSS (`time`) | 1,662,287,872 bytes | 2,160,279,552 bytes |
| Process peak physical footprint (`time`) | 568,906,816 bytes | 1,310,988,992 bytes |
| Sampled peak process CPU | 35.65% | 365.33% |
| Host pressure samples | 212 high | 138 high, 75 moderate |

All 32 query starts overlapped both extraction and background embedding. Logs show 32 foreground and 104 background admissions. Prompt tokens were 406–1,694 and generated output 316–573 tokens. Qwen initialization took 10.777 s in the first process and 1.358 s in the second; this is not a controlled cold-filesystem comparison. Gemma load/probe took 5.733 s. The existing heavy-model pressure policy recommended skipping at every baseline sample and 138/213 combined samples. This probe deliberately exercises the raw text/model boundary, not capture admission. Host pressure varied; one run per alternative is diagnostic and does not establish a percentile objective, causal slowdown ratio, whole-app headroom or production acceptance. RSS and physical footprint are different measurements. Pixel inference, CLIP, app rendering, store indexing and a long capture soak are absent.

Both processes saved their complete measurements and then exited **134** during the documented Metal teardown `GGML_ASSERT`. Treat the data as completed measurements with abnormal process termination, not a passing native lifecycle test. Raw data/traces/logs are under `qwen-gemma-{baseline,combined}/`; `qwen-gemma-summary.json` and `summarize_combined.py` sit in the same scratch root. To reproduce, temporarily restore the retained source, build the example, use a fresh isolated output directory with a `models/<Qwen filename>` symlink, and invoke the built executable as `<baseline|combined> <isolated directory> <expected installed GGUF path>` with `FNDR_EMBED_MODEL_DIR` set to the separate pinned Gemma directory. Unset `FNDR_INFERENCE_N_CTX`, `FNDR_INFERENCE_N_BATCH` and `FNDR_INFERENCE_N_UBATCH`. The probe refuses an existing `result.json`.

Grounding findings, separate from parsing: the sparse-page output invents an intention to access services and a next step to open one; code/article/inbox outputs add advice absent from their sources. The spreadsheet output says 20 rows although the source has 40 data rows and only 1,718 characters, below the input cutoff. Confidence is 0.9–0.95. These are development findings, not a human-labeled quality score. Current capture validation does not inspect `next_steps`/`todos`, lets confidence ≥0.8 retain unsupported scalar fields, and uses word overlap that ignores two-digit numbers. Accepted action lists are then persisted. The existing WS2 scorer separates format validity and lexical support but does not consume `must_mention`/`must_not` gold constraints; lexical overlap alone cannot prove factual support.

Recommendation and implementation guidance:
- Keep Gemma inactive pending migration and acceptance. Reuse the existing capture-validator tests and WS2 scorer to add source-based action/intent/numeric regressions, including positive cases where explicit next steps must survive. Do not convert every list to empty or claim that a prompt tweak proves grounding. Preserve held-out gold fixtures.
- Before adding unload, replace the text inference closure's transmuted `&self` with owned runtime state. Waiting futures can be cancelled while `spawn_blocking` continues, and the app supports replacing its engines. Prove the owned runtime survives cancellation and external-owner drop until the job finishes.
- Actual text and pixel weights are deliberately leaked; the pixel runtime has its own strong cached ownership and larger context. `QwenVlmWorker`'s idle timer only logs unload and has no production constructor caller. Fix misleading lifecycle claims, but do not describe that as memory reduction or implement drop/reload while weights leak.
- Delete the disposable probe from production paths (done). No new model service, queue framework or manual QA request. Repeat targeted model measurements after the ownership/grounding slices; pixel and whole-app acceptance remain later gates.


## Owned blocking text inference

DIAGNOSIS REPORT
- Observed failure: `complete_with_control` extended `&InferenceEngine` to `'static` with `transmute` before `spawn_blocking`. Cancelling its waiting future does not stop the worker, while the app can replace/release engines. The model's leaked weights do not keep the context, template or trace metadata alive.
- Smallest safe repro: convert only the context container to `Arc<Mutex<_>>`, retain the old borrowed closure, hold the context lock, manually poll the actual completion once, and inspect ownership before/after dropping the waiter. Cancel and drain the runtime while the original engine remains alive before asserting. This observed **`blocking job must own its context`** without dereferencing freed data. Command: `FNDR_INFERENCE_TEST_APP_DIR=/tmp/fndr-integration-oct5/qwen-gemma-baseline CARGO_BUILD_JOBS=1 cargo test --locked --lib cancelled_completion_keeps_context_owned_until_blocking_job_finishes -- --ignored --nocapture`.
- Fix: the existing engine is cloneable; its context is shared through `Arc`, and the blocking closure owns a clone rather than a borrowed lifetime extension. The generation mutex, KV clearing, cancellation/deadline checks, task labels and trace behavior are unchanged. No second model or context is allocated by cloning.
- Regression: the real-model test now reports one passed assertion set. Its strengthened branch also drops the external engine while the context is held, then cancels and drains the worker, verifies ownership release, and observes the final context owner disappear. On a regressed borrowed path it retains the external engine through drain so the negative test remains safe. Cancellation happens before decoding; no owner capture/store is used and trace output goes to a temporary directory.
- Process limitation: both red and green invocations subsequently hit the known Metal teardown abort (Cargo exits 101 reporting SIGABRT). A passing test assertion is **not** a clean model-process exit or proof of weight reclamation. Logs: `owned-inference-red.log`, `owned-inference-green.log`, and `owned-inference-drop-green.log` under `/tmp/fndr-integration-oct5/`. An intervening edit command initially used the wrong working directory and made no changes; the corrected absolute-path edit produced the green runs.
- Removed temporary instrumentation: none added. The worker's misleading idle-unload comment/log now says the cached runtime remains resident.
- Remaining risk: deliberate text/pixel model leaks, pixel-runtime ownership, actual idle release and the Metal shutdown assertion remain separate. A dropped waiter without an explicit cancellation control still lets an already running completion finish, now with valid owned state.

ANTI-BLOAT REVIEW
- Delivered behavior: running text completions retain valid state across waiter cancellation and external engine release.
- Reused: existing engine, context mutex, blocking executor, cancellation flag and real-model tests. Removed the transmute and its obsolete lifetime explanation; no service, dependency or scheduling layer added.
- Independent read-only review approved lifetime, task-label capture, cancellation and drop ordering with no actionable findings. The real-model regression is ignored in asset-free CI and explicitly exercised locally; clean process teardown remains unproven.


Focused verification: `CARGO_BUILD_JOBS=1 cargo test --locked --lib inference::` passed **76 tests**, with three real-model tests intentionally ignored. Explicit `cargo test --locked --lib complete_writes_labeled_trace_lines_with_token_counts -- --ignored --nocapture` passed its assertions for real output, token counts and the `trace_smoke_test`, `query_expansion`, and `answer` task labels, then encountered the same documented Metal teardown abort. Logs: `owned-inference-focused.log` and `owned-inference-trace.log` in the scratch directory. No frontend changes or browser claims.

Next grounding slice should replace freely invented action wording with bounded model-selected references to host-numbered source text, resolve and retain the original wording through existing `raw_evidence`, and distinguish source statements from inferred suggestions. Exact quotes establish occurrence, not user ownership or pending status; downstream task-reference assembly must preserve that distinction. Numeric novelty guards may reject the observed unsupported row count but must not be described as general factual verification. Use the existing eight cases as development regressions with positive source-backed actions; preserve held-out gold cases and the 640-token extraction budget.


Final broad validation: `CARGO_BUILD_JOBS=1 cargo test --locked` passed **1,086 tests**, with 20 intentionally ignored, across 21 reported targets including doc tests. The extra ignored test is the explicitly exercised real-model ownership regression above. Existing compiler warnings remain. Full log: `owned-inference-full-rust.log`. The suite-regenerated storage-index report was saved locally as `owned-inference-storage-indexes.md` and the tracked historical report restored.

## Source-backed extraction and observed statements

GRILL WITH DOCS RESULT
- Problem: the eight-case combined-model probe parsed successfully while inventing intent, next steps and a spreadsheet row count. Confidence and lexical overlap did not protect downstream task assembly.
- Contract: Qwen selects bounded line references into the existing 4,000-character extraction input. Rust resolves the selected line plus adjacent context, preserves the original snapshot hash, and rejects invalid references, overlong quotes and quote windows reaching an incomplete input cutoff. Exact occurrence does not establish ownership, pending status, truth or permission to act.
- Storage: new text extractions keep canonical intent, next steps and todos empty; capture fusion, both merge orders, visual text fallback, normalization and review preserve that boundary. Original snapshots live in existing raw evidence, with at most four distinct snapshots. Unsupported current contracts cannot fall back to stale task fields or same-hash history. Legacy records and actual pixel inference are not migrated by this change.
- Retrieval/UI: shared source-statement mapping bounds observations to 12 complete quotes / 6,000 characters, retaining rank/current-snapshot selection priority and deduplicated memory citations. Evidence packs and Resume distinguish observations from generated context and pending work. Vault reads statements only when a memory is expanded, through current visibility/blocklist rules and consolidation aliases. Quotes remain outside embedding prose and lightweight card lists (ADR 007).
- Limits: generated descriptive context remains unverified. The numeric-literal novelty guard addresses the observed unsupported number; it does not prove entailment or correct arithmetic. Stored shape/hash checks are provenance handling, not cryptographic authenticity. No owner-vault rewrite, production embedding cutover or native QA is claimed.

TDD SUMMARY
- Initial `cargo test --locked --lib source_`: 14 passed / 12 failed at expected extraction, merge and downstream boundaries; implementation then passed all 26. Follow-up visual fallback, unsupported-marker merge, storage roundtrip, normalization, review and aggregate-budget regressions observed 6 passed / 6 failed before their fixes; the next source run passed all 32.
- Independent review added cutoff-qualifier and same-hash unsupported-current regressions. The selected-memory read also began with an empty stub: the next run observed 33 passed / 3 failed at those exact boundaries. The visibility negative case already passed with the empty stub and must pass again with the real read.
- Frontend observation/loading/unavailable tests observed 5 passed / 3 failed before implementation; expanded-card and panel tests subsequently passed 19/19. Final integration results follow below.
- Logs are local under `/tmp/fndr-integration-oct5/`: `source-boundaries-{red,green}.log`, `source-storage-{red,green}.log`, `source-review-red.log`. Tests use synthetic records and temporary stores.

ANTI-BLOAT REVIEW
- Reused raw evidence, the existing capture validator, storage normalizer, review projection, EvidencePack/Resume, selected-memory lookup and expanded card. Lifted the existing visibility rule into one shared function rather than duplicating it. No schema, service, dependency or configuration switch added.
- One focused extraction-evidence module owns snapshot resolution/validation because inference, capture and downstream readers need the same contract. The new IPC read is on demand; quotes are not added to every list result. Removed the obsolete estimated prompt-budget test; the actual tokenizer boundary replaces that stale assumption.
- Offline/replay merges that explicitly disable vector recomputation still retain their existing vectors. Clearing canonical fields is not a historical re-embedding claim. Teammate ownership and batched human QA remain unchanged.

Independent UI review also identified an existing, separate retrieval-wide privacy gap: `drop_hidden_hits` currently excludes low-signal/internal rows but does not enforce current blocklist/soft-delete policy itself and retains unknown IDs after lookup failure. The legacy debug inspector has a similar direct-read boundary. The new selected read and aggregate EvidencePack will recheck visibility, but that does not certify all existing cards/debug traces. Follow up at the shared retrieval boundary with real-store excluded-row regressions; do not treat this as completed VS-61 coverage.

The first real Qwen prompt revision (`source_refs_v1`) was **not accepted**: 7/8 development cases parsed, the inbox hit the 640-token cap, and none retained source statements, including the positive chat/plan cases. Canonical intent/tasks were empty, but the sparse-page narrative still inferred intent and the spreadsheet's separate generated results claimed 42 rows. This is why those fields remain unverified. The terminal narrative was rejected by the numeric novelty guard. Median extraction was 12.446 s, maximum 19.890 s; there were no repair calls and one cap hit. A ninth, deliberately line-dense input tokenized to 15,405 tokens and was correctly refused before generation (0 output tokens), preserving instructions instead of trimming them. Measurements were saved before the known Metal teardown abort (exit 134). Artifacts: `qwen-source-refs/{result.json,summary.json,llm_traces.jsonl,stderr.log}`.

Revision `source_refs_v2` keeps the same 640-token output cap and host contract, adds a concrete positive line-selection example, caps narrative/list verbosity, and permits omitted unsupported fields. A second isolated run repeats the eight cases and overflow probe, plus a small explicit request/completed-negated-action diagnostic. Content tracing is enabled only for this synthetic probe process to inspect model compliance; no owner content is used. Results follow after completion.

The second prompt revision was also **not accepted**: 6/8 parsed, one cap hit, one unsuccessful repair, one chat quote pointing to a benchmark question, and no quote for the explicit-request diagnostic. The model also copied `L1`… source markers into `files_touched`. Revision 3 tests a compact core schema with plain numbered lines, avoiding fabricated session dates/fingerprints and redundant tags/aliases/statistics. The existing typed model defaults those omitted fields; their absence must not be presented as inferred data. This is a model-compliance experiment, not a reason to loosen host quote validation or increase the generation budget.

PROTOTYPE REPORT — selected source-reference contract
- Revision 3 parsed 8/8 cases with no repair/cap hits (median 6.341 s), but emitted paraphrases where integer references were requested. Its small positive case instead copied the exact numbered request. Revision 4 retains the compact schema and requests verbatim numbered source lines. Rust accepts an integer reference or one exact, unique raw/numbered source-line representation; incorrect prefixes, duplicate/ambiguous lines, paraphrases and substrings are rejected. Existing whole-quote/cutoff checks still apply. The new resolver regressions observed 37 pass / 2 expected failures, then all 39 source tests passed.
- Final revision `source_refs_v4`: **8/8 original cases parsed, 0 repairs, 0 cap hits**, median 7.886 s / maximum 13.732 s / total 68.063 s. Original-case prompt tokens were 436–1,873; output tokens 160–480. All parsed cases kept canonical intent/next_steps/todos empty. The additional explicit-request case retained both the exact approval-qualified request and the completed/negated review statement. The dense-input probe refused 13,307 prompt tokens before generation (0 output tokens). Source snapshots and qualifiers are preserved without promoting either observation to a pending task.
- Quality limits remain material: the larger chat/plan did not retain action quotes because the model emitted paraphrases/empty placeholders; the positive case's generated narrative falsely said the draft was approved. The host did not accept that claim as an action, but this does **not** establish factual correctness of summaries or good source-selection recall. Keep generated context unverified; next model-quality work should test constrained source selection and narrative contradiction handling, rather than further speculative prompt wording. The spreadsheet no longer claimed a row count, but its trend/quarter descriptions are still unverified.
- The final process saved results then hit the known Metal exit assertion (**134**). Peak RSS was 1,638,858,752 bytes and physical footprint 559,584,256 bytes. These isolated text runs do not establish native/whole-app memory acceptance. Each revision used the installed Qwen model already fingerprinted above, a fresh scratch profile, the existing output cap and no concurrent root build/model workload. Synthetic-only content tracing was enabled for revisions 2–4.
- Artifacts: `qwen-source-refs-v{2,3,4}/` under the scratch root, with final `result.json`, `summary.json`, `llm_traces.jsonl` and `stderr.log`. Disposable source retained as `source_refs_model_probe.rs`, SHA-256 `0322eb2366d936001c382f6511f9ea430be7f4ee796039f7d5c1ca1845c11bc3`; its checkout copy was removed. The existing ignored real-model test now includes positive quoted observations and oversized-prompt refusal, rather than adding a second permanent benchmark framework.

Additional verification: source/prompt budget focused runs passed **39 + 2 tests** across their final relevant revisions. The aggregate privacy regression first exposed all six excluded categories and then passed using the shared visibility rule. Frontend typecheck passed; 19 focused tests passed. Synthetic mounted Vault checks at 1280 and 360 px showed no horizontal overflow, preserved long quotes/qualifiers/citations, and rendered script-like source text without execution. Screenshots were retained locally as `browser/statements-{wide,narrow}.png`; no browser artifact is committed. Independent final contract review found no further actionable regression. Native capture and human usefulness remain untested.

Follow-up harness choice: installed `llama-cpp-2` provides JSON-schema-to-grammar conversion and a grammar sampler without a new dependency. Before introducing it, verify the existing sampler call sequence: the installed `sample()` implementation already accepts the selected token, while FNDR also calls `accept()`. Grammar would be advanced twice unless that path is corrected. This source-review finding is not a demonstrated quality improvement or a fix in this slice. Keep constrained selection and sampler behavior as one separately tested change, and use the explicit request plus larger chat as positive selection cases. The unused current-only evidence parser was removed; its storage test now reuses the shared snapshot parser.

Final broad validation: `CARGO_BUILD_JOBS=1 cargo test --locked --manifest-path src-tauri/Cargo.toml` passed **1,112 tests, 20 intentionally ignored**, across 21 targets including doc tests. After the unused-helper cleanup, `cargo test --locked --lib source_` again passed all 39. The strengthened existing real-model regression (`env -u FNDR_INFERENCE_N_CTX -u FNDR_INFERENCE_N_BATCH -u FNDR_INFERENCE_N_UBATCH CARGO_BUILD_JOBS=1 cargo test --locked --lib extraction_fits_default_token_budget -- --ignored --nocapture`) passed its entire assertion set in 71.58 s: eight parsed, uncapped cases; both exact positive quotes; empty canonical tasks/intent; oversized-prompt refusal with zero generated tokens and the correct prompt version. It then encountered the known Metal shutdown abort, so Cargo exited 101 after reporting one test passed. This is passing extraction assertions with abnormal process teardown, not a clean lifecycle gate.

Logs: `source-final-full-rust.log`, `source-final-cleanup-focused.log`, and `source-real-regression.log` in the scratch root. Existing compiler warnings remain. The suite-generated historical storage-index report was saved as `source-final-storage-indexes.md` and the tracked version restored. Final scope has no new dependency, schema migration, owner-data mutation, installed-model change, board mutation or teammate message. Native QA stays batched, and the broader five-outcome plan remains active.


## Shared retrieval and inspector visibility

DIAGNOSIS REPORT
- Observed failure: shared retrieval filtered only low-signal/internal rows after fusion, retained unknown IDs and failed-open on lookup errors, and left route/debug candidates untouched. Search rows copied stored related IDs without checking current policy. The debug inspector read seed details and graph/knowledge labels without authorizing their backing memories.
- Smallest repro: temporary-store tests observed a hidden row retained in the shared record map, a blocked inspector seed returning raw details, graph/knowledge `PRIVATE_` sentinels, and unfiltered deleted/missing/blocklisted related IDs. A later alias-budget regression observed 65 alias lookups when only 64 were allowed. Logs: `visibility-routes-red.log`, `visibility-inspector-red.log`, `visibility-links-red.log`, `visibility-inspector-alias-red.log` in `/tmp/fndr-integration-oct5/`.
- Root cause: independent projections treated candidate selection or stored references as read authorization. Fusion's cap ran before visibility, and the debug projection consumed the original route candidates.
- Fix: replace the post-fusion drop with one batched current-row authorization before fusion, reusing the existing visibility predicate. Missing/stale candidates and failed lookups fail closed. Preserve route order and scoring. Authorize nested links separately, retaining visible canonical survivors with bounded alias resolution. The inspector guards its seed, requires verifiable graph backing/edge endpoints and every knowledge-page support to be visible, and omits unverifiable derived content.
- Limits: filtering still follows each route's candidate cap, so visible results below a hidden-heavy route pool may be missed. The graph route remains unwired in production shared retrieval; this does not certify arbitrary graph paths. The inspector intentionally omits legacy session/entity/URL/project nodes lacking typed memory provenance; stored rows are unchanged. In-flight policy changes are not transactionally synchronized across all stages.
- Remaining boundaries: legacy `build_context_pack` activity/project/task/graph enrichment; MCP full-context related/timeline expansion, raw/source-evidence and knowledge reads; direct Vault/Resume listing; historical saved packs/audit/explanation reads. These can bypass shared retrieval and require separate source-aware authorization. This slice does not complete VS-61 or establish privacy across every human/agent endpoint.
- Temporary instrumentation: none added. Tests use synthetic records and temporary stores; no owner data or installed assets are changed.

ANTI-BLOAT REVIEW
- Behavior delivered: excluded records and unverifiable references are removed from the shared search/debug and inspector projections.
- Reused: existing visibility predicate, batched store reads, canonical alias lookup and current projection boundaries. Removed the fail-open post-fusion filter; no new schema, service, dependency or configuration switch.
- Complexity: small projection/provenance helpers and real-store boundary regressions. Private inspector extraction makes the actual command body testable without a native Tauri host. Alias work is bounded; legacy storage cleanup and broader policy architecture remain separate.
- Independent review found no additional must-fix in pre-fusion filtering; it identified the route-pool recall and legacy graph-coverage limits above and requested a public retrieval wiring regression.
- Verification results follow after the focused and broad gates complete.

Focused results: both pre-fusion/trace regressions passed. The public `retrieve_search_results` plus `run_query(ComposeMode::Cards)` regression passed, retaining a visible file citation/canonical linked survivor while excluding blocked/deleted IDs and payload sentinels. Nested-link authorization assertions and the 64-lookup cap passed; the first green run's final storage-invariance assertion used a pre-normalized fixture and failed because insertion trims links. The test now snapshots stored links before projection and compares afterward. No production correction was needed for that fixture issue. Full-suite results below cover the corrected assertion and final inspector alias cap.

Final validation: `CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/Users/anurupkumar/.cache/cargo-target-shared cargo test --locked --manifest-path src-tauri/Cargo.toml` passed **1,119 tests, 20 intentionally ignored**, across 21 targets including doc tests. All seven added regressions and the strengthened existing route regression pass. The separate public-boundary integration run passed before the full gate. Commands also included `cargo test --locked --lib route_visibility_`, `--lib search_rows_`, `--lib debug_inspector_`, and `--test retrieve public_retrieval_excludes_hidden_hits_and_nested_links_from_serialized_payloads`; red/green chronology is recorded above. Final log: `visibility-full-rust.log`; focused public log: `visibility-public-green.log`. Existing compiler warnings remain. The generated storage-index report was saved as `visibility-storage-indexes.md` and its tracked historical version restored. No frontend/native checks were rerun for this backend-only change.

Verdict: approve within the stated boundary. Next continue source-aware authorization through `build_context_pack` and legacy MCP enrichment/direct reads, then Vault/Resume and saved derived/audit context. Keep low-signal diagnostic views distinct from privacy exclusions. Preserve source/canonical alias semantics, use synthetic blocked-source sentinels, and avoid rewriting owner data or broadening the graph until provenance can be verified. Model-quality, production EmbeddingGemma migration and batched native QA remain separate gates; team assignments and communication are unchanged.


## Derived context, agent conversion and typed graph visibility

DIAGNOSIS REPORT
- Observed failure: after shared search filtering, `build_context_pack` could restore excluded activity, infer a project from it, reuse a provenance-free cached project summary, and append tasks backed by excluded memories. Agent conversion retained missing-row cached evidence, exposed excluded IDs in diagnostics and collected URLs from unselected evidence. The typed insight-graph snapshot exported unverified project labels and global edges/conflicts.
- Repro: temporary-store `context_visibility_` tests failed for mixed/private/missing/deleted event sources, cached project fields, tasks and hidden-only fallback. Both agent conversion regressions failed for serialized private sentinels and unselected evidence URLs. The public typed-graph snapshot test failed for mixed, absent, missing and excluded backing rows/edges. Logs: `context-visibility-red.log`, `context-agent-red.log`, `context-graph-red.log` under `/tmp/fndr-integration-oct5/`.
- Root cause: derived rows and their references were trusted as authorization. Search admission did not cover fallback reads, cached project aggregates, task relationships or later serialization.
- Fix: candidate authorization precedes event creation; every event's primary/source/evidence reference must resolve to an eligible current record, and historical private event/evidence classifications remain excluded. Bounded alias lookup retains old citations. Project read projections rebuild from admitted events and tasks rather than loading untraceable saved summaries. Fallback/project expansion deduplicates events. Latest-project inference now checks eligible events.
- Tasks: all memory links are required to pass, and existing source activity is checked regardless of project filtering. Explicit reference-free `Manual` tasks remain eligible only for unscoped collection. Orphaned generated and meeting-only tasks are omitted until their own source authority is available. Project matching recognizes canonical survivors and historical aliases. Linked blocked/sensitive URLs are omitted with the task.
- Agent conversion: reauthorizes sources before any ID-bearing diagnostic, removes the missing-record cached fallback, rejects private evidence and derives URLs only from admitted cards. `selected_memory_ids` retains its existing card/workflow/URL scope; it does not narrow verified project-level context. This is documented in the request type.
- Graph snapshot: nonempty complete source provenance is required for nodes; edges require authorized endpoints and explicit metadata references. Project selection also scopes edges/conflicts. Missing/malformed provenance fails closed. The legacy inspector reuses the lifted metadata parser, preserving its prior node-ID backing behavior.
- Review found a real unscoped-task gap after the first green run: a visible row with a historically private event still admitted its cached task. The new regression observed red (`context-tasks-red.log`), then the fix passed. No classifier heuristic was broadened; existing sensitivity and privacy-class rules are reused.
- Verification so far: seven initial core/agent checks passed; the strengthened set including existing inspector regressions passed 10/10 (`context-final-focused.log`), and the public graph regression passed (`context-graph-green.log`). The full gate below also covers added alias-budget and fallback-dedup regressions. One edit script had a Python syntax error before writing anything; the corrected command made the intended edits.

ANTI-BLOAT REVIEW
- Reused existing context assembly, project rebuilding, record visibility, batched reads, source aliases, task/event stores and graph projection. Deleted missing-source evidence fallback and duplicate graph metadata parsing. No schema, dependency, service, configuration switch or new documentation file.
- Historical project summaries, memories, tasks and graph rows remain stored. These changes authorize fresh read projections; they do not rewrite the owner's profile or implement deletion of historical derivatives.
- Remaining boundaries: saved pack/audit/explanation reads, direct Vault/Resume and legacy MCP raw/source/related/timeline/knowledge reads, and code/working-state endpoints that read activity independently still need checks. In-flight policy changes are not transactionally synchronized across all stages. Candidate/event caps can still limit recall; no new latency or whole-app resource claim is made.
- Keep manual/native QA batched. Production EmbeddingGemma activation, source-selection recall, generated-summary factuality, actual unload/Metal teardown, typed workflows and richer graph/timeline UX remain on the broader plan. No board mutation or teammate message.

Final verification: `CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=/Users/anurupkumar/.cache/cargo-target-shared cargo test --locked --manifest-path src-tauri/Cargo.toml` passed **1,128 tests, 20 intentionally ignored**, across 21 targets including doc tests. All nine added regressions passed, alongside existing retrieval/agent/inspector tests. Final log: `context-visibility-full-rust.log`. The suite-generated storage-index report was saved as `context-visibility-storage-indexes.md` and its tracked historical version restored. Existing warnings remain; no model workload or frontend/native test was run for this Rust-only slice.

Final review: no additional must-fix found inside fresh context-pack assembly. Filtering follows bounded event reads (6/8/18), so hidden newest events may crowd out older eligible context. Next direct boundaries are `build_code_context`, `build_context_delta` and `get_recent_working_state`, which still independently read activity, followed by remaining MCP/Vault/Resume and saved-context/audit reads. Preserve explicit manual tasks, citation/alias semantics and source-class policy; do not silently certify historical derivatives or treat selected-card filtering as full project scoping.
