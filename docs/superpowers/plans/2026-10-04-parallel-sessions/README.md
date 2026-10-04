# Parallel local and cloud sessions Implementation Plan

> **For agentic workers:** This plan coordinates two long-running Claude Code sessions. Each session reads this file, then its own kickoff (`LOCAL-KICKOFF.md` or `CLOUD-KICKOFF.md`). Per-ticket steps live in the tickets (`docs/team/tickets/*.md`: Why, Today, Do, Done when, Evidence). This plan does not repeat them; it decides who does what, in what order, through which bridge, and what to do when stuck. Use superpowers:subagent-driven-development for fan-out inside a session.

**Goal:** Take all 39 of Anurup's board tickets to `status::evidence` (or to an explicit, documented human blocker) by the Oct 16 freeze, and file the follow-up work the last three weeks of findings call for.

**Architecture:** Split by what each machine can verify. The **cloud** session (Linux container, GitHub only) builds everything that is pure logic, evaluation data, UI, research, and docs, and gets Rust verification from GitHub Actions (`macos-14`, `cargo test --locked`). The **local** session (M1 Mac, VPN, GitLab token) owns everything native or real-data, integrates the cloud's branches through the merge gate, and holds the board. GitHub branches `claude/*` are the bridge. Neither session edits the other's files.

**Tech Stack:** Tauri 2 + Rust (`src-tauri/`), React + TypeScript (`src/`), LanceDB, Python audit scripts, GitLab board via `scripts/team/gitlab_sync.py`, GitHub Actions CI (`.github/workflows/test.yml`).

**Spec:** `docs/team/2026-10-month-plan.md` (priorities, Beta targets, section 6 "how it works"), `docs/team/tickets/*.md` (the work), `docs/superpowers/plans/2026-09-30-memory-journey-claude-HANDOFF.md` (where the last session stopped), `docs/evidence/W02/VS-01-baseline.md` (the numbers to beat).

## Global Constraints

- No em dashes or en dashes in docs, code comments, tickets, commit messages, PR text, or chat.
- Commits are authored by the owner identity (`anurupkumar18`, `81anurup@gmail.com`). No `Co-Authored-By` trailer and no "Generated with Claude Code" line in commits, PRs, or tickets.
- Never commit real captures, `.fndrjourney.zip` bundles, database files, tokens, or anything from `~/Library/Application Support/com.fndr.app*`. The GitHub repo is **public**: everything the cloud pushes is world-readable. The cloud works on synthetic corpora and aggregate numbers only.
- The GitLab token never leaves the owner's Mac. Never paste it into a prompt, a file, a PR, or a ticket.
- Do not tune OCR, extraction, embeddings, or ranking from artifacts that did not complete the full pipeline (handoff rule). Search changes are judged by `make qa-retrieval`; Recall@5 may not drop more than 0.05 on any path.
- ADR-017 stays (MCP tools/call needs the bearer token). ADR-018 (cloud reasoning) is not accepted; no captured text goes to any cloud model.
- Local: never create git worktrees automatically (disk). Reference machine is an M1 with 8 GB: `CARGO_BUILD_JOBS=1`, keep native runs short.
- Anti-bloat gate (AGENTS.md): delete, reuse, or tighten before adding a layer.

---

## Part 1: Ground truth (verified 2026-10-04)

| Fact | Consequence |
|---|---|
| GitLab `origin/main` is `d6e7188`. GitHub `gh/main` is `248f7f3` (Sep 23), **93 commits behind**. | The cloud clones GitHub. Step 0 is publishing `main` to GitHub or the cloud works on a stale tree. |
| `origin` has two push URLs (GitLab and GitHub), so `git push origin main` updates both. | One command publishes. Local does it after each gated merge. |
| GitLab is reachable only on the university VPN with the token in `~/.zshrc`. The cloud has neither. | The cloud cannot move tickets, comment, or open MRs. It writes ticket notes; local posts them. |
| GitHub CI runs the full `cargo test --locked` on `macos-14` and frontend tests on Ubuntu, but only for `pull_request` into `main`. | The cloud opens **draft PRs** (never merged) purely to get CI. Local merges the branch into GitLab `main` itself. |
| Cloud is Linux: no Apple Vision OCR, Accessibility, ScreenCaptureKit, Touch ID, speech helper, or Tauri window. It has no real vault. | Native and real-data tickets are local-only. |
| The owner's board: 39 tickets, 2 in `evidence` (VS-01, VS-27), 3 `doing` (PX-07, GS-15, GS-17), 34 `ready`. 163 nominal hours, 91 of them p0. | Assumes agent help already. |
| `gitlab_sync.py` knows statuses `ready, doing, evidence, closed` only. | "Needs human" is a comment prefix, not a column, until a ticket adds the status. |
| `make qa-retrieval-check` does not exist yet (VS-04 creates it). Baseline: Search Recall@5 0.955, Ask 1.000, 17 of 22 top-1 agreement. Owner vault: median 129 chars, structured fields 0%, chunk table 0 rows. | The first cloud train builds the gate; until it lands, compare against `docs/evidence/W02/retrieval-baseline-seeded.json` by hand. |
| EM-03 (chunk every new memory, Minh) is `ready`, not started. VS-18 depends on it. | Cross-lane blocker. Decision D1 below. |
| Case 1 of the Memory Journey is blocked at dedupe: frames seen during the 8 s handoff seed `PerceptualHasher`, so the armed target reads as a duplicate. | First local fix (N1). Nothing downstream of VS-28 moves before it. |

## Part 2: The bridge

```
 cloud (GitHub only)                                local (Mac, VPN, token)
 -------------------                                ----------------------
 branch claude/train-*  --push-->  GitHub  <--fetch--  integrate/<train> branch
 draft PR for CI        <-- CI result on macos-14 --       |
 claude/cloud-outbox    --push-->  GitHub  <--read---      | make test, qa-retrieval-check,
   CLOUD-OUTBOX.md                                         | dash scan, secret scan
                                                           v
                                                    main --push origin--> GitLab + GitHub
 rebase on origin/main  <------------ GitHub main <-------+
                                                    local posts comments, moves tickets
 local/outbox <--read--  GitHub  <--push--  LOCAL-OUTBOX.md (directives, answers, decisions)
```

Rules:

1. **Cloud never merges and never pushes to `main`.** It pushes `claude/*` branches and opens draft PRs titled `[VS-04] short title` with the four-line body (Promise, How it actually works, What can go wrong, How we would know).
2. **One branch per train, one commit per ticket.** Commit subject starts with the ticket ID (`VS-05: ...`). Local can merge a whole train or cherry-pick one ticket. Trains are stacked in the order below.
3. **Cloud rebases on `origin/main` before each ticket** and whenever `LOCAL-OUTBOX.md` says main moved.
4. **Outboxes.** `CLOUD-OUTBOX.md` (on branch `claude/cloud-outbox`, that file only) and `LOCAL-OUTBOX.md` (on branch `local/outbox`, that file only). Each has a `NEEDS HUMAN (open)` block at the top, then one entry per ticket: branch, commit, CI status, evidence file, comment text ready to post. Update after every ticket and at least every two hours.
5. **Ticket comments and moves are local's job.** Cloud writes the comment text in its outbox; local posts it with `gitlab_sync.py comment` and moves the ticket.
6. **New work.** Cloud proposes tickets in `docs/team/tickets/proposed/cloud-proposals.md` (same format as `docs/team/tickets/README.md`; the sync only reads the top-level directory, so this folder is inert). Local reviews, moves accepted ones into `docs/team/tickets/anurup-followups-2026-10.md`, runs `make gitlab-plan`, then `make gitlab-sync APPLY=1`. The owner pre-authorizes this filing for both sessions (it overrides the "never create tickets" line in `gitlab-agent-instructions.md` for this campaign only). Assignee is `anurupkumar` unless a teammate's lane owns it; never reassign existing tickets.
7. **File ownership.**
   - Cloud owns: `src-tauri/src/search/**`, retrieval parts of `src-tauri/src/context_runtime/**`, `src-tauri/examples/retrieval_qa.rs`, `scripts/demo/**`, `scripts/audit/**`, `src/domains/memory-vault/**`, `src/domains/ask/**`, `src/domains/search/**`, `docs/**` except tickets, evidence it authors.
   - Local owns: `src-tauri/src/{capture,accessibility,ocr,memory_journey.rs,speech.rs,voice,downloads.rs}/**`, `src-tauri/helpers/**`, `docs/team/tickets/*.md`, `docs/team/roster.json`, GitLab, anything that needs a macOS permission.
   - Shared, append-only: `Makefile` (add targets at the end, one block per ticket), `docs/evidence/**` (one new file per ticket, named `<ticket>-<author>.md`), `.github/workflows/**` (cloud may propose, local merges).
   - If a ticket needs a file the other side owns, write a contract note in your outbox and ask; do not edit it.
8. **Public-repo hygiene.** Local scans every incoming diff for absolute paths, tokens, and memory text before merging (`git diff main...<branch> | grep -nE '/Users/|ghp_|glpat|Bearer '` plus a read of new fixtures).

## Part 3: Who does what

Legend: **C** cloud, **L** local, **H** hybrid (cloud builds the harness and write-up, local runs it on the M1), **P** needs the owner personally (cloud or local prepares the packet).

| Ticket | Title (short) | Who | Why |
|---|---|---|---|
| VS-01 | Baseline | L | In evidence; local answers review comments |
| VS-02 | Office-PM persona and 20 queries | C | Pure data plus `retrieval_qa.rs` test |
| VS-03 | Time, app, negative queries | C | Pure data plus report rows |
| VS-04 | Retrieval report as merge gate | C | Python or Rust compare plus `make qa-retrieval-check`; local verifies it on the Mac |
| VS-05 | Remove word-overlap cutoff | C | One module, test-first |
| VS-06 | Keyword branch returns best, not first | C | Superseded by VS-07 if BM25 lands first; cloud decides and says so |
| VS-07 | BM25 full-text index | C | LanceDB FTS; CI proves it on macOS |
| VS-08 | Reciprocal rank fusion | C | Pure function plus property tests |
| VS-09 | One `retrieve` function | C | Architecture core; publishes the contract Kunj and Minh consume |
| VS-10 | Search screen uses `retrieve` | C | |
| VS-11 | Ask, Resume, MCP use `retrieve` | C | |
| VS-12 | "No good match" | C | |
| VS-13 | Time and app phrases to filters | C | Pure parser, table-driven tests |
| VS-14 | Why visual path with little text | L | Live capture session with metrics dump |
| VS-15 | Accessibility tree text first | L | macOS AX API, permission prompts |
| VS-16 | AX text in capture, OCR fallback | L | Capture pipeline; the biggest vault-quality lever |
| VS-17 | Choose embedding model | H | Cloud writes the bake-off harness and criteria sheet; local runs it on the M1 for latency and memory |
| VS-18 | Retrieve over chunks, roll up | C | Blocked on EM-03 (D1) |
| VS-19 | Cross-encoder reranker spike | H | Cloud builds the experiment; local measures latency |
| VS-20 | Fast at 10,000 memories | H | Cloud builds a synthetic 10k seeder and bench; local runs it on the reference machine |
| VS-21 | Same query, same results | C | |
| VS-22 | Show why each result matched | C | UI plus `retrieve` explanation field |
| VS-23 | Vault reads like your work | C | UI in `memory-vault`, no dependencies |
| VS-24 | Ask answers with cited sentences | C | Needs PD-01 decided for any cloud-model path; local-model path first |
| VS-25 | Delete retired search code | C | After VS-10 and VS-11 |
| VS-26 | README and walkthrough tell the truth | C | After VS-18 |
| VS-27 | One memory journey | L | In evidence; Case 1 fix is N1 |
| VS-28 | Six-journey baseline | L, P | Human approval of labels is the boundary |
| VS-29 | Journey checks as merge gates | L | After VS-28 |
| PX-07 | Activity traces follow real process boundaries | L | Native |
| GS-15 | Screen Guide full-screen and Retina | L | Native |
| GS-17 | Private diagnostic bundle | L | Native |
| PD-01 | ADR-018 opt-in cloud reasoning | C drafts, P decides | Cloud writes the ADR draft and option table; the owner accepts or edits |
| PD-02 | Positioning page and competitor teardown | C | Web research plus writing |
| PD-03 | Async rhythm and decision log | C | Docs |
| PD-04 | Beta story and what judges score | C | Docs from the month plan section 9 |
| PD-05 | Friday scoreboard | C | Script plus doc from evidence files |
| PD-17 | Team charter | C drafts, P approves | |
| PD-18 | Talk to two knowledge workers | P | Cloud writes the interview guide and note template; local relays |

Count: C 25 (including the PD-01 and PD-17 drafts), H 3, L 10, P-only 1 (PD-18).

### Decisions for the owner (default if you do not answer)

| # | Decision | Default |
|---|---|---|
| D1 | EM-03 gates VS-18 and is Minh's, still `ready`. Ask Minh to start it this week, or reassign it to the cloud session. | Cloud builds VS-18 against the existing chunk table schema behind a feature flag so it lands the day EM-03 does; local pings Minh on the ticket. |
| D2 | PD-01: accept opt-in cloud reasoning? | Draft only; local-model path everywhere until you accept. |
| D3 | Model for the cloud session (you spend the expiring credit). | Strongest available. |

## Part 4: Trains and tracks, in dependency order

### Cloud trains (start A, E and the VS-23 piece of C immediately, in parallel with sub-agents)

| Train | Branch | Tickets (in order) | Depends on |
|---|---|---|---|
| A: measure | `claude/train-a-measure` | VS-04, VS-02, VS-03 | VS-01 JSON (committed) |
| B: retrieval core | `claude/train-b-retrieval` (stacked on A) | VS-05, VS-07, VS-06, VS-08, VS-09, VS-13, VS-10, VS-11, VS-12, VS-21, VS-25 | VS-04 |
| C: vault and Ask UX | `claude/train-c-ux` | VS-23 now; VS-22, VS-24 after VS-18 | none, then train D |
| D: chunks | `claude/train-d-chunks` (stacked on B) | VS-18, VS-26, plus H harnesses for VS-17, VS-19, VS-20 | train B, EM-03 (D1) |
| E: docs and decisions | `claude/train-e-docs` | PD-02, PD-03, PD-04, PD-05, PD-17 draft, PD-01 draft, PD-18 guide | none |
| F: new work | `claude/train-f-new` | Items from Part 6 marked C | per item |

### Local tracks (L1 is a standing duty that interrupts the others for minutes, not hours)

| Track | Work | Notes |
|---|---|---|
| L1 integrate | For each cloud branch: merge gate (Part 5), push, post comments, move tickets | Highest leverage: unblocks the cloud's next stacked train |
| L2 finish what is doing | GS-15, GS-17, PX-07 to evidence | Already started; close them before opening new native work |
| L3 journey | N1 dedupe handoff fix, rerun Case 1, then Cases 2 to 6 with the owner (VS-28), then VS-29 | Native; some steps need the owner at the keyboard |
| L4 capture text | VS-14 spike, then VS-15, VS-16 | The deep, long-horizon native work; give it the longest uninterrupted stretches |
| L5 measure on the M1 | VS-17, VS-19, VS-20 using cloud harnesses | After VS-16 for VS-17 |
| L6 board and backlog | Reconcile findings against the 145 tickets, file what is missing, post relays, keep `LOCAL-OUTBOX.md` current | First 30 minutes of the session, then continuous |

Suggested local start order: L6 reconcile and file (30 min) -> publish main (Step 0, if not already) -> L2 -> N1 -> L4 spike -> then L1 whenever a cloud train lands -> L3 and L4 in the long stretches.

## Part 5: Merge gate (local, every incoming train)

1. `git fetch gh && git log --stat main..gh/claude/<branch>`; read the diff stat, new fixtures, and CI result on the draft PR.
2. `git switch -c integrate/<train> main && git merge --no-ff gh/claude/<branch>` (resolve conflicts here, never on `main`).
3. Run, and paste the output into the ticket comment:
   - `CARGO_BUILD_JOBS=1 make test`
   - For search, capture-text, chunking, or embedding changes: `make qa-retrieval-check` (once VS-04 has merged; before that `make qa-retrieval` and compare to `docs/evidence/W02/retrieval-baseline-seeded.json`)
   - `git diff --check main`
   - Dash scan: `git diff main --name-only | xargs grep -nP '[\x{2013}\x{2014}]'` must print nothing
   - Public-repo scan (Part 2 rule 8)
4. `git switch main && git merge --ff-only integrate/<train>` (or `--no-ff` for a train), then `git push origin main`.
5. Tell the cloud via `LOCAL-OUTBOX.md` that main moved; post each ticket's evidence comment; move to `evidence`.
6. If the gate fails: do not patch silently. Write the failure into `LOCAL-OUTBOX.md` for that ticket (command, output, suspected cause). The cloud fixes it on the branch. If the fix is under 10 lines and obviously right, local may make it and say so.

## Part 6: New work to consider filing (reconcile before filing)

The local session first greps the 145 existing tickets and `docs/**` for each item and files only what is genuinely missing, with evidence for why. Candidates, from the handoff, Phase 0 findings (`2026-09-23-user-first-qa-reset.md` Parts 1 and 1b), and the month plan:

| # | Candidate | Source | Who | Prio |
|---|---|---|---|---|
| N1 | Handoff frames must not seed dedupe history: tri-state arm API (`inactive`, `handoff_pending`, `started`), skip the ordinary tick while pending, regression test, dedupe evidence (threshold, match kind, distance) in the journey | Handoff next steps 1 to 3 | L | p0 |
| N2 | `memory_review` returns no parseable JSON and structured-memory JSON parse errors: find cause, tolerant parse or retry, add as a journey eval case | Handoff known issue 2 | L (may belong to Kunj's LM lane: ask) | p1 |
| N3 | 2.3 to 2.4 GB physical footprint in long QA: measure by component, set a budget, add a regression check | Handoff known issue 3, month plan target 700 MB | L | p1 |
| N4 | Graph route searches an empty in-memory graph and README calls the insight graph "Stable": drop it from fusion in `retrieve` until persisted, correct the README | Phase 0 Part 1 | C | p1 |
| N5 | Run `make qa-retrieval-check` in GitHub Actions so the gate is not manual (needs embedder cache) | VS-04 follow-up | C, local merges workflow | p1 |
| N6 | `fndr.remember` agent write-back: spec, provenance fields, redaction, rate limits, injected-note test corpus (month plan priority 6, not ticketed) | Month plan section 6 | C (spec and corpus) | p2 |
| N7 | `extract_task_candidates` has no production caller: connect to Resume next steps | Phase 0 Part 1 | C | p2 |
| N8 | Privacy Activity counters reset on quit and do not show MCP reads: persist and log reads (client, tool, bytes) | Phase 0 Part 1 flags, month plan "Trust" | L | p2 |
| N9 | Model weight diet (BGE downloaded and unused, 1.9 GB of models on 8 GB) decided from VS-17 results | Phase 0 flags | L | p2 |
| N10 | Make pure retrieval logic buildable and testable without macOS-only crates, if the cloud's Gate 0 shows Linux cannot compile the crate | Cloud Gate 0 | C | p2, conditional |
| N11 | Add a `needs-human` status to `gitlab_sync.py` so the board column the docs describe exists | Part 1 | L | p2 |

Ticket format and validation: `docs/team/tickets/README.md`; allowed labels are in `LABEL_COLORS` in `scripts/team/gitlab_sync.py`. Use the next free ID in a lane prefix (VS-30 onward for vault and search).

## Part 7: Autonomy charter (applies to both sessions)

You are trusted to run for a long time without asking. You own the **how**; the ticket and the guardrails own the **what**.

You may, without asking:
- Re-sequence within your lane when the dependency graph allows, split or merge tickets, and say why in your outbox.
- Push back on a ticket you believe is wrong, with evidence, and propose a better one.
- Delete code, and prefer deletion (anti-bloat gate).
- Fan out sub-agents for independent work (cloud: freely, including isolated copies; local: disjoint files only, no worktrees).
- Run experiments and spikes; report negative results, they count.
- Use web research (cloud especially) for PD-02 and the embedding and reranker comparisons.
- File follow-up tickets through the proposal path (Part 2 rule 6).

You must not:
- Touch another person's ticket, assignee, or labels other than `status::`. Teammates (Kunj, Minh, Felipe) merge to the same trunk; rebase often and never rewrite shared history.
- Enable auto-merge, force-push shared branches, or push the cloud's work to `main`.
- Spend money or call paid external APIs.
- Send any real memory text, capture, or bundle off the Mac.
- Fabricate numbers. Every metric in a ticket comment comes from a command whose output you pasted.
- Stop early to be tidy. If your lane is done, pull from the stretch list.

Strengths to lean on:
- **Cloud:** long autonomous builds, wide fan-out, writing large synthetic corpora, research and synthesis, refactors with CI as the feedback loop, property-based tests, and documentation that tells the truth.
- **Local:** native debugging with the real OS, judgement on real data without exporting it, integration, and the board. Use the longest uninterrupted stretches on VS-14, VS-15, VS-16.

## Part 8: When you are stuck

"Stuck" means you tried at least three distinct approaches and wrote down each one. Before that, keep trying.

1. **Do not stop the session.** Park the blocked item, switch to the next unblocked ticket, and keep going. End your turn only when everything remaining is blocked.
2. Add a block to the top of your outbox under `NEEDS HUMAN (open)`:
   ```
   ASK: one sentence of what you need from the owner
   WHY ONLY YOU: permission, credential, judgement, or hardware
   STEPS: exact commands or clicks, copy-pasteable
   TRIED: the three or more approaches and what each returned
   MEANWHILE: what you are doing instead
   ```
3. **Cloud:** if you end your turn, the first line of your message is `ACTION REQUIRED:` followed by the ASK lines. You cannot notify the owner any other way.
4. **Local:** use `PushNotification` with the ASK sentence when the owner may be away, and comment `NEEDS HUMAN: <ASK>` on the ticket (no tokens, no memory text).
5. Known, predictable asks (so the owner can batch them): accept or edit ADR-018 (PD-01); approve or correct the gold labels for the six journeys (VS-28); sit at the Mac for native journey cases that need foregrounded content; grant Screen Recording and Accessibility prompts for VS-15; nudge or reassign EM-03 (D1); two interviews (PD-18); VPN or token expiry (local stops and says so, no retry loops).

## Part 9: Cadence, finish line, stretch

- **Checkpoints.** Every ticket: commit and push (cloud) or update the board (local). Every two hours: refresh the outbox. When context is heavy: run the handoff workflow (`.agent-skills/portable-engineering/productivity/handoff/SKILL.md`) and continue from the written handoff.
- **Friday scoreboard.** PD-05 produces it; local posts it. Numbers come from `make qa-retrieval`, `make vault-health`, and the evidence files, never from memory.
- **Finish line:** Oct 16 freeze. All 39 tickets at `evidence` or carrying a `NEEDS HUMAN` or dependency note; Beta targets table in the month plan updated with measured values; final report in each outbox.
- **Stretch (pull when your lane is done, in this order):** Beta demo numbers (before and after Recall@5 chart data from the evidence JSON, month plan section 9, 4:10 beat); a third persona corpus; ablation write-up (BM25 only, vector only, fused, with and without chunks); anti-bloat review of `search/` and `context_runtime/`; property tests for fusion and the phrase parser; test-suite honesty audit (which tests protect user outcomes, which only fixtures); README truth pass.

## Part 10: What the owner does

1. Read this plan (10 minutes). Answer D1 to D3 or accept the defaults.
2. Publish: commit these three files on `main` and `git push origin main` (updates GitLab and GitHub; this is also the 93-commit catch-up).
3. Start the cloud session on `anurupkumar18/FNDR`, strongest model, network access that allows `github.com`, `crates.io`, `static.crates.io`, `registry.npmjs.org`, `pypi.org`, `huggingface.co`. Paste `CLOUD-KICKOFF.md`.
4. Start the local session (new chat, so it begins with clean context). Paste `LOCAL-KICKOFF.md`. Keep the Mac on power, awake (`caffeinate -dimsu` in a spare terminal), and on the VPN.
5. Check the two outboxes when convenient; act only on `NEEDS HUMAN (open)`.
