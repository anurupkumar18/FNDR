# FNDR October plan: from demo to daily tool

**Status:** Draft v0, 2026-09-23. Becomes v1 after the owner's hands-on QA pass (Sep 24 to 27) and the Monday Sep 28 meeting. Covers Mon Sep 28 to Sun Oct 25; Beta is Wed Oct 21.

**Read with:** `docs/product/qa-walkthrough.md` (how each feature works today and how we score it), `docs/superpowers/plans/2026-09-23-user-first-qa-reset.md` (what the sweep found), `docs/team/TEAM.md` (workflow, definition of done).

## 1. What we are building, in one sentence

FNDR is the shared work memory for you and your AI assistants on your Mac: it remembers what you were doing, hands you or any assistant a cited summary so you can pick up where you left off, and lets those assistants add what they learn back into the same memory, with everything that leaves your Mac visible to you.

## 2. Who it is for

Knowledge workers who do school, office, and project work on a MacBook, switch between many apps, get interrupted, and already use an AI assistant (ChatGPT, Claude, Cursor). We design for three people:

| Person | Typical day | What "resume" means for them |
|---|---|---|
| Student | Canvas, Google Docs, PDFs, a code editor, group chats | "Where was I on the essay, which source was I quoting, and what did the TA say about that error?" |
| Analyst or PM | Slack, Sheets, email, slides, many short meetings | "What did my manager ask for, what did I already clean, and what is left before Thursday?" |
| Project lead | Notion, Keynote, standups, an AI assistant for drafting | "Who owns what, what did we decide, and what do I send to whom tonight?" |

Out of scope this month: phone companion, general "life logging," teams sharing one memory.

## 3. The one metric

**Time to correctly resume.** After an interruption, how long until the person can say what they were doing, name the next step, and has the right document open, and was it correct? Measured three ways:

1. The Resume task in `docs/product/qa-walkthrough.md` (normal tools versus FNDR Home versus an assistant with FNDR).
2. The same task with five outside knowledge workers before Beta (section 7, product experience lane).
3. In the app: local-only counts of Resume opened, source opened, pack copied, and "Was this right?" answers. Nothing leaves the Mac.

Everything else (captures per hour, graph size, model traces) is a diagnostic, not a goal.

## 4. What the hands-on pass found

To be filled from the scorecard on Sep 27.

| Feature | Useful 1 to 5 | Trust 1 to 5 | Verdict | One sentence |
|---|---|---|---|---|
| Resume Work | | | | |
| Search | | | | |
| Ask FNDR | | | | |
| Memory Vault | | | | |
| Agent access | | | | |
| Privacy Activity | | | | |
| Daily Summary | | | | |
| To-dos | | | | |
| Stats | | | | |
| Wrapped | | | | |
| Screen Guide | | | | |
| Seen-before toasts | | | | |
| Engine diagnostics | | | | |

## 5. Product shape this month

**Five destinations** (replacing nine sidebar items): Home (Resume), Search and Ask, Memory Vault, Daily Brief, Trust and Settings. Everything else moves to a **Labs** group or is removed, based on section 4 verdicts. Proposed defaults if the pass does not say otherwise:

| Today | This month |
|---|---|
| Home greeting and search pill | Home is Resume Work, with search above it |
| Daily Summary, To-dos, Stats, Wrapped | Merged into Daily Brief; Wrapped becomes an export from it; Stats moves to Labs |
| Screen Guide | Labs unless it scores 4 or higher |
| Engine diagnostics | Developer mode |
| 13 compiled but unmounted panels | Removed unless a verdict keeps one |
| Quick Find (Alt+Space), switched off | Back on, with Resume inside it |
| Agents can read, cannot write | Agents can add notes and suggest updates, with provenance and review |

**Three capabilities carry the demo:**

1. **Resume** (Home, Quick Find, and any assistant over MCP).
2. **Seen before** (a quiet, cited nudge when an error or document you already dealt with comes back).
3. **Trust** (Privacy Activity shows what was skipped, what your assistants read, and any cloud requests).

## 6. How each capability works behind the scenes

Every lane writes its merge requests against these descriptions. If the code does something different, update this section in the same merge request. We do not describe a feature in a demo, slide, or README in a way this section does not support.

### Resume

1. Capture stores a memory with `project`, `topic`, `outcome`, `next_steps`, `decisions`, `errors`. Today the local model fills these only when memory pressure allows; otherwise a rule-based fallback fills fewer fields.
2. `resume_work(hours, budget)` groups memories by project, then session, then domain, then app, and keeps the newest evidence inside a token budget, each item tagged with its memory id.
3. Home, Quick Find, and MCP `memory.resume_work` all call that one function.

Where it fails: wrong or missing project (threads split or merge wrongly); raw values like `unknown` or `in_progress` in the state line; stale threads shown as active. How we know: human-labeled Resume cases in the gold set, the "Was this right?" answers, and the Resume task.

### Agent write-back (new)

1. An assistant calls `fndr.remember` with a note, decision, summary, or to-do. FNDR stores it as its own memory with `source_type = "agent"`, the client's name, the tool, and the time, runs the same secret redaction as screen text, and embeds it so Search and Resume can find it.
2. An assistant calls `fndr.suggest_update` on an existing memory. Nothing is overwritten; the suggestion waits in a review inbox in the Vault, where the person accepts or rejects it.
3. Agent-written items always show a badge ("Added by Claude Code, Oct 3") and are never merged into screen memories.

Where it fails: an assistant that read a malicious web page writes a false memory (memory poisoning). Defense: provenance, badges, review for edits, size and rate limits, and a test set of injected notes that must not change any other memory or tool policy.

### Seen before

1. On each new memory, FNDR normalizes error lines into signatures (paths, numbers, and addresses removed) and checks returning documents (same URL or file after two or more days away).
2. A match against an older memory in the same project or app produces one quiet nudge citing when and what happened ("You fixed this on Sep 19: activate the course venv"), at most once per signature per hour.
3. "Not the same" feedback is recorded and counted.

Where it fails: false or noisy nudges. Kill rule: any false nudge on the fixture corpus blocks release.

### Trust

1. Capture runs privacy gates before any pixels; skipped frames are counted by reason.
2. FNDR's own network requests are counted by host. New this month: each MCP call is logged locally (client name, tool, bytes returned), and each cloud reasoning request if that tier is on (feature, bytes sent, host, model).
3. Privacy Activity shows all of it in plain language, and a live check proves a blocklisted site is absent from Search, Resume, and agent packs.

### Reasoning tier (proposed in ADR-018, decided in week 1)

Capture, OCR, storage, and embeddings stay on the Mac, always. Reasoning (structuring a memory, summarizing a Resume thread, answering in Ask) runs on the local model by default. If the person opts in with their own API key, those three tasks can use a cloud model instead, sending only text that already passed the privacy gates and redaction, never pixels, and logging every request in Privacy Activity. We publish the measured quality difference between local and cloud on the human-labeled set, so the choice is made by data.

## 7. Lanes

Four lanes, one per person. Each lane owns user-facing outcomes, not just code. Pair when a deliverable crosses lanes; the pairs are listed.

### Lane 1: Memory engine and agent loop (Anurup)

Mission: make what FNDR stores correct enough to resume from, and make FNDR a memory assistants can write to safely.

| Week | Deliverable | Done when |
|---|---|---|
| W1 | ADR-018 reasoning tier (supersedes the strictly-local rule for reasoning only) | Accepted by the team at the Friday demo; master plan constraints updated |
| W1 to W2 | `fndr.remember` and `fndr.suggest_update` (section 6) | Rust tests: provenance present on every agent memory; agent notes found by Search and Resume; suggestions never overwrite; 10 injected-note fixtures change nothing else; limits enforced |
| W2 | Resume quality: human-readable state line, no raw status tokens, better thread grouping, agent notes inside threads, "Was this right?" answers stored through the existing retrieval feedback path | On the seeded profile every thread reads as a sentence; on the owner's real profile, wrong-thread rate recorded before and after |
| W2 to W3 | Reasoning tier implementation for the three tasks, off by default, key in macOS Keychain, every request logged | Toggle on and off works; with it off, zero cloud requests in Privacy Activity; with it on, each request appears with bytes and host |
| W3 | Demo loop: Claude Code resumes a thread over MCP, finishes the next step, writes a note back; the next Resume shows it | Recorded twice without edits |

Pairs with: Lane 3 on evals and MCP contract, Lane 4 on the Vault review inbox.

### Lane 2: Mac native layer and capture trust (Kunj)

Mission: make FNDR feel like part of macOS and make what it captures from knowledge-worker apps accurate and provably private.

| Week | Deliverable | Done when |
|---|---|---|
| W1 | Agent access lifecycle: a persisted "Agent access" setting that starts the server on launch when on, a Stop control, and the status visible in Settings | Relaunch keeps a Claude Code connection working with no clicks; off means no listening port |
| W1 to W2 | Quick Find back on (Alt+Space) with a Resume mode: pick a thread, paste its pack into the focused app | Native recording: from Google Docs, Alt+Space, pick thread, pack pasted in under 5 seconds |
| W2 | Accessibility text first for knowledge-worker apps (Google Docs and Sheets in Chrome, Word, Pages, Keynote, PowerPoint, Preview PDFs, Slack), OCR as fallback, with document path and page as the reopen target | Before and after text quality on fixtures and one live hour; "Open latest source" reopens the right document and page |
| W2 to W3 | Seen before v1 by upgrading the existing proactive loop (section 6) | Zero false nudges on the fixture corpus; the seeded pandas case cites the older fix; rate limit tested |
| W3 | Native privacy proof: blocklisted site absent from Search, Resume, and agent packs; permission revoke and re-grant recovery | Screen recording plus a short evidence note |

Pairs with: Lane 1 on Seen-before matching, Lane 4 on the Quick Find and nudge design.

### Lane 3: Quality, evaluation, and the agent contract (Minh)

Mission: be the team's source of truth on whether FNDR is right, and make FNDR a memory any assistant can connect to in minutes and rely on.

| Week | Deliverable | Done when |
|---|---|---|
| W1 | Human labels: review the 50 extraction and 30 retrieval draft cases, add 20 Resume cases from the seeded profiles; Felipe re-labels 10 blind for agreement | Every case marked `human_reviewed` or rejected; agreement figure recorded |
| W1 to W2 | MCP surface cleanup from `docs/product/mcp-tool-audit.md`: remove pure duplicates, keep old names as aliases for one release, about 14 canonical tools; a contract test that calls each canonical tool against a seeded store | `cargo test` contract suite passes; `docs/mcp.md` lists only canonical tools |
| W2 | Agent activity log: each MCP call recorded locally (client name from the MCP handshake, tool, bytes returned, time) plus local counts for Resume opened, source opened, pack copied | IPC returns today's counts; unit tests; nothing leaves the Mac |
| W2 to W3 | Eval runs: baseline for local extraction and Resume, then local versus cloud with Lane 1's tier; numbers with confidence intervals in `docs/evidence/` | One table the demo can show, produced by a command anyone can rerun |
| W3 | Tested setup guides for Claude Code, Claude Desktop, and Cursor; README feature table matches what the code does | A teammate follows each guide cold in under 5 minutes |

Pairs with: Lane 1 on write-back tests and evals, Lane 4 on how the activity log reads.

### Lane 4: Product experience and user research (Felipe)

Mission: make FNDR obvious to a first-time user, and bring back evidence from real people about whether it helps.

| Week | Deliverable | Done when |
|---|---|---|
| W1 | Home is Resume: design pass on the Phase 0 Resume list (hierarchy, empty and error states, source and copy actions), and the five-destination sidebar with Labs | Browser preview and native screenshots in light and dark; the sidebar matches section 5 |
| W1 to W2 | Vault: agent badge and provenance, review inbox for suggested updates (accept or reject) | Component tests; a recording of accepting and rejecting one suggestion |
| W2 | Daily Brief: one screen for the day's summary, follow-ups, and the useful parts of Stats; Wrapped as an export; remove unmounted panels the verdicts cut | Old panels gone from the build; tests updated; no dead navigation |
| W2 | Privacy Activity v2 in plain language: what was skipped, what assistants read today, any cloud requests | A non-team person can explain the screen back in one sentence |
| W1 to W3 | User research: recruit five knowledge workers (two students, two office, one project lead), run the Resume task and a short usability questionnaire on a seeded profile, three sessions before the freeze | Times, correctness, and quotes in `docs/research/`; no participant screen data stored |
| W3 to W4 | Beta storyboard, slides, backup video, evidence packet | Two timed rehearsals; the video plays offline |

Pairs with: Lane 1 on the review inbox, Lane 2 on Quick Find, Lane 3 on the research numbers.

## 8. Weekly gates

| Week | Dates | Theme | Friday demo must show |
|---|---|---|---|
| W1 | Sep 28 to Oct 4 | Usable core | Home Resume shows at least three real threads from the owner's own week; Claude Code writes a note that appears in the Vault with a badge; labels done; ADR-018 decided; five-destination sidebar |
| W2 | Oct 5 to 11 | Right, not just present | Local versus cloud eval table; Quick Find Resume; Seen-before passes the fixture rule; Privacy Activity shows assistant reads; two user sessions done |
| W3 | Oct 12 to 18 | Prove it | Five user sessions; the demo loop recorded; native privacy proof; freeze Fri Oct 16 |
| W4 | Oct 19 to 25 | Beta | Beta Wed Oct 21; retro Fri Oct 23; November plan drafted from what we learned |

## 9. Beta demo (5 minutes)

| Time | Beat | Proof |
|---|---|---|
| 0:00 | Interrupted knowledge work, and assistants that forget everything between chats | One slide |
| 0:40 | Monday morning: Home shows the essay, the assignment, and the internship readout, each with the next step and sources; open the exact document | Live, seeded profile |
| 1:40 | "Continue in Claude": Claude Code pulls the thread over MCP, does the next step, writes a note back; FNDR's Vault shows "Added by Claude Code" | Live |
| 2:40 | The same error comes back; FNDR cites the fix from last week | Live or recorded clip |
| 3:20 | Trust: a blocklisted site is absent everywhere; Privacy Activity shows exactly what Claude read today and any cloud requests | Live |
| 4:10 | Numbers: resume time with and without FNDR from five users; local versus cloud quality | Two charts |
| 4:45 | What is next | Slide |

## 10. How we work this month

- Every merge request describes its feature in four lines: **Promise, How it actually works, What can go wrong, How we would know.** If "How we would know" is empty, it is not ready.
- Show real output. Do not polish a screen to hide what the engine produced; fix the engine or say what is missing.
- A feature we cannot demo end to end is removed from the demo, not simulated.
- Privacy and data: never commit real captures, databases, or tokens; research sessions use seeded profiles.
- No em dashes in docs, tickets, or commit messages.
- Monday 30 minutes plan, Wednesday written check-in on your ticket, Friday demo and retro (section 8).

## 11. What this replaces in the master plan

| Master plan item | This month |
|---|---|
| FEA-01 Figma storyboard | Replaced by working screens and the Beta storyboard in Lane 4 |
| FEA-02, FEA-03 Resume Work | Continues: Phase 0 list on Home, then Lane 1 quality and Lane 4 design |
| FEA-04 Deja vu | Continues as Seen before in Lane 2 |
| FEA-05, CAP-07 Privacy Proof | Continues as Privacy Activity v2 (Lanes 2, 3, 4) |
| MOD-04, MOD-05, MOD-06 gold set and evals | Continues in Lane 3, with human labels first |
| MOD-08 model bake-off | Replaced by local versus cloud comparison under ADR-018 |
| MOD-15 Intelligence panel | Paused; eval numbers appear in the demo and the evidence packet instead |
| RET-02 MCP audit | Done; its migration is Lane 3 week 1 to 2 |
| CAP-06, S0 event-driven capture, DEC-01 to DEC-08 | Paused until after Beta unless a lane finishes early |
| Global constraint "strictly local models" | Proposed change in ADR-018 (reasoning only; capture and storage stay local) |

## 12. Decisions for the owner

| # | Decision | Recommendation | Default if not decided by Sep 28 |
|---|---|---|---|
| 1 | Opt-in cloud reasoning with the user's own key | Yes, for structuring, Resume summaries, and Ask only | Local only |
| 2 | Agent write-back scope | Notes and suggested updates with review; no deletes; no silent edits | Notes only |
| 3 | Pull from external tools (Drive, Notion, Calendar) through MCP clients | Not this month | Not this month |
| 4 | Five destinations plus Labs | Yes, adjusted by the QA verdicts | Yes |
| 5 | Meetings screen | Stays hidden this month | Hidden |
| 6 | Beta date | Wed Oct 21, confirm with instructors | Oct 21 |
