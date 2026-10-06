# FNDR October plan: from demo to daily tool

**Status:** Working revision, 2026-10-05. The owner is refining the broader plan through short Q&A rounds. October 16 is the teammate delivery checkpoint, not a scope ceiling for the owner's work; the owner expects roughly two additional months for implementation and refinement. The October 21 Beta remains a proposed milestone, not a newly confirmed deadline. Older measurements below are dated baselines, not current results.

**Read with:** `docs/product/qa-walkthrough.md` (how each feature works today and how we score it), `docs/superpowers/plans/2026-09-23-user-first-qa-reset.md` (what the sweep found, Parts 1 and 1b), `docs/team/TEAM.md` (workflow, definition of done).

### Owner direction, updated October 5

The full scope has five outcomes: useful screen-context extraction with local models; shared hybrid search for people and agents; agentic actions and deterministic workflows; easy setup and enjoyable sustained use; and connected memories that support graph traversal and timelines over long histories. VS-42 through VS-68 cover part of this scope and must be integrated with the existing team lanes.

**Decided in Q&A round 1:**

- Support the 8 GB Mac with adaptive local models, resource-aware scheduling and unloading. Cloud reasoning remains explicit opt-in; capture, storage and embeddings remain local. Model utilization is useful only when it improves context quality within the device budget.
- Deliver typed, testable actions and saved workflows first, then bounded UI automation where app APIs or deep links are insufficient. Initial restoration means returning to supported resources and positions with explicit outcomes and fallbacks. Arbitrary unsaved application state is not currently reconstructable.
- EmbeddingGemma remains the chosen text-model direction. VS-47 reference parity, VS-48 M1 measurements and ADR 019 acceptance precede the VS-49 migration coordinated with Minh's EM-09.
- Defer the combined 60-minute native QA session until implementation and automated integration are ready. Batch requests for human help; use earlier focused native probes only when needed to resolve an implementation decision. Build and test migration on seeded profiles first; VS-51's owner-profile migration still follows live capture validation.
- Use parallel agents for independent implementation or review with explicit ownership. Inspect cloud changes at integration boundaries without waiting for the whole cloud session. Reuse existing portable skills, test harnesses and ticket evidence; remove obsolete code and consolidate active docs within the touched slice. Preserve historical evidence.

**Decided in Q&A round 2:**

| Decision | Accepted direction | Sequence |
|---|---|---|
| First connected-memory experience | Cited related work and a timeline | Build a richer graph explorer next, using the same data |
| Everyday entry point | Resume-focused Home with a prominent search/command box | Keep Vault and Connections one step away |

**Source-review findings that constrain implementation:** source provenance already exists in `raw_evidence.source_kind`; duplicate frames already exit before Accessibility reads; the ONNX export's pooled output must be inspected before adding dense-layer code; and persisted insight graph data exists even though shared retrieval currently builds an empty graph. Reconcile VS-43, VS-45, VS-47 and VS-33 with those boundaries before implementation. Source inspection does not establish native correctness or performance.

**October 5 implementation checkpoint:** cloud security/retrieval/v6 correctness changes passed local integration with four review corrections. The pinned MiniLM assets pass all three synthetic retrieval sets; the installed tokenizer reproduces the earlier lost-query discrepancy and remains untouched pending deliberate migration. Empty web areas now fall back to OCR. Home surfaces cited recent work through the existing Resume boundary, with eligibility filtering and loading/retry states. Initial M1 measurements favor fp32 EmbeddingGemma at 256 dimensions for the next isolated prototype, with concurrent workload and distribution-term decisions still open. Follow-up long-chunk and individual-query results are recorded below. See `docs/evidence/W04/2026-10-05-cloud-integration-local.md` and ADR 019. Capture-source lineage/reporting and default-off agent-note integration now pass automated checks; local review closed merge, reopen, derived-context and formatting gaps and fixed a non-advancing long-token chunker. The inactive EmbeddingGemma path now applies role prefixes after chunking and retains fp32 parity. Repeated isolated measurements found roughly 26 ms median individual queries and 21 seconds per 104 long-document chunks; concurrent capture and migration remain next gates. These results do not complete native capture, shared model migration, typed restoration or persisted graph traversal.

The related-memory follow-up now resolves persisted `related_memory_ids` through the same Vault/MCP boundary after restart, including consolidation aliases and current exclusion rules. Stored links are identified separately from fallback similarity; assistant notes retain their author and only expose explicit references. This is a bounded one-hop relationship view, not completed graph traversal. Live board inspection still showed Minh's EM-09 migration Ready; preserve that assignment and prepare shared-model integration and acceptance evidence alongside it.

The next extraction slice replaces generated intent/tasks with exact observed source statements for new text extractions. Snapshot citations survive capture, merge, storage and review, and appear separately in Vault details and agent evidence. Descriptive summaries remain unverified; legacy records and actual pixel inference are not rewritten. Shared model migration and native usefulness still require their own checks. The follow-up shared retrieval boundary now filters current candidates before fusion/debug projection, authorizes nested links, and protects inspector graph/knowledge projections. Fresh context-pack assembly now checks all event/task sources, rebuilds project read projections, and validates graph/agent output. Remaining direct memory/activity reads and saved derived context are the next privacy slice. Filtering follows route candidate caps, and unverifiable legacy graph labels are omitted; see the dated evidence report for exact results and limits.

## 1. What we are building, in one sentence

FNDR is the work memory for your Mac: find anything you have seen by what it meant, reopen it exactly where you were, act on it with a quick command or your voice, and share that memory with your AI assistants, with everything that leaves your Mac visible to you.

## 2. Who it is for

Knowledge workers who do school, office, and project work on a MacBook, switch between many apps, get interrupted, and already use an AI assistant (ChatGPT, Claude, Cursor).

| Person | Typical day | What FNDR must do for them |
|---|---|---|
| Student | Canvas, Google Docs, PDFs, a code editor, group chats | "Find the passage I read about labor contracts and open the PDF on that page." |
| Analyst or PM | Slack, Sheets, email, slides, many short meetings | "What did my manager ask for, and open the sheet I was cleaning." |
| Project lead | Notion, Keynote, standups, an AI assistant for drafting | "Open the deck, remind me to send it to Marco at 9, and tell Claude where we are." |

Out of scope this month: phone companion, general life logging, teams sharing one memory.

## 3. What success means

**The one metric:** time to correctly recover prior work (find it, reopen it, know the next step), compared with the person's normal tools. Measured with the Resume task in the walkthrough, with five outside users before Beta, and with local-only in-app counts.

### Acceptance contract for the broader scope

These are proposed implementation and verification criteria derived from the owner's five outcomes. They do not claim completed behavior or human-approved quality labels. Both rounds of product direction are accepted; implementation details follow source inspection and the existing ticket contracts.

| Outcome | Observable acceptance | Existing boundary and evidence |
|---|---|---|
| Useful local context | A permitted public/synthetic workflow produces source-linked facts about the work, its changes and any evidenced next step; unsupported intent stays unknown. Deferred enrichment survives restart, interactive requests take priority, and resource use is measured on the 8 GB Mac. | `capture/`, `memory_embedding_document.rs`, inference worker and review validation; VS-27 to VS-29, VS-42 to VS-46, LM-01 to LM-10. Report pipeline integrity, factual usefulness and agent grounding separately. |
| Shared human/agent retrieval | Exact identifiers, paraphrases, time/app filters and long multi-chunk documents return the relevant source passages through the same retrieval boundary. Search, Ask and MCP agree for equivalent requests. Unsupported questions do not produce unsupported answers. | `context_runtime::retrieve`, `chunk_route`, VS-04, VS-18, VS-47 to VS-51 and EM-03/04/09. Run the production composer and hybrid path as well as model-reference parity; report exact-match and paraphrase results separately. |
| Actions and deterministic workflows | A captured resource reopens through one typed outcome contract. Saved sequences execute the registered steps, journal outcomes and stop on a failed precondition. Missing/moved files, app-only fallback and unsupported state are explicit. Later UI automation must verify its target and postcondition and stop if the expected state is absent. | Minh's RE-07/12, Kunj's GS-03/04/09/11/12 and SK-01 to SK-05, plus VS-67/68. Unit/fixture tests cover dispatch and failures; final native checks cover actual reopening and execution. |
| Easy setup and sustained use | A clean profile reaches required-model readiness, captures a deliberate public example, finds it and reopens it. Interrupted downloads, skipped/denied permissions, an empty vault and failed actions have a clear next step. Keyboard navigation and long-history loading remain usable. | Existing Onboarding, Home, Search and Vault surfaces; Felipe's OB-04 to OB-06 and PX work, plus VS-23. Browser checks establish UI behavior; batched native QA establishes permissions and first recovery. |
| Connected long-term memory | Persisted nodes and edges survive restart; a keyword/semantic seed finds a related memory through a bounded, cited path. People and agents can inspect the relationship and its timeline. Deleted or excluded memories cannot reappear through graph expansion. Histories with many memories have measured traversal cost. | Existing `graph_store`, `graph_index`, `graph_route`, graph IPC and MCP interfaces. Start with a restart/one-hop integration fixture, then expand to timeline and explorer journeys. Reconcile VS-33 before changing the graph route; no duplicate store. |

The shared data flow is permitted capture -> source-preserving memory and chunks -> versioned local embeddings and persisted relationships -> shared retrieval -> cited human/agent context -> explicitly requested action -> typed outcome and journal. Extracted screen content supplies evidence, never authority to execute actions. Graph links and model summaries must retain source provenance rather than turn inferred relationships into facts.

Cross-cutting acceptance: VS-58 to VS-61 cover egress, credentials, secret handling and MCP authentication. Migration must resume after interruption, preserve readable old data until validation, and remove stale vectors and links when a memory is deleted. Latency and memory claims require named hardware, workload and model configuration. Existing numeric targets below remain targets until measured; larger model utilization alone is not success.

**Beta targets** (numbers come from `make vault-health`, `make qa-retrieval`, and the named logs; "today" is the owner's profile or the seeded profile on 2026-09-23):

| Measure | Today | Beta target |
|---|---|---|
| Search Recall@5 on the labeled query set | measured in Pass D | 0.90 or higher |
| Recall@5 on paraphrase queries only | measured in Pass D | 0.80 or higher |
| Search screen and Ask give the same top result | measured in Pass D | always (one retrieval path) |
| Search p95 latency at 10,000 memories | not measured | 500 ms or lower |
| Memories that reopen to the exact page or file | 10% | 90% of a live day |
| Median stored text per memory | 129 characters | 800 or more |
| Source-backed action coverage | not measured | Retain explicit action statements with citations; assess recall on labeled cases, without requiring a task on passive captures |
| Memories with chunk-level vectors | 0 rows | every memory |
| Voice: first partial text, final text after release | none, several seconds | 0.5 s, 1 s |
| Voice commands done correctly on a 50-utterance script | not possible | 45 of 50 |
| Always-on CPU and memory, vision model not loaded | not measured | 3% or lower, 700 MB or lower |

## 4. What the hands-on pass found

Pending the batched native QA session. The originally scheduled September pass did not supply completed results here; do not infer a verdict from an empty row.

| Feature | Useful 1 to 5 | Trust 1 to 5 | Verdict | One sentence |
|---|---|---|---|---|
| Memory Vault | | | | |
| Search | | | | |
| Ask FNDR | | | | |
| Resume Work | | | | |
| Voice input | | | | |
| Screen Guide | | | | |
| Agent access | | | | |
| Privacy Activity | | | | |
| Daily Summary | | | | |
| To-dos | | | | |
| Stats | | | | |
| Wrapped | | | | |
| Seen-before toasts | | | | |
| Engine diagnostics | | | | |

## 5. Priorities this month, in order

1. **Vault and search that are right every time.** One retrieval path, real text in the index, chunk-level vectors, keyword plus meaning, measured every Friday.
2. **Reopen exactly.** Every memory opens the page, document (and page number), or downloaded file it came from.
3. **Voice that is fast and visible.** Streaming on-device transcription with partial text and clear stages.
4. **Quick actions by command or voice.** Open apps, documents, and memories; paste; run your Shortcuts; make a reminder. Approval where it matters, everything journaled.
5. **Resume Work** on top of the above (it is only as good as retrieval and stored fields).
6. **Assistants write back** (agent notes with provenance). VS-68 covers the first slice after the VS-35 specification and VS-61 authentication work.
7. **Skills from what worked** (a first slice: save a successful command sequence as a skill, run it by name, share it with Claude Code).
8. Stretch: **Seen before** (a cited nudge when an error or document comes back).
9. **Connected long-term memory.** Reuse persisted entities and relationships for bounded, cited traversal and timeline continuity. Deliver cited related work and a timeline first, then a richer graph explorer over the same data.

### Execution order and ownership

| Stage | Deliverable and dependencies | Work distribution |
|---|---|---|
| 1. Make changes measurable | Reuse Memory Journey and the retrieval gate; reconcile source-review findings with ticket premises. Start VS-62 to VS-65 where their dependencies allow. For VS-63, establish whether cross-platform misses are numerical near-ties before changing tolerance; preserve visibility of actual regressions. | Anurup owns retrieval evidence. Verify VS-40 before the Linux work in VS-64; VS-65 follows. Check cloud branch/commit state before claiming any offered slice. |
| 2. Improve capture and model correctness | Capture coverage/provenance (VS-42 to VS-46) and EmbeddingGemma reference parity (VS-47) can progress independently. Validate the existing ONNX export before implementing extra pooling or dense layers. Use synthetic/public multi-step workflows while human gold labels remain pending. | Anurup's capture lane; cloud-suitable VS-47; integrate Kunj's scheduling and extraction work rather than building another model harness. |
| 3. Unify text retrieval | VS-48 chooses the dimension from M1 evidence; VS-49 integrates with EM-09 and EM-03/04/05. Test resumable migration and production hybrid retrieval on seeded profiles. VS-50 retires models only after migration validation; VS-51 performs the owner-profile migration after native capture validation. | Minh retains the embedding lifecycle and versioned migration boundary; Anurup owns retrieval comparisons and integration. |
| 4. Connect the user journeys | Deliver persisted-graph retrieval with cited paths, typed reopen outcomes, saved workflows and onboarding-to-first-recovery. UI presentation follows round 2. Run privacy work alongside these changes, with authentication before agent write-back. | Reuse Minh's RE, Kunj's GS/SK/LM and Felipe's OB/PX/VO lanes. Anurup integrates graph/retrieval, VS-58 to VS-61 and VS-67/68. No teammate reassignment is implied. |
| 5. Add measured visual retrieval | VS-52 defines and tests its go/no-go bar. On go, VS-53 to VS-56 deliver the image index, route, tests and UI; VS-57 accounts for resource use. Verify that legacy image backfill has an available source before promising coverage; never assume old screenshots were retained. | Anurup owns the spike and native measurements; cloud-suitable route/tests/UI slices follow the go decision. Image search is independent of the single text-model contract. |
| 6. Accept and refine | Integrate team outputs, regenerate evidence charts (VS-66 after VS-63), run automated gates, then the combined native session and targeted follow-ups. Confirm capture before the real-vault migration and compare retrieval afterward. Use longer-term dogfood findings to refine all five outcomes. | Batch owner QA; keep automated, native and human-usefulness evidence distinct. October 16 remains a team checkpoint. |

Independent slices may overlap; these stages express dependencies, not six serial projects. Recheck branch/ownership state before each slice rather than treating this plan as a live board.

**Immediate local queue, updated October 5:** cloud integration, capture provenance, persisted related links, shared model sessions, bounded query scheduling and owned blocking inference are implemented. New text-extraction observations now retain source citations through storage/review and human/agent reads, with canonical intent/tasks suppressed and oversized extraction prompts rejected intact. The compact prompt parses the eight development fixtures, but larger-chat quote recall and generated-summary contradictions remain open. Next: close shared retrieval card/debug visibility gaps, then test constrained source selection and narrative quality with Kunj's LM-05 boundary, followed by pixel/whole-app measurements and safe unloading. Gemma remains inactive; preserve Minh's EM-09 and Kunj's lifecycle/scheduling ownership. Check live branches and board state at integration boundaries; no reassignment or board-status change is implied. Current results and limitations are in the one-page status and October 5 evidence file.

Architecture records should be amended with the implementing slice: ADRs 002/008/019 for the embedding contract and migration; ADR-018 for resource-aware local-first reasoning; and the command-surface contract for bounded UI automation. A graph decision should define persisted traversal, provenance, deletion and query limits using the existing stores. Do not create another framework or roadmap to hold these decisions.

Product shape:

| Today | This month |
|---|---|
| Nine sidebar items | Five destinations: Home (Resume and search), Search and Ask, Memory Vault, Daily Brief, Trust and Settings. Everything else in Labs or removed per section 4 |
| Screen Guide, read-only, local 2B vision model | Replaced as the main assistant by one command surface (Quick Find and voice). "About this screen" stays as one optional tool inside it, using the cloud vision model only if the person opts in |
| Quick Find (Alt+Space), switched off | Back on as the command bar: search, reopen, resume, or do |
| Hard-coded voice phrases | Intent router over typed tools |
| 13 compiled but unmounted panels | Removed unless a verdict keeps one |

## 6. How it works behind the scenes

Every merge request describes its feature against this section. If the code does something different, update this section in the same merge request. We never describe a feature in a demo, slide, or README in a way this section does not support.

### Vault and search

**What LanceDB is and why we use it.** LanceDB is a database that lives inside FNDR as files in `~/Library/Application Support/com.fndr.app/lancedb` (no server). Each table stores rows with ordinary columns (app, title, URL, time, text) and vector columns (lists of numbers that capture meaning). It answers "find the rows whose vectors are closest to this query's vector, where app is Slack and time is last week" in one call, and it also supports a full-text (BM25) keyword index. That combination, meaning plus keywords plus filters, local and file-based, is why it fits FNDR.

**Current source baseline, October 5 (`c124957`):**

1. The live text embedding contract is MiniLM 384 dimensions. Capture can use browser semantic text, Accessibility text or OCR and records source provenance; the combined native capture validation is pending. The earlier 129-character median is a historical owner-profile measurement.
2. BGE 1024-dimensional chunk retrieval exists behind a flag. End-to-end capture coverage and the migration to one text model remain work to verify, not established completion.
3. Search, Ask and the main MCP search surfaces share `context_runtime::retrieve`, with BM25, vector retrieval, deterministic fusion and time/app filters. Legacy/raw exceptions remain to audit. See `2026-10-05-status-and-next.md` for the previously recorded gate results; no fresh gate is implied here.
4. Insight graph storage and traversal exist, but `retrieve_fused` constructs empty graph inputs. Connecting persisted graph data to shared retrieval requires bounded expansion and tests for provenance, privacy filtering and restart behavior.

**Target by Beta:**

1. **One retrieval function** in `context_runtime` used by the Search screen, Ask, Resume, Quick Find, and every MCP tool.
2. **Real text in the index.** Each memory keeps its cleaned screen text (Accessibility text first, OCR as fallback), split into chunks of about 300 tokens, embedded at capture time into one chunk table. The memory row keeps its summary vector.
3. **Keywords plus meaning.** A LanceDB full-text (BM25) index over chunk text, title, and URL, plus vector search over chunks and summaries, combined with reciprocal rank fusion. No hard word-overlap cutoff.
4. **One text embedding model:** integrate EmbeddingGemma through VS-47 to VS-51, choosing its dimension with reference parity, retrieval quality, latency and memory measurements on the M1 8 GB. ADR 019 remains proposed until its acceptance gates pass. Image embeddings remain a separate modality. Keep a reranker only if measured gains justify its latency and memory cost.
5. **Query understanding** for time and app ("last Tuesday," "in Slack") as filters.
6. **Consistency guard:** `make qa-retrieval` runs before any retrieval change merges; Recall@5 may not drop more than 0.05 on any path.

Where it fails: thin or wrong text in (fix at capture), the wrong embedder (fix by measurement), stale chunks after a memory merge (re-chunk on merge), latency at scale (indexes, measured at 10,000 memories).

### Reopen

1. **Web pages:** the page URL from the browser's Accessibility tree (Chrome, Safari, Arc, Edge), plus a text-fragment anchor (`#:~:text=`) built from a distinctive visible sentence, so reopening scrolls to the passage in browsers that support it.
2. **Documents:** the focused window's `AXDocument` attribute gives the file path in Preview, Pages, Keynote, Word, TextEdit, and editors; Preview's page number comes from the window title. Deep links where apps offer them (Slack, Notion, VS Code).
3. **Downloads:** when a file lands in ~/Downloads, FNDR records its path, its source URL (`kMDItemWhereFroms`), and the memory of the page it came from; PDF text (PDFKit) and document text are chunked and embedded like screen text, so you can find a download by what is inside it.
4. **Before opening,** FNDR checks the file still exists; if it moved, it looks it up by name with Spotlight.

### Voice

1. Audio is captured natively (not in the web view) and streamed to Apple's on-device recognizer (SpeechAnalyzer on macOS 26; the older on-device recognizer as fallback). Partial text arrives while you speak. The Python Whisper helper leaves the default path.
2. The UI shows each stage: listening (with a level meter), hearing (partial text), understood (the intent), doing (the action card), done or needs approval.

### Quick actions (command bar and voice)

1. One pipeline for typed and spoken commands: text, then an **intent router** (a fixed grammar for common verbs first, then model function calling for the rest), then **typed tools**, then the **risk policy**, then the executor, then the result and a journal entry.
2. Tools this month: `open_app`, `open_url`, `open_memory_source`, `reveal_file`, `search`, `resume_thread`, `copy_pack`, `paste_text` into the focused app, `run_shortcut` (the person's own Apple Shortcuts), `create_reminder`, `start_timer`. "About this screen" is one more tool.
3. Risk policy: open and search run immediately; paste and create need one tap to confirm; nothing sends messages or deletes anything this month.
4. Screen, OCR, and web text are data, never instructions: they can never add a tool or an argument the person did not ask for.

### Skills from what worked

1. Every command writes a journal entry: what was asked, which tools ran with which arguments (sensitive values hashed), the result, the app context, and the person's feedback (worked, did not, undone).
2. "Save as skill" (or three identical successful runs) drafts a `SKILL.md`: a name, when to use it, the steps as tool calls, examples, and what failed before. It builds on `agent/skills.rs`.
3. After approval a skill becomes a named command ("do my Monday setup"), an example the router learns from, and, if the person chooses, a skill file Claude Code can load.
4. Journal feedback becomes evaluation cases and later training data. Automatic mining of routines waits until after Beta.

### Resume

Groups recent memories into threads and shows the state, next steps, and sources; Home, Quick Find, and MCP `memory.resume_work` call one function. Its quality depends on retrieval and on the stored project and next-step fields; the reasoning tier below is how those fields get filled reliably on 8 GB.

### Assistants write back

An assistant calls `fndr.remember` with a note, decision, summary, or to-do. FNDR stores it as its own memory with `source_type = "agent"`, the client's name, the tool, and the time, refuses content matched by the shared secret detector, embeds it, and shows a badge ("Added by Claude Code"). It is never merged into screen memories. Defense against false memories: provenance, badges, size and rate limits, and a test set of injected notes that must not change any other memory or tool policy. Suggested edits to existing memories wait until after Beta.

### Trust

Privacy gates run before any pixels; skipped frames are counted by reason. FNDR's own network requests, every MCP read (client, tool, bytes), and every cloud reasoning request (feature, bytes, host) are logged locally and shown in plain language in Privacy Activity. A live check proves a blocklisted site is absent from Search, Resume, and agent packs.

### Reasoning tier (ADR-018, decided in week 1)

Capture, OCR, storage, and embeddings stay on the Mac, always. Reasoning (structuring a memory, the intent router's function calling, Ask answers, "about this screen") runs on the local model by default; with the person's opt-in and their own API key, it can use a cloud model instead, sending only text that already passed the privacy gates and redaction (and, for "about this screen," the one image the person asked about), with every request logged. We publish the measured local versus cloud quality difference.

## 7. Lanes

Four lanes, one per person. The tickets are the source of truth (`docs/team/tickets/`, synced to the GitLab boards by `make gitlab-sync`); this section is the summary. Each lane also owns product and decision tickets in `product-decisions.md`.

| Lane | Owner | Mission | Week 1 to 3 highlights | Tickets |
|---|---|---|---|---|
| Vault and search | Anurup | Find anything by meaning or exact words, same answer everywhere | Baseline and merge gate; cutoff removed; BM25 plus rank fusion; one `retrieve` for every surface; Accessibility text in capture; chunk retrieval; embedding choice | `anurup-vault-search.md` (VS-01 to VS-26) |
| Reopen and embeddings | Minh | Every memory opens exactly where it came from; every memory has vectors and chunks automatically | Reopen QA matrix (41 cases); file paths and PDF pages; downloads openable and never executed; chunk at capture; automatic backfill; no zero vectors; embedding QA matrix | `minh-reopen-embeddings.md` (RE-01 to RE-14, EM-01 to EM-12) |
| Command surface, skills, local models | Kunj | FNDR does useful things by text or voice, learns skills from what worked, and gets real use out of local models | Command contract; notch HUD (merged Sep 23) reconciled with the command surface; tool registry and executors; grammar router; Quick Find as command bar; risk policy; journal and skills; model usage measured; enrichment policy wired; interactive before background | `kunj-command-skills-models.md` (GS-01 to GS-14, SK-01 to SK-07, LM-01 to LM-10) |
| Voice, onboarding, tests | Felipe | One fast, visible voice pipeline everywhere; onboarding that gets a new user to a first useful moment; tests that protect what users do | Voice baseline and contract; native speech helper; shared voice control on Home, Search, command bar; Touch ID fix; onboarding copy and permissions; five destinations; test audit and journey tests | `felipe-voice-onboarding-tests.md` (VO-01 to VO-13, OB-01 to OB-07, PX-01 to PX-06, QT-01 to QT-07) |

Historical load (nominal hours without an agent, from `make gitlab-plan` on 2026-09-23): p0 was 66 to 75 hours per person against about 60 hours of month capacity. These estimates describe the original team schedule, not a limit on the owner's broader scope. Sequence the owner's work by dependencies, verification and review capacity; retain each teammate's assignment unless explicitly changed.

Cross-lane contracts: `retrieve` (Anurup) is used by Kunj's `search` tool and Minh's reopen checks; `reopen_memory` (Minh) is used by Kunj's `open_memory_source`; `voice://state` (Felipe) feeds Kunj's router; the risk policy (Kunj) gates MCP reopen (Minh). Each contract is written down in the first ticket that defines it.

## 8. Weekly gates

Original October coordination milestones follow. October 16 is the teammate checkpoint; broader owner work continues beyond it. Native acceptance is batched after implementation and automated integration, with remaining findings tracked explicitly.

| Week | Dates | Theme | Friday demo must show |
|---|---|---|---|
| W1 | Sep 28 to Oct 4 | Right answers, right place | One retrieval path with BM25; first `make qa-retrieval` improvement; reopen v1 on a live day; labels done; ADR-018 decided; notch HUD (merged Sep 23) reconciled with the command surface; five-destination sidebar |
| W2 | Oct 5 to 11 | Real RAG, real voice | Chunk index live with the chosen embedder; Recall@5 on both personas; streaming voice with partial text; first tools running from Quick Find; two user sessions |
| W3 | Oct 12 to 18 | Act and prove it | Voice commands doing real work; skills first slice; agent notes; downloads found by content; five user sessions; teammate delivery checkpoint Fri Oct 16 |
| W4 | Oct 19 to 25 | Beta | Beta Wed Oct 21; retro Fri Oct 23; November plan from what we learned |

## 9. Beta demo (5 minutes)

| Time | Beat | Proof |
|---|---|---|
| 0:00 | Knowledge work is scattered across apps, and assistants forget everything between chats | One slide |
| 0:30 | Find by meaning: "that reading about labor contracts" returns the passage; one click opens the PDF on page 112; a downloaded file is found by a phrase inside it | Live |
| 1:30 | Voice: "open the essay draft and remind me to send the deck to Marco at 9"; the stages are visible; one tap approves the reminder | Live |
| 2:20 | Resume with an assistant: Claude Code pulls the thread over MCP, does the next step, writes a note back that appears in the Vault | Live |
| 3:10 | Skill: "save that as my Monday setup," then run it by name | Live |
| 3:40 | Trust: a blocklisted site is absent everywhere; Privacy Activity shows what Claude read and any cloud requests | Live |
| 4:10 | Numbers: Recall@5 before and after, voice latency, reopen rate, time to recover work with and without FNDR | Two charts |
| 4:45 | What is next | Slide |

## 10. How we work this month

- Every merge request describes its feature in four lines: **Promise, How it actually works, What can go wrong, How we would know.** If "How we would know" is empty, it is not ready.
- Any change to capture text, chunking, embeddings, or ranking attaches before and after `make qa-retrieval` output.
- Show real output. Do not polish a screen to hide what the engine produced; fix the engine or say what is missing.
- Everyone runs FNDR on their own Mac all day, every day, and logs one diary line per day. We cannot improve a vault nobody fills.
- A feature we cannot demo end to end is removed from the demo, not simulated.
- Never commit real captures, databases, or tokens; research sessions use seeded profiles.
- No em dashes in docs, tickets, or commit messages.
- Monday 30 minutes plan, Wednesday written check-in, Friday demo, scoreboard, and retro.

## 11. What this replaces in the master plan

| Master plan item | This month |
|---|---|
| FEA-01 Figma storyboard | Replaced by working screens and the Beta storyboard (Lane 4) |
| FEA-02, FEA-03 Resume Work | Continues, after retrieval (Lanes 1 and 4) |
| FEA-04 Deja vu | Stretch as Seen before (Lane 2) |
| FEA-05, CAP-07 Privacy Proof | Continues as Privacy Activity v2 (Lanes 3 and 4) |
| CAP-05 native context | Extended into Accessibility text first and reopen targets (Lane 2) |
| MEM-05, MEM-09, RET-01 embedding, ablation, ranking | Pulled forward to W1 and W2 as the top priority (Lanes 1 and 3) |
| MOD-04, MOD-05, MOD-06 gold set and evals | Continues in Lane 3, human labels first |
| MOD-08 model bake-off | Replaced by the local versus cloud comparison under ADR-018 |
| MOD-15 Intelligence panel | Paused; numbers appear in the demo and evidence packet |
| RET-02 MCP audit | Done; its migration is Lane 3 |
| E-F4 approve-then-act agent | Replaced by the typed tools, risk policy, and journal in section 6 |
| CAP-06, S0 event-driven capture, DEC-01 to DEC-08 | Paused until after Beta |
| Screen Guide as a separate product (ADR-014) | Becomes one optional tool inside the command surface; ADR-014 to be amended |
| Global constraint "strictly local models" | ADR-018 (reasoning only; capture, storage, and embeddings stay local) |

## 12. Decisions for the owner

| # | Decision | Recommendation / accepted direction | Fallback or status |
|---|---|---|---|
| 1 | Opt-in cloud reasoning with the person's own key | Decided 2026-10-05: adaptive local-first with 8 GB support; cloud reasoning explicitly optional | Local until opted in |
| 2 | Retrieval ownership | Decided 2026-09-23: Anurup owns retrieval and its evaluation; Minh owns embedding coverage and reopen | Done |
| 3 | Notch HUD branch | Merged into main on 2026-09-23 (350105c); GS-02 reconciles it with the command surface | Done |
| 4 | Tools that change things | Paste, reminders, and Shortcuts with one-tap confirm; nothing that sends or deletes this month | As written |
| 5 | Agent write-back scope | Notes only this month; suggested edits after Beta | Notes only |
| 6 | Pull from external tools (Drive, Notion, Calendar) through MCP clients | Not this month | Not this month |
| 7 | Five destinations plus Labs | Yes, adjusted by the QA verdicts | Yes |
| 8 | Beta date | Wed Oct 21, confirm with instructors | Oct 21 |
