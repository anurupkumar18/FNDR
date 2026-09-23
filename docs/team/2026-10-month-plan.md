# FNDR October plan: from demo to daily tool

**Status:** Draft v0.2, 2026-09-23. Becomes v1 after the owner's hands-on QA pass (Sep 24 to 27) and the Monday Sep 28 meeting. Covers Mon Sep 28 to Sun Oct 25; Beta is Wed Oct 21.

**Read with:** `docs/product/qa-walkthrough.md` (how each feature works today and how we score it), `docs/superpowers/plans/2026-09-23-user-first-qa-reset.md` (what the sweep found, Parts 1 and 1b), `docs/team/TEAM.md` (workflow, definition of done).

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

**Beta targets** (numbers come from `make vault-health`, `make qa-retrieval`, and the named logs; "today" is the owner's profile or the seeded profile on 2026-09-23):

| Measure | Today | Beta target |
|---|---|---|
| Search Recall@5 on the labeled query set | measured in Pass D | 0.90 or higher |
| Recall@5 on paraphrase queries only | measured in Pass D | 0.80 or higher |
| Search screen and Ask give the same top result | measured in Pass D | always (one retrieval path) |
| Search p95 latency at 10,000 memories | not measured | 500 ms or lower |
| Memories that reopen to the exact page or file | 10% | 90% of a live day |
| Median stored text per memory | 129 characters | 800 or more |
| Memories with a project and next steps | 0% | 60% or more |
| Memories with chunk-level vectors | 0 rows | every memory |
| Voice: first partial text, final text after release | none, several seconds | 0.5 s, 1 s |
| Voice commands done correctly on a 50-utterance script | not possible | 45 of 50 |
| Always-on CPU and memory, vision model not loaded | not measured | 3% or lower, 700 MB or lower |

## 4. What the hands-on pass found

To be filled from the scorecard and Pass D numbers on Sep 27.

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
6. **Assistants write back** (agent notes with provenance).
7. **Skills from what worked** (a first slice: save a successful command sequence as a skill, run it by name, share it with Claude Code).
8. Stretch: **Seen before** (a cited nudge when an error or document comes back).

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

**Today:**

1. Capture stores one row per memory in `memories_v4_minilm_384` (113 columns) with three 384-number MiniLM vectors computed from a short composed summary (median 129 characters of text on the owner's profile).
2. The chunk tables for the design in ADR-008 (`memory_chunks_v1_bge_1024`) are empty unless someone runs a manual reindex.
3. The Search screen runs `search/`: vector search plus a `LIKE '%word%'` scan, then a reranker that drops results sharing under 15% of the query's words. Ask and MCP run `context_runtime/`: a planner with vector, keyword, time, entity, and graph routes (the graph is empty), fused. Two paths, two rankings.

**Target by Beta:**

1. **One retrieval function** in `context_runtime` used by the Search screen, Ask, Resume, Quick Find, and every MCP tool.
2. **Real text in the index.** Each memory keeps its cleaned screen text (Accessibility text first, OCR as fallback), split into chunks of about 300 tokens, embedded at capture time into one chunk table. The memory row keeps its summary vector.
3. **Keywords plus meaning.** A LanceDB full-text (BM25) index over chunk text, title, and URL, plus vector search over chunks and summaries, combined with reciprocal rank fusion. No hard word-overlap cutoff.
4. **One embedding model chosen by measurement** on the M1 8 GB: MiniLM-L6 (today), bge-small-en-v1.5, EmbeddingGemma-300M, Qwen3-Embedding-0.6B. Criteria: Recall@5 on the labeled set, milliseconds per chunk, memory. Optionally a small cross-encoder reranker on the top 30, kept only if it wins within the latency budget.
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

An assistant calls `fndr.remember` with a note, decision, summary, or to-do. FNDR stores it as its own memory with `source_type = "agent"`, the client's name, the tool, and the time, applies the same secret redaction as screen text, embeds it, and shows a badge ("Added by Claude Code"). It is never merged into screen memories. Defense against false memories: provenance, badges, size and rate limits, and a test set of injected notes that must not change any other memory or tool policy. Suggested edits to existing memories wait until after Beta.

### Trust

Privacy gates run before any pixels; skipped frames are counted by reason. FNDR's own network requests, every MCP read (client, tool, bytes), and every cloud reasoning request (feature, bytes, host) are logged locally and shown in plain language in Privacy Activity. A live check proves a blocklisted site is absent from Search, Resume, and agent packs.

### Reasoning tier (ADR-018, decided in week 1)

Capture, OCR, storage, and embeddings stay on the Mac, always. Reasoning (structuring a memory, the intent router's function calling, Ask answers, "about this screen") runs on the local model by default; with the person's opt-in and their own API key, it can use a cloud model instead, sending only text that already passed the privacy gates and redaction (and, for "about this screen," the one image the person asked about), with every request logged. We publish the measured local versus cloud quality difference.

## 7. Lanes

Four lanes, one per person, each owning user-facing outcomes. Pairs are listed where deliverables cross lanes.

### Lane 1: Retrieval and memory engine (Anurup)

Mission: make search and Ask right every time, and make the engine behind commands and skills sound.

| Week | Deliverable | Done when |
|---|---|---|
| W1 | One retrieval path in `context_runtime` for Search, Ask, Resume, and MCP; word-overlap cutoff removed; LanceDB BM25 index plus reciprocal rank fusion | `make qa-retrieval`: the two paths agree on every query; paraphrase Recall@5 up; no keyword row gets worse |
| W1 to W2 | Chunk-level index built at capture time from the full cleaned text; embedding model chosen by the Lane 3 bake-off; one migration | Chunk rows for every new memory in `make vault-health`; ADR recording the choice with the numbers |
| W2 | ADR-018 and the reasoning tier for memory structuring and Ask | Project and next-step fill on a live day, before and after, from `make vault-health`; zero cloud requests when off |
| W2 to W3 | Intent router (grammar first, then function calling constrained to the tool schemas) and the action journal | 50-utterance command script: 45 correct; injected screen text never produces a tool call |
| W3 | Skills first slice: save as skill, approve, run by name, optional export for Claude Code | A recorded run of "save that as my Monday setup," then running it by name |

Pairs with: Lane 2 on tools and capture text, Lane 3 on evals, Lane 4 on command and skill UX.

### Lane 2: Mac native layer: capture text, reopen, voice, and actions (Kunj)

Mission: make FNDR see the right text, open the right thing, hear you quickly, and do things on the Mac safely.

| Week | Deliverable | Done when |
|---|---|---|
| W1 | Decide with the owner: merge the useful parts of the notch HUD branch or retire it, so no work lives outside `main` | Branch merged or closed; decision noted in the merge request |
| W1 | Reopen v1: page URLs from the Accessibility tree, `AXDocument` file paths, Preview page numbers, downloads with source URL and a link to the page memory | `make vault-health` on a live day: 90% of memories reopen to a page or file; a downloaded PDF opens from the Vault |
| W1 to W2 | Accessibility text first for knowledge-worker apps (Google Docs and Sheets in Chrome, Word, Pages, Keynote, PowerPoint, Preview, Slack, Notion), OCR as fallback | Median stored text 800 characters or more on a live day; before and after `make qa-retrieval` on a live-captured query set |
| W2 | Native streaming voice (SpeechAnalyzer on macOS 26, older on-device recognizer as fallback), stage events to the UI | Voice log: first partial text within 0.5 s, final within 1 s of release, 50 utterances |
| W2 to W3 | Tool executors (`open_app`, `open_url`, `open_memory_source`, `reveal_file`, `paste_text`, `run_shortcut`, `create_reminder`, `start_timer`) with the risk policy, and Quick Find back on as the command bar | Each tool has a test; a native recording of voice and typed commands doing real work |
| W3 | Download content indexing (PDF and document text chunked and embedded) | Find a downloaded PDF by a phrase that is only inside it |

Stretch: Seen before. Pairs with: Lane 1 on the router and chunking, Lane 4 on voice and command UX.

### Lane 3: Search quality, evaluation, and the agent contract (Minh)

Mission: be the team's source of truth on whether search is right, and make FNDR a memory any assistant can connect to in minutes and rely on.

| Week | Deliverable | Done when |
|---|---|---|
| W1 | Human labels: review the draft extraction and retrieval cases; grow the retrieval query set to 60 queries across two seeded personas (add an office-PM week) including time and app filters; Felipe re-labels 10 blind | Every case marked human-reviewed or rejected; agreement recorded; `make qa-retrieval` runs both personas |
| W1 | Agent access that survives relaunch: a persisted setting that starts the server on launch, and a Stop control | A Claude Code connection keeps working after relaunch with no clicks |
| W1 to W2 | MCP canonical surface from `docs/product/mcp-tool-audit.md`, with a contract test per canonical tool | Contract suite passes; `docs/mcp.md` lists only canonical tools |
| W2 | Embedding and reranker bake-off runner for Lane 1: Recall@5, MRR@10, ms per chunk, and memory for each candidate; search latency at 10,000 memories | One table in `docs/evidence/` anyone can regenerate with one command |
| W2 to W3 | `fndr.remember` (agent notes with provenance) with the injected-note tests, and a local log of agent reads | Notes found by Search and Resume; injected notes change nothing else; today's agent reads countable over IPC |
| W3 | Friday retrieval scoreboard, setup guides for Claude Code, Claude Desktop, and Cursor, and a README feature table that matches the code | Scoreboard posted every Friday; a teammate follows each guide cold in under 5 minutes |

Pairs with: Lane 1 on retrieval changes and write-back, Lane 4 on how numbers and agent activity read.

### Lane 4: Product experience and user research (Felipe)

Mission: make FNDR obvious to a first-time user, and bring back evidence from real people about whether it helps.

| Week | Deliverable | Done when |
|---|---|---|
| W1 | Home is Resume plus search; five destinations with Labs | Browser preview and native screenshots in light and dark |
| W1 to W2 | Search and Vault UX: why a result matched (words or meaning), source icons (page, document, download), "Open where I left off" as the main action, app and time filter chips | Component tests; five people find a known item without help in the research sessions |
| W2 | Voice and command UX: listening, partial text, understood intent, action preview, approval, undo; one design for Home, Quick Find, and voice | A recording of each state from Lane 2's events |
| W2 | Daily Brief merge; remove panels the verdicts cut | No dead navigation; tests updated |
| W1 to W3 | User research: five knowledge workers (two students, two office, one project lead); tasks: find a known item, reopen it, resume a thread, run one voice command | Times, correctness, and quotes in `docs/research/`; no participant screen data stored |
| W3 to W4 | Agent-note badge, Privacy Activity v2 (agent reads, cloud requests), Beta storyboard, slides, backup video, evidence packet | Two timed rehearsals; the video plays offline |

Pairs with: Lane 1 on skills UX, Lane 2 on Quick Find and voice, Lane 3 on research numbers.

## 8. Weekly gates

| Week | Dates | Theme | Friday demo must show |
|---|---|---|---|
| W1 | Sep 28 to Oct 4 | Right answers, right place | One retrieval path with BM25; first `make qa-retrieval` improvement; reopen v1 on a live day; labels done; ADR-018 decided; notch branch resolved; five-destination sidebar |
| W2 | Oct 5 to 11 | Real RAG, real voice | Chunk index live with the chosen embedder; Recall@5 on both personas; streaming voice with partial text; first tools running from Quick Find; two user sessions |
| W3 | Oct 12 to 18 | Act and prove it | Voice commands doing real work; skills first slice; agent notes; downloads found by content; five user sessions; freeze Fri Oct 16 |
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

| # | Decision | Recommendation | Default if not decided by Sep 28 |
|---|---|---|---|
| 1 | Opt-in cloud reasoning with the person's own key | Yes, for structuring, the intent router, Ask, and "about this screen" | Local only |
| 2 | Retrieval ownership | Lane 1 owns the one retrieval path; Lane 3 owns the numbers that judge it | As written |
| 3 | Notch HUD branch | Merge the useful parts in W1, then close it | Close it after W1 |
| 4 | Tools that change things | Paste, reminders, and Shortcuts with one-tap confirm; nothing that sends or deletes this month | As written |
| 5 | Agent write-back scope | Notes only this month; suggested edits after Beta | Notes only |
| 6 | Pull from external tools (Drive, Notion, Calendar) through MCP clients | Not this month | Not this month |
| 7 | Five destinations plus Labs | Yes, adjusted by the QA verdicts | Yes |
| 8 | Beta date | Wed Oct 21, confirm with instructors | Oct 21 |
