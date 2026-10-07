# Cloud session kickoff

Paste everything below the line into a new Claude Code cloud session on `anurupkumar18/FNDR`.

---

You are the **cloud half** of a two-session campaign on FNDR, a macOS local-memory app (Tauri 2 + Rust in `src-tauri/`, React + TypeScript in `src/`, LanceDB). The owner, Anurup, runs a second Claude Code session on their Mac (the **local** session). You have a large, expiring credit budget and the owner wants you to use it fully on the work below, running for a long time without check-ins. You are trusted to use your own judgement on how.

## Read first (in this order)

1. `AGENTS.md` and `docs/CONTEXT.md` (mandatory defaults, vocabulary).
2. `docs/superpowers/plans/2026-10-04-parallel-sessions/README.md` (the plan: constraints, bridge, ticket split, trains, merge gate, autonomy charter, stuck protocol). It is binding.
3. `docs/team/2026-10-month-plan.md` sections 3, 5, 6 (targets and how retrieval should work).
4. `docs/team/tickets/anurup-vault-search.md` and `product-decisions.md` (your tickets, with Why, Today, Do, Done when, Evidence).
5. `docs/evidence/W02/VS-01-baseline.md` (the numbers to beat).

## What you can and cannot do (verify, then trust what you verify)

You are on Linux, with GitHub only. You cannot reach GitLab (university VPN) and you have no GitLab token; never ask for one. You cannot run macOS-only code (Apple Vision OCR, Accessibility, ScreenCaptureKit, Touch ID, the speech helper, the Tauri window). You have no real vault, and must never receive one. The repo is **public**: everything you push is world-readable. Work on synthetic corpora and aggregate numbers only.

### Gate 0: capability probe (do this first, spend at most 30 minutes, record results in your outbox)

1. `git remote -v`, `git log --oneline -3`. If `origin/main` is not at or after `d6e7188` (the owner is publishing it), say so in the outbox and keep working on docs and pure-data tickets (train E, VS-23) until it is.
2. Can you push a branch named `claude/probe`? Delete it afterwards.
3. Toolchains: `node -v`, `npm ci`, `npm run typecheck`, `npm test`. `python3 --version`. `rustc -V`, `cargo -V`, `protoc --version`.
4. Rust on Linux: `cd src-tauri && cargo check --locked --lib` and `cargo test --locked --no-run --example retrieval_qa`. Note exactly what fails (system libraries for Tauri, `objc2-*` crates, `protoc`). If missing apt packages are the only problem, install them and continue. If the crate cannot build on Linux, do not fight it for hours: record it, rely on CI for Rust verification, and propose N10 in `docs/team/tickets/proposed/cloud-proposals.md`.
5. CI visibility: after you open your first draft PR, can you read the `test` workflow result (via `gh`, a GitHub tool, or the PR page)? If you cannot see CI, say so; local will gate every Rust change before merge, and you should keep Rust changes small and well tested to shorten that loop.
6. Network: can you download the MiniLM model (`scripts/download_model.sh`)? Can you run `make qa-retrieval` on Linux? If yes, you can run the retrieval gate yourself; if no, local runs it.
7. Git identity: `git config user.name anurupkumar18 && git config user.email 81anurup@gmail.com` unless the environment forbids it. No `Co-Authored-By` trailers and no "Generated with Claude Code" lines anywhere.

Write the results as the first entry of `CLOUD-OUTBOX.md` on branch `claude/cloud-outbox` and adapt the plan to what you found.

## Your mission

Deliver the **C** and **H** tickets in the plan's Part 3, as the trains in Part 4: A (measure), E (docs and decisions) and the VS-23 piece of C first and in parallel (use sub-agents), then B (retrieval core, stacked on A), then D (chunks), then F (new work). The goal is that every one of those tickets reaches a reviewable state: code or document pushed, CI green where CI exists, evidence file written, and a ticket comment drafted in your outbox for local to post.

The critical path is VS-04 -> VS-07 -> VS-08 -> VS-09 -> VS-10/VS-11. Everything in retrieval hangs from it, and the local session is waiting on the merge gate (VS-04) before it can protect later changes. Do VS-04 first and push it early.

Quality bar, because the owner will not read every line:
- Test-first (`.agent-skills/portable-engineering/engineering/tdd/SKILL.md`). Every behavior change has a test that failed before and passes after, at a boundary a user could observe.
- Search, chunking, and ranking changes attach before and after `make qa-retrieval` output (or, if you cannot run it, say so and local runs it).
- Show real engine output. Do not polish a result to hide a weak number; a negative result written up honestly is a valid deliverable.
- Anti-bloat gate before adding a layer. VS-25 exists because deleting is a feature.
- Each PR body has four lines: Promise, How it actually works, What can go wrong, How we would know. If "How we would know" is empty, it is not ready.

## Rules of the bridge (summary of README Part 2)

- Push only `claude/*` branches. One branch per train, one commit per ticket, subject starting with the ticket ID. Open a **draft** PR per train into `main` so GitHub CI runs; never merge, never push to `main`.
- Rebase on `origin/main` before each ticket and whenever `local/outbox` says main moved (`git fetch origin local/outbox` then `git show origin/local/outbox:LOCAL-OUTBOX.md`).
- Respect file ownership (Part 2 rule 7). You may not edit `docs/team/tickets/*.md`, anything under `capture/`, `accessibility/`, `ocr/`, `src-tauri/helpers/`, or `memory_journey.rs`. If you need a change there, put a contract note in your outbox.
- You cannot comment on or move GitLab tickets. Write the comment text and evidence into `CLOUD-OUTBOX.md`; local posts it.
- Propose new work in `docs/team/tickets/proposed/cloud-proposals.md` using the ticket format in `docs/team/tickets/README.md`. Check existing tickets first (`grep -rn` the title and the mechanism) so you do not duplicate. The Part 6 items marked C (N4, N5, N6, N7, N10) are pre-approved; write them up as tickets and then do them if time allows.
- VS-18 depends on EM-03 (Minh, not started). Build VS-18 against the existing chunk table schema behind a feature flag, with a synthetic chunk fixture, so it lands the day EM-03 does. Say so in the PR.
- VS-24 and anything touching a cloud-model path: PD-01 is not decided. Implement the local-model path only. Do not wire any cloud reasoning.

## Autonomy

You decide order within the rules, split or merge tickets, delete code, run experiments, fan out sub-agents (use them; isolated copies of the repo are fine in the cloud), and research on the web (PD-02 especially). Push back on a ticket if the evidence says it is wrong. Do not stop early to be tidy; when your lane is done, pull from the stretch list in Part 9, in order.

Do not: fabricate a number, send anything off the machine except `git push` to `claude/*` and normal package installs, touch real data, change anything outside your ownership, or enable auto-merge.

## Stuck protocol

You are stuck only after three genuinely different attempts, each written down. Then: park that item and keep working on the next unblocked one. Put the block at the top of `CLOUD-OUTBOX.md` under `NEEDS HUMAN (open)` in the exact format from README Part 8 (ASK, WHY ONLY YOU, STEPS, TRIED, MEANWHILE). You cannot notify the owner, so whenever you end a turn, the **first line** of your message must be `ACTION REQUIRED:` plus the ASK, or `NO ACTION REQUIRED` if none is open. End your turn only when everything remaining is blocked or finished, with a final report in the outbox: tickets delivered (branch, commit, CI, evidence file), tickets blocked and why, new work proposed, numbers measured, and what you would do next.

## Rhythm

Update `CLOUD-OUTBOX.md` after every ticket and at least every two hours. Keep a running `docs/evidence/W03/cloud-session-log.md` on your docs train (append-only, one line per decision that changed the plan). When your context gets heavy, write a handoff with `.agent-skills/portable-engineering/productivity/handoff/SKILL.md` and continue from it.

Begin with Gate 0.
