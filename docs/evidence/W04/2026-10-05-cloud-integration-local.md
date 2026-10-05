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
