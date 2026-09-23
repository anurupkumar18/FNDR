# Product, research, and decisions

We are building FNDR like a startup: code is half the work. The other half is deciding what to build, talking to the people it is for, writing down decisions so nobody works from an old one, and telling the story well enough that a panel of judges and interviewers remember it. We rarely meet, so every decision here ends as a written record in the repo that anyone can read later.

How decisions work: the ticket's assignee drafts, posts the draft link in the team chat, gives everyone two days to comment, then records the decision in `docs/team/decision-log.md` (created by PD-03) with the date, the options considered, and who agreed. Silence after two days counts as agreement.

## PD-01 Decide opt-in cloud reasoning (ADR-018)
- assignee: anurupkumar
- labels: area::product, type::decision, prio::p0
- milestone: W02-Measure
- estimate: 3h
- depends: PD-08

**Question.** Should FNDR let a person opt in to a cloud model (their own API key) for structuring memories, the command router, Ask answers, and "about this screen," while capture, OCR, storage, and embeddings stay on the Mac?

**Do.**
1. Draft `docs/decisions/018-reasoning-tier.md`: options (local only; opt-in cloud per task; cloud by default), what text or image leaves, how it is logged in Privacy Activity, key storage (macOS Keychain), and how we measure the quality difference.
2. Use Kunj's PD-08 table for which tasks are allowed.
3. Update the master plan's "strictly local" constraint to point at the ADR.

**Done when.** ADR accepted and the decision log updated by Mon Sep 28.

**Evidence.** The ADR.

## PD-02 One positioning page and a competitor teardown
- assignee: anurupkumar
- labels: area::product, type::research, prio::p0
- milestone: W02-Measure
- estimate: 4h
- depends: none

**Do.**
1. `docs/product/positioning.md`: who it is for, the problem in their words, FNDR in one sentence, three proof points we can show, and what we are not.
2. Teardown table: Screenpipe, Rewind and Limitless, Microsoft Recall, Raycast AI, ChatGPT and Claude desktop memory features. For each: what it does, price, local or cloud, where FNDR wins, where it loses. Mark vendor-blog claims as unverified.

**Done when.** Every teammate can repeat the one sentence; onboarding (OB-03) and slides use it.

**Evidence.** The page.

## PD-03 Our async operating rhythm and a decision log
- assignee: anurupkumar
- labels: area::product, type::decision, prio::p0
- milestone: W02-Measure
- estimate: 2h
- depends: none

**Do.**
1. Create `docs/team/decision-log.md` (date, decision, options, who agreed, link).
2. Write the weekly rhythm into `docs/team/TEAM.md`: Monday plan post (each person: this week's tickets), Wednesday written check-in on each Doing ticket (done, blocked, next), Friday scoreboard (retrieval, vault health, voice, reopen numbers) and a 3-minute recorded demo per person.
3. Response norm: reply in the team chat within one working day; blocked for more than a day means ping the lane owner and the lead.

**Done when.** Merged and the first Monday post is out.

**Evidence.** The first Monday post link.

## PD-04 The Beta story and what the judges score
- assignee: anurupkumar
- labels: area::product, type::decision, prio::p0
- milestone: W04-Prove
- estimate: 4h
- depends: PD-02

**Do.**
1. List what the panel and the course rubric reward; map each demo beat in `docs/team/2026-10-month-plan.md` section 9 to the evidence file that proves it.
2. Ten hard questions with short answers ("How is this different from Recall?", "What leaves the Mac?", "How do you know search is good?").
3. One dry run with someone outside the team.

**Done when.** Dry run under five minutes; answers in `docs/product/qa-prep.md`.

**Evidence.** The dry-run notes.

## PD-05 The Friday scoreboard
- assignee: anurupkumar
- labels: area::product, type::feature, prio::p1
- milestone: W03-Build
- estimate: 3h
- depends: VS-04, RE-13

**Do.** A script or Make target that prints one page every Friday: retrieval Recall@5 per persona and path, vault health (memories per active day, text length, structured share, chunk coverage, reopen share), voice latency, command success, and user-session results. Post it in the team chat.

**Done when.** Two Fridays posted.

**Evidence.** The two posts.

## PD-06 Decide what the command bar may do this semester
- assignee: rathodkunj
- labels: area::product, type::decision, prio::p0
- milestone: W02-Measure
- estimate: 2h
- depends: none

**Do.**
1. `docs/product/actions-policy.md`: the tool list for October, each tool's risk level (runs, one-tap confirm, never), and what never ships this semester (sending messages or email, deleting files or data, purchases, anything from screen text).
2. Record in the decision log.

**Done when.** Decision recorded; GS-01 builds on it.

**Evidence.** The policy.

## PD-07 Teardown of assistants that act on your Mac
- assignee: rathodkunj
- labels: area::product, type::research, prio::p1
- milestone: W02-Measure
- estimate: 3h
- depends: none

**Do.** Try Clicky, Raycast AI, Siri and Apple Intelligence, ChatGPT desktop ("work with apps"), and Claude desktop for the same five tasks (open a document, find something you saw, make a reminder, run a Shortcut, answer a question about the screen). Record time, success, and what felt good or bad. Write what we copy and what we will not do.

**Done when.** `docs/research/assistant-teardown.md` merged.

**Evidence.** The table.

## PD-08 Which tasks must stay local, and which may use a cloud model
- assignee: rathodkunj
- labels: area::product, type::decision, prio::p0
- milestone: W02-Measure
- estimate: 2h
- depends: LM-01

**Do.** A table of every model task (memory structuring, review, router, Ask, about this screen, daily brief, embeddings) with: local quality today, latency, memory, privacy sensitivity of the input, and a recommendation (local only, local default with cloud opt-in). Input to PD-01.

**Done when.** The table is linked from the ADR-018 draft.

**Evidence.** The table.

## PD-09 A one-page trust spec: what FNDR keeps and for how long
- assignee: minhpro001
- labels: area::product, type::decision, prio::p1
- milestone: W03-Build
- estimate: 3h
- depends: none

**Do.**
1. `docs/product/trust-spec.md`: every kind of data FNDR stores (memories, chunks and vectors, downloads' extracted text, the command journal, skills, traces, counters), where it lives on disk, default retention, how to delete it, and what is never stored (screenshots, blocklisted content, secrets).
2. Propose a default retention period and record the decision.
3. Check each claim against the code; file a bug for any mismatch.

**Done when.** Merged, with every claim linked to code.

**Evidence.** The spec.

## PD-10 Human-review the gold labels and decide what numbers we may claim
- assignee: minhpro001
- labels: area::product, type::decision, prio::p0
- milestone: W02-Measure
- estimate: 5h
- depends: none

**Today.** `src-tauri/tests/fixtures/gold/v0/` holds 50 extraction cases, 30 retrieval questions, and 20 guard cases labeled `claude-draft`.

**Do.**
1. Review each case; mark `human_reviewed`, fix, or reject; Felipe re-labels 10 blind and you record agreement.
2. `docs/product/claims-policy.md`: which numbers we may show publicly (only from human-reviewed sets, with sample size), how we round, and when we say "unverified."

**Done when.** Every case has a status; the policy is merged.

**Evidence.** Agreement figure and the policy.

## PD-11 Decide the canonical MCP tool list
- assignee: minhpro001
- labels: area::product, type::decision, prio::p1
- milestone: W03-Build
- estimate: 2h
- depends: none

**Today.** `docs/product/mcp-tool-audit.md` classifies 51 tools and proposes about 14 `fndr.*` tools.

**Do.** Confirm the final list, the deprecation period for old names, and which tools need the action risk policy (GS-11); record the decision and open implementation tickets for the removals.

**Done when.** Decision recorded with the follow-up tickets.

**Evidence.** The decision log entry.

## PD-12 Decide what reopen and downloads may store
- assignee: minhpro001
- labels: area::product, type::decision, prio::p1
- milestone: W02-Measure
- estimate: 2h
- depends: RE-01

**Do.** Decide and record: which URL parts are stripped (tokens, session ids), whether text-fragment anchors are stored, whether download source URLs are stored, how executable downloads are handled (RE-09), and how private or blocklisted sources are treated.

**Done when.** Decision recorded and linked from RE-05, RE-08, RE-09.

**Evidence.** The decision log entry.

## PD-13 Plan and run five user sessions
- assignee: u1442515
- labels: area::product, type::research, prio::p0
- milestone: W04-Prove
- estimate: 8h
- depends: PD-02

**Do.**
1. Recruit five knowledge workers outside the team (two students, two office workers, one project lead); a short consent note (seeded profile only, no personal screens recorded).
2. Tasks: find a known item, reopen it, resume a thread, run one voice command, and explain what FNDR stores. Time each; note where they hesitate; ask a five-question usability survey.
3. Run two sessions by Oct 9 and all five by Oct 16.

**Done when.** `docs/research/2026-10-sessions.md` with times, success, quotes, and the top five problems.

**Evidence.** The notes.

## PD-14 Define activation and the onboarding target
- assignee: u1442515
- labels: area::product, type::decision, prio::p1
- milestone: W03-Build
- estimate: 2h
- depends: OB-01

**Do.** Decide what counts as "activated" (for example: a first successful search within 10 minutes and one reopen in the first day) and the target share; record it and point OB-06 and OB-07 at it.

**Done when.** Recorded in the decision log.

**Evidence.** The entry.

## PD-15 Decide the five destinations and Labs
- assignee: u1442515
- labels: area::product, type::decision, prio::p0
- milestone: W02-Measure
- estimate: 2h
- depends: none

**Do.** Using the owner's QA scorecard (`docs/product/qa-walkthrough.md`) and UI-UX program decisions D-01, D-05, D-06, D-07: final sidebar, what moves to Labs, what is removed. Record the decision; PX-01 implements it.

**Done when.** Recorded by Wed Sep 30.

**Evidence.** The entry.

## PD-16 Decide how voice behaves
- assignee: u1442515
- labels: area::product, type::decision, prio::p0
- milestone: W02-Measure
- estimate: 2h
- depends: VO-01

**Do.** Decide and record: push-to-talk or tap-to-toggle; whether a final transcript runs immediately or waits for confirmation (UI-UX decision D-21 recommends confirmation first for searches; commands follow the action risk policy); which surfaces have a microphone (D-04); whether FNDR speaks answers by default.

**Done when.** Recorded; VO-02 builds on it.

**Evidence.** The entry.

## PD-17 Team charter: who decides what
- assignee: anurupkumar
- labels: area::product, type::decision, prio::p1
- milestone: W02-Measure
- estimate: 2h
- depends: PD-03

**Do.** One page in `docs/team/TEAM.md`: each lane's decision rights (what you may decide alone, what needs the lead, what needs everyone), how disagreements are settled (written options, 48 hours, lead decides), and how to hand off work when you are away.

**Done when.** Merged and acknowledged by each teammate in the chat.

**Evidence.** The acknowledgements.

## PD-18 Talk to two knowledge workers about how they get back into work
- assignee: anurupkumar
- labels: area::product, type::research, prio::p1
- milestone: W04-Prove
- estimate: 2h
- depends: none

**Do.** Two 20-minute conversations with people outside the team: how they find things they saw, how they resume after an interruption, which AI tools they use. No FNDR demo until the end. Notes in `docs/research/conversations.md` (no names).

**Done when.** Two entries.

**Evidence.** The entries.

## PD-19 Talk to two knowledge workers about getting things done on their Mac
- assignee: rathodkunj
- labels: area::product, type::research, prio::p1
- milestone: W04-Prove
- estimate: 2h
- depends: none

**Do.** Same format as PD-18, focused on the quick actions and automations they use (Shortcuts, Raycast, Siri, keyboard habits). Notes in `docs/research/conversations.md`.

**Done when.** Two entries.

**Evidence.** The entries.

## PD-20 Talk to two knowledge workers about finding old files and pages
- assignee: minhpro001
- labels: area::product, type::research, prio::p1
- milestone: W04-Prove
- estimate: 2h
- depends: none

**Do.** Same format as PD-18, focused on how they refind documents, downloads, and web pages (browser history, Spotlight, bookmarks, search in email). Notes in `docs/research/conversations.md`.

**Done when.** Two entries.

**Evidence.** The entries.

## PD-21 Talk to two knowledge workers about voice and first impressions
- assignee: u1442515
- labels: area::product, type::research, prio::p1
- milestone: W04-Prove
- estimate: 2h
- depends: none

**Do.** Same format as PD-18, focused on when they would speak to their computer and what would make them trust a new app with screen permission. Notes in `docs/research/conversations.md`.

**Done when.** Two entries.

**Evidence.** The entries.
