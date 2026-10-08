# FNDR Quality Lab

## Purpose

Quality Lab is the repeatable, local test environment for FNDR's capture-to-memory-to-retrieval experience. It runs against the same checkout and the same native Tauri app as `npm run tauri dev`, with `FNDR_DATA_DIR` redirected to a separate synthetic profile. It is not a browser prototype, second frontend, or separate app version.

The Lab combines three kinds of evidence without conflating them:

1. **Capture fixtures:** committed synthetic screen images exercise the production macOS OCR and text cleanup path. The same fixture command also passes all store and privacy-negative metadata cases through the deterministic pre-frame gate used by the native capture loop. Privacy-negative images are hashed for corpus integrity but are not decoded, OCR'd, or imported; this verifies admission decisions, not end-to-end live sampling.
2. **Synthetic memory profile:** one of the existing seeded corpora populates a separate local profile so the native Vault, Home/Resume, Search, Ask, and reopen presentation can be inspected. These records are explicitly marked as seeded examples; they did not run through live capture or extraction.
3. **Memory Journey:** the existing debug inspector records one explicitly armed live attempt, reconstructs persisted evidence, or imports a labeled positive image fixture through FNDR's existing image-import path. Fixture runs then execute paired exact/paraphrase Search and grounded/unsupported Ask checks against the stored record. This does not stand in for the capture loop.

## Current local workflow

Run from the repository root:

```sh
python3 scripts/quality_lab.py fixtures
python3 scripts/quality_lab.py prepare --suite knowledge-worker
python3 scripts/quality_lab.py score --suite knowledge-worker
python3 scripts/quality_lab.py compare \
  --before src-tauri/target/quality-lab/knowledge-worker/<baseline-run> \
  --after src-tauri/target/quality-lab/knowledge-worker/<candidate-run>
python3 scripts/quality_lab.py start --suite knowledge-worker
```

The suite may be `knowledge-worker`, `office-pm`, or `software-engineer`. `prepare` refuses to replace an existing profile. Pass `--reset` only when you want the matching Quality Lab suite moved to Trash and reseeded. `score` writes the current report below `src-tauri/target/quality-lab/`; the native app launched by `start` uses that exact suite profile. Quit the Lab app normally before scoring or launching another suite. The runner refuses to score/start while the native process or port 1420 dev server is active, avoiding concurrent access to one profile and duplicate Cargo builds.

`compare` accepts two successful retrieval-run directories. It requires matching suite, corpus, queries, frozen reference, and case keys, then shows Search/Ask/Retrieve deltas and every per-query rank change. It preserves each run, identifies source/contract fingerprint changes, and does not combine metrics into one quality score. A changed contract hash means reviewers should inspect both run manifests before attributing a delta.

The `fixtures` run manifest includes structured measurements alongside its raw log: CER by positive fixture, baseline-budget status, worst CER case, all admission decisions, motion-sequence dedupe outcomes, and test summaries. OCR CER budgets are historical regression ceilings, not quality targets; always inspect CER values and human-readable OCR/memory text. A passing fixture suite does not mean OCR or summaries are good.

The profile is stored at `~/Library/Application Support/com.fndr.app.quality-lab/<suite>`. The existing demo seeder verifies that this path cannot overlap the owner profile. It links the owner's model directory to avoid a second multi-gigabyte model copy. Because it is a symlink, do not install, update, or remove models from the Lab app; those operations could change the shared model files. Resetting a Lab profile moves only that suite data directory to Trash.

## Native synthetic image replay

With `quality_lab.py start` running, open Engine Diagnostics → Memory Journey. The **Synthetic fixture replay** control loads only manifest entries marked `store`; choose one and select **Import + run 4 checks** when a gold case is available. FNDR imports that committed image through the existing image-import pipeline and stores a new record in the active lab profile. The inspector opens a reconstructed view and sequentially runs the case's exact Search, paraphrase Search, grounded Ask, and unsupported Ask. The result shows target rank, citation, refusal, answer, and a 0–4 mechanical score. Exact/paraphrase pass when the target is in the top five, grounded passes when the target is cited, and unsupported passes when Ask refuses. `quality-cases.json` keeps stable case IDs, required facts, and query labels beside the image corpus. These checks do not decide whether a generated summary faithfully covers the required facts. Review that against the expected OCR text and persisted extraction/vector/storage evidence, then record the human usefulness and fact-coverage judgment separately.

**Run all gold cases** repeats the same production route serially for every fixture with a gold case. The batch continues after an individual fixture fails and reports its error, completed count, and per-case checks; successful journey bundles retain their normal evidence. This imports multiple new memories into the active suite profile and uses the existing six-bundle/128 MiB Memory Journey retention policy, so earlier journey bundles may age out. Use a freshly prepared/reset synthetic profile when you need an uncontaminated batch, and never run it against the owner vault. The batch view is an in-session summary; individual journeys and query evidence are persisted by Memory Journey. The suite-level `score`/`compare` commands remain the durable before/after retrieval reports and must run only after the native app is quit.

To measure suite-level before/after retrieval, record a baseline with the app stopped, start the app and replay/review the fixture, quit the app, then run `score` again and compare the two reports. The four case checks add durable Memory Journey query evidence; they do not alter the frozen suite scorer's corpus or query set.

This route is sequential and creates persistent synthetic records in the active suite profile. Save a retrieval score before and after replay if measuring its effect on the suite; the `compare` command requires unchanged query/corpus/reference files and reports both source/profile outcomes. Reset only the synthetic suite profile after quitting FNDR when a clean replay baseline is needed. The runner refuses the owner profile, and the native replay command repeats that check before each write.

The imported image command intentionally does not claim live screen capture: capture admission/privacy blocking and capture-loop deduplication are not exercised. Privacy-negative fixtures are not offered in the image importer. Live capture and existing-memory reconstruction remain separately labeled in Memory Journey.

## What each lane proves

| Lane | Production behavior exercised | Scoring/evidence | Does not prove |
|---|---|---|---|
| `fixtures` | Production pre-frame privacy gate for all store/negative metadata rows; OCR recognition and high-signal text cleanup on committed positive screen images; production `PerceptualHasher` decisions for synthetic motion sequences and positive fixture pixels | Expected admission-reason match (including no false blocks on store cases), per-image character error rate, dedupe sequence outcomes, and positive-frame A→B→A behavior; report saved under `target/quality-lab/capture-fixtures/` | Pixel/OCR-based negative admission, live screen sampling, capture-loop timing/orchestration, model extraction, embedding, storage, native UI, or actual screen-permission behavior |
| `prepare` + `score` | Store insertion, surfaceability, and production Search/Ask/Retrieve on a seeded persona corpus | Frozen reference comparison for Recall@5, MRR@10, top-1 agreement, p95, rank changes, and negative-query scores; report per suite | OCR, extraction quality, live capture stages, or human usefulness by itself |
| `start` | The current native UI reading the isolated profile | Human review of Memory Vault, Home/Resume, Search evidence/confidence, Ask citations/refusals, and source reopen | A replay of screenshot fixtures through capture or equivalence with release packaging |
| Memory Journey → Import synthetic fixture | One positive committed screen image through the production image-import command, OCR, configured extraction/fallback, embeddings, durable store insertion, persisted-record inspection, and four serial gold Search/Ask queries | Returned memory ID, persisted extraction/vector/storage fields, target rank, citation, refusal, answer, and human review against fixture OCR/fact references | Live screen sampling, capture admission, privacy-negative decisions, capture-loop dedupe, live-capture stage timings, or automated semantic approval of generated memory text; the imported record persists in the active Quality Lab suite until that profile is reset |
| Memory Journey | One manually initiated real capture attempt or reconstruction of one saved record; production Search/Ask | Observed stages/artifacts, timings, queries, and separate pipeline/usefulness/grounding scorecards | A reconstructed stage not persisted in the selected record; it is marked unavailable |

## Product scorecards

Keep these dimensions separate. A single blended number could make a serious privacy, grounding, or usability failure look acceptable.

### Pipeline integrity

- Admission: expected store/skip decisions, with zero privacy-protected fixture admissions.
- OCR: character error rate and exact preservation of distinctive facts; cleanup retention versus removed noise.
- Extraction: required-fact recall, unsupported-claim rate, schema/validator outcome, and whether the fallback remains useful when evidence is thin.
- Storage: exactly one insert/merge/skip outcome, surfaced versus Needs more signal, and embedding role/model/space/dimension/freshness.
- Resources: capture-to-search latency, peak app RSS, model load time, and storage used by the isolated profile (exclude shared model files from profile size).

### Human usefulness

- Can a person tell what work happened from the title, context, source, and timestamp?
- Does the record preserve a decision, outcome, next step, or limitation only when the source supports it?
- Are repeated narrator labels, raw bundle IDs, OCR noise, generic “screen shows” prose, or instruction-shaped text visible?
- Is the memory worth keeping; can it be found and reopened to the same useful context?
- Human usefulness labels remain human-reviewed. A model judge can flag examples but cannot approve its own gold labels.

### Search and agent grounding

- Exact and paraphrase Recall@5, MRR@10, nDCG@10, and top-1 stability across Search, Ask, and Retrieve.
- Every shown result has an intelligible confidence label and the source evidence that matched; plausible weak candidates remain visible without false certainty.
- Evidence/citation precision, required-fact coverage, unsupported-fact rate, and correct refusal for unsupported questions.
- Negative queries measure calibration separately from retrieval recall; no-result behavior is not a substitute for confidence and evidence.

## Before/after protocol

Every automated run records the Git commit, working-tree state and source fingerprint (including UI styles, integration tests, the runner, and dependency lockfiles), suite name, corpus/fixture SHA-256, relevant prompt/model configuration source hashes, platform, timestamp, and raw machine-readable metrics. Runtime model/tokenizer identity should be added from the actual loaded-model status before a run is treated as a model bakeoff. Compare the same case IDs, queries, profile seed time, and configuration before changing a prompt, tokenizer, embedding contract, admission rule, or ranker. Keep the previous report; never overwrite an accepted reference during exploration.

Report each measure as **improved**, **unchanged**, **regressed**, or **unmeasured** per case and path. Show the paired examples that caused a difference. Quality gates should be conjunctive: privacy and evidence-grounding invariants must hold; retrieval must not lose an approved relevant case; human usefulness must improve or be reviewed; resource cost must fit the target device. Faster latency cannot cancel a hallucination, and better average ranking cannot cancel a privacy failure.

For model comparisons, run candidates sequentially on the same labeled questions. Record active model/version, tokenizer hash, input prefix, vector-space ID, dimension, memory footprint, cold/warm latency, and index migration status. Keep MiniLM active until a candidate wins on reviewed retrieval quality, whole-app resource use, and a resumable/rollback-safe reindex. Do not mix vectors from separate model spaces or activate Gemma because dimensions happen to match.

## Next high-value build slices

1. **Capture-loop fixture replay:** extend the current import-path replay to run a committed screen fixture through live capture admission, frame dedupe, OCR/cleanup, extraction, embedding, storage, and presentation. It must write only to the Quality Lab profile, never the owner vault. Every stage says observed, skipped, failed, or unavailable; no inferred stage is presented as observed. Keep privacy-negative fixtures out of the import route because that route bypasses capture admission.
2. **Case manifest and gold review:** join image hashes, expected OCR text, must-mention/must-not facts, expected admission, exact/paraphrase/negative queries, grounded/unsupported questions, and reopen expectations by stable case ID. Keep draft labels out of blocking metrics until a human approves them.
3. **Native comparison view:** render last approved baseline versus the current run in the existing debug diagnostics area, with deltas, evidence snippets, and click-through to the same Vault/Search/Ask presentation. Do not make a web dashboard the source of truth.
4. **Golden run capture:** save compact, local JSON reports and a redacted summary suitable for code review. Raw screenshots/OCR and full journey bundles stay local and are never uploaded or committed.
5. **Budgeted soak:** add a serial, optional run that records cold/warm launch, peak resident memory, query latency, capture latency, and disk delta on the user's Mac. Keep heavyweight local models sequential to respect the 8 GB device; compare Gemma only after the retrieval set is labeled and MiniLM numbers are frozen.

## Safety and reliability

- The default and only profile namespace is `com.fndr.app.quality-lab`; never accept the owner profile, its parent, or a child as a Lab target.
- Fixture content is synthetic/public and versioned. Never import private screenshots or owner memories into a committed case set.
- Normal release builds contain no fixture replay action. Lab writes, deletion, export, and model management are visibly scoped to the synthetic profile.
- The seeder's stored records are not called “captured” in reports. Only a Memory Journey or fixture replay that actually traverses the production boundary can claim capture-stage evidence.
- No local model output is treated as trusted instructions. OCR, titles, fixture text, and model responses are evidence, not authority.
