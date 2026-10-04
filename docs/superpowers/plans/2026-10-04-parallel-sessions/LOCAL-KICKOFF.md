# Local session kickoff

Paste everything below the line into a new local Claude Code session in `/Users/anurupkumar/FNDR` (not a worktree).

---

You are the **local half** of a two-session campaign on FNDR. A **cloud** session (Linux, GitHub only, public repo, no VPN, no GitLab token) builds the pure-logic, evaluation, UI, research, and docs tickets and pushes `claude/*` branches. You own everything native or real-data, you integrate the cloud's work through a merge gate, and you hold the GitLab board. The owner, Anurup, has weekly credit to burn and wants you to run for a long time with few check-ins. You are trusted to use your own judgement on how.

## Read first (in this order)

1. `AGENTS.md`, `docs/CONTEXT.md`, `docs/team/gitlab-agent-instructions.md`.
2. `docs/superpowers/plans/2026-10-04-parallel-sessions/README.md`: the plan. It is binding. Part 2 (bridge), Part 3 (split), Part 5 (merge gate), Part 6 (new work), Parts 7 and 8 (autonomy and stuck protocol).
3. `docs/superpowers/plans/2026-09-30-memory-journey-claude-HANDOFF.md`: where the last session stopped (Case 1 blocked at dedupe).
4. `docs/team/2026-10-month-plan.md` (sections 3, 5, 6), and the tickets in `docs/team/tickets/anurup-vault-search.md` and `cross-cutting-reliability.md`.

Environment: macOS, M1 8 GB. `source ~/.zshrc` provides `GITLAB_TOKEN` (it does not persist between Bash calls; source it in each command that talks to GitLab). University VPN must be up. If a GitLab command fails (VPN down, token expired), stop and tell the owner; no retry loops. Use `CARGO_BUILD_JOBS=1`. Keep native QA runs short (footprint hit 2.3 GB last time). Never create worktrees. Commits are authored as the owner, with no `Co-Authored-By` trailer and no "Generated with Claude Code" line.

## Your mission

Deliver the **L** tickets, the local half of **H** tickets, and the standing integration duty in the plan's Part 4. The cloud's critical path is VS-04 -> VS-07 -> VS-08 -> VS-09 -> VS-10/VS-11; your job there is to keep the merge gate fast so the cloud is never waiting on you.

### Step 0 (first 30 minutes)

1. Make sure a `gh` remote exists (`git remote add gh git@github.com:anurupkumar18/FNDR.git` if `git remote -v` does not list it) and `git fetch origin gh`. The plan files are already on `main` in both GitLab and GitHub. Teammates push to GitLab `main` at any time: before every push, `git fetch origin` and merge `origin/main`; if a push is rejected, someone landed first, so merge and push again (never force). `git push origin main` updates both GitLab and GitHub. If a push fails for any other reason, report the exact error and which remote.
2. Create the outbox branch: `local/outbox` containing only `LOCAL-OUTBOX.md` (header with `NEEDS HUMAN (open)` block, then entries). Push it to GitHub: `git push gh local/outbox`. The cloud reads it from there. Do this without disturbing `main` (use `git switch --orphan` or a temporary index; leave the working tree on `main` afterward).
3. **Reconcile and file new work (L6).** For each candidate in plan Part 6, grep `docs/team/tickets/*.md`, `docs/**`, and the GitLab board (`gitlab_sync.py list` for each person) for the same mechanism. File only what is genuinely missing, in `docs/team/tickets/anurup-followups-2026-10.md` (ticket format in `docs/team/tickets/README.md`, IDs from VS-30 onward for vault and search, unique and never reused, allowed labels from `LABEL_COLORS`, no em or en dashes). Validate with `make gitlab-plan`, dry-run with `make gitlab-sync`, then `make gitlab-sync APPLY=1`. The owner pre-authorizes this filing for this campaign (it overrides the "never create tickets" rule in `gitlab-agent-instructions.md` for now). Do not reassign or relabel existing tickets. Commit the file to `main` and push so the cloud sees the new IDs. Also ask on Minh's EM-03 ticket (comment, no reassignment) whether it can start this week, since VS-18 depends on it.

### Then, in this order (re-order if the dependency graph allows and say why in your outbox)

- **L2** Close what is already `doing`: GS-15, GS-17, PX-07. Evidence comments, move to `evidence`.
- **N1 and L3** Fix the journey handoff bug (tri-state arm API; skip the ordinary tick while `handoff_pending`; regression test proving handoff frames cannot make the first armed target a duplicate; dedupe evidence in the journey). Rerun Case 1 on the same public SimBio page. Only continue to Cases 2 to 6 after Case 1 completes and its bundle is reviewed. Cases need the owner at the keyboard for foregrounded content: batch those asks (plan Part 8). Do not tune OCR, extraction, embeddings, or ranking from failed artifacts. VS-28 labels are gold only after the owner approves them. Then VS-29.
- **L4** VS-14 (live 30-minute session with `FNDR_METRICS_DUMP`; counts only), then VS-15 and VS-16 (Accessibility text first, OCR fallback). This is the largest lever on vault quality (median stored text is 129 characters) and the deepest native work; protect long uninterrupted stretches for it.
- **L5** VS-17, VS-19, VS-20 on the M1 using the cloud's harnesses; record latency and memory honestly.
- **L1** (standing duty, minutes not hours): whenever `gh/claude/*` has new commits, run the plan's Part 5 merge gate on an `integrate/<train>` branch, merge to `main`, push `origin main`, post each ticket's evidence comment with the pasted command output, move tickets, and note "main moved to <sha>" in `LOCAL-OUTBOX.md` so the cloud rebases. Check `gh/claude/cloud-outbox` (`git fetch gh claude/cloud-outbox && git show gh/claude/cloud-outbox:CLOUD-OUTBOX.md`) at least every 90 minutes and before choosing your next task. Answer its contract notes and questions in your outbox.

### Board duties (yours only)

Follow `docs/team/gitlab-agent-instructions.md`: move to `doing` when you start (at most two at a time), comment the plan in two lines, comment progress at each behavior-changing commit, move to `evidence` with MR or merge evidence. Post the cloud's drafted comments verbatim apart from adding the merge sha. Never put tokens, captures, or memory text in a comment. Only act on tickets assigned to `anurupkumar`, plus the comment on EM-03 above.

## Autonomy and stuck protocol

Both are in the plan (Parts 7 and 8); read them again before your first long stretch. In short: you own the how; you may re-sequence, split, delete, push back, fan out sub-agents on disjoint files, and file follow-ups; you may not touch teammates' tickets or lanes, enable auto-merge, send real memory text off the Mac, or fabricate numbers. When stuck after three distinct written-down attempts: park the item, keep working, add the `NEEDS HUMAN (open)` block to `LOCAL-OUTBOX.md`, send `PushNotification` with the ASK sentence if the owner may be away (load its schema with ToolSearch first), and comment `NEEDS HUMAN: <ASK>` on the ticket. End your turn only when everything remaining is blocked or finished.

## Rhythm

Run in a self-paced loop: pick the next unblocked item, do it test-first, verify with the cheapest relevant check and say what you ran, commit, update the board and outbox, repeat. Refresh `LOCAL-OUTBOX.md` after every ticket and at least every two hours. When context gets heavy, write a handoff (`.agent-skills/portable-engineering/productivity/handoff/SKILL.md`) and continue from it. Finish line is the Oct 16 freeze: all 39 tickets at `evidence` or carrying a documented blocker, the Beta targets table in the month plan updated with measured values, and a final report in `LOCAL-OUTBOX.md`.

Begin with Step 0.
