# WS4 Team Operating System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Several tasks need a human with a GitLab account; those are marked.

**Goal:** Every teammate can answer, at any moment, what to do next, why, how to test it, what evidence to attach, what is done, and what the product looks like in three months, and instructors can verify all of it from the GitLab board, the commit history, and an evidence packet.

**Architecture:** The plan's ticket manifest generates the board, so the plan and the board cannot drift. Every ticket is self-contained (why, steps, verify, evidence, agent brief). Evidence lives in the repo under `docs/evidence/W<nn>/`. A Monday-Wednesday-Friday rhythm keeps the board honest. Git identity is unified with `.mailmap` so contribution counts are correct.

**Tech Stack:** GitLab at `capstone.cs.utah.edu` (issues, labels, milestones, boards, merge requests), a bash script and a Python script (both tested), Markdown templates, `git shortlog`.

**Spec:** `2026-09-21-beta-final-master-plan.md` sections 5 and 7 (manifest). Tickets: OPS-01 to OPS-12.

## Global Constraints

- GitLab is the source of truth for the board and merge requests (decision D-6, proposed). GitHub is a push mirror only.
- Nothing requires a paid GitLab tier: scoped labels and iterations are Premium features, so labels use the `::` naming as plain labels and weekly milestones stand in for iterations. The bootstrap creates them either way. Verify the tier in Task 1.
- Never commit a GitLab token, real captures, database blobs, or model files.
- No commits to `main`. No em dashes in any document, ticket, or commit message.

---

## 1. Why the team is lost, and what fixes each cause

| Cause (verified in the repo) | Effect | Fix in this plan |
|---|---|---|
| Two repos and two planning worlds (v2 PRD says v2 is mainline while teammates commit to v1) | Nobody knows where to work | ADR-015, `CLAUDE.md` pointer (OPS-01) |
| No `.gitlab/` directory: no issue or merge request templates | Tickets have no why, no test, no evidence | Templates (Task 2) |
| Planning docs are v2-shaped (122 tickets, 6 milestones, lane labels) | Board would not match this semester | Manifest of 47 Beta tickets generated into the board (Task 4) |
| No cadence or definition of done | Work ends with "I think it works" | `TEAM.md` and Monday, Wednesday, Friday rhythm (Task 3) |
| Same person appears under several git identities (`git shortlog` splits Anurup into 3 rows, Minh into 2, Kunj into 3, Felipe into 2) | Instructors inspecting commit history undercount everyone | `.mailmap` (Task 2) |
| CI runs only from `.github/workflows/`; the capstone GitLab shows no pipeline | Graders see no test evidence on GitLab | Attach `make test` output to every merge request as evidence |
| No picture of the finished product | Teammates cannot judge whether their ticket matters | `FEA-01` storyboard plus the Vision section in the master plan |

## 2. The system on one page

```
Plan manifest (master plan, section 7)
        |  scripts/team/plan_to_csv.py
        v
GitLab issues: why, steps, verify, evidence, agent brief, labels, milestone, estimate
        |
   status::ready -> status::doing -> status::review -> status::evidence -> Closed
        |                                   |                    |
   pick a ticket                   merge request           evidence files in
   (max 2 doing)                   + make test output      docs/evidence/W<nn>/
        |
Monday plan (30 min) | Wednesday async comment | Friday demo + retro (45 min)
        |
docs/status/2026-W<nn>.md  ->  instructors
        |
Beta and Final evidence packets
```

## 3. GitLab configuration

**Labels (32).** `status::` (ready, doing, review, evidence), `ws::` (capture, model, features, ops, security, learning, memory, retrieval, decisions), `type::` (feature, spike, bug, chore, docs, learning), `prio::` (p0, p1, p2), `phase::` (beta, final), `evidence::` (needed, attached), `agent::` (either, claude, codex), plus `blocked`, `agent-ok`, and `needs-human`. `agent::either` is the default for agent-executed tickets; whoever starts a ticket changes it to `agent::claude` or `agent::codex` so two tools never work the same branch. Created by `scripts/team/gitlab_bootstrap.sh`.

**Milestones (12).** `W01-Baseline` (Sep 21 to 27) through `W12-Submit` (Dec 7 to 13). A ticket's milestone is its due week.

**Board.** Six lists: Open (backlog), `status::ready`, `status::doing`, `status::review`, `status::evidence`, Closed. Filters people use: by assignee, by `ws::`, by milestone.

**Branch protection.** `main` protected: no direct push, merge requests only, one approval required.

**Boards and time.** Use GitLab time tracking quick actions (`/estimate 7h`, `/spend 2h`) so hours are on the record; they are available without a paid tier.

## 4. Ticket anatomy

Every ticket answers the questions a teammate, an instructor, or an AI agent would otherwise have to ask:

| Question | Where the ticket answers it |
|---|---|
| Who is accountable, who does the work, and with or without an agent? | Owner and Executor lines |
| How long, and by when? | Time expected (nominal hours) and Due (the week's milestone) |
| How far along is the phase? | Phase line (ticket k of N, P0 hours scheduled before it) plus the live report from `make phase-progress` |
| Why does this exist? | Why |
| What happened earlier, what must be closed first, what comes next? | Where this fits (Earlier, Must be closed before you start, Next) |
| What exactly do I do? | What to do (a link to the numbered steps in a plan) |
| What should the pull request look like? | Pull request shape (branch, scope, size class, commits, merge request rules) |
| How do I know it works, and what proves it? | How to verify and Evidence to attach |
| When is it done? | Definition of done |
| How do I hand it to Claude Code or Codex, and switch when a limit hits? | Agent brief and switching between tools |

The generator writes all of this from the manifest row, and computes the Next line and the phase position, so the ticket, the plan, and the board cannot drift apart.

## 5. Definition of done, evidence, and QA

**Definition of done (all must be true to close):**

1. A merge request is merged with green local verification: `make test` output pasted, plus the ticket's own verify command.
2. One teammate other than the author reviewed the merge request.
3. Evidence is attached and the label changed from `evidence::needed` to `evidence::attached`.
4. No real captures, tokens, or database files are in the diff.
5. If the work continues past this ticket, a handoff note exists.

**Evidence by ticket type:**

| Type | Required evidence |
|---|---|
| Feature with UI | Screen recording (60 seconds or less) plus the test output |
| Backend or pipeline | Test output plus before and after numbers from `make capture-baseline` or `make eval` |
| Spike | Findings note with a go or no-go and the measured numbers |
| Bug | Failing test that now passes, plus the reproduction in the issue |
| Learning | The one-page summary in `docs/learning/` |
| Ops | The merged file or a screenshot of the configured GitLab page |

**Where it lives.** Files under `docs/evidence/W<nn>/`. Recordings are large: attach them to the issue in GitLab and put the link in the evidence file rather than committing video. Evidence files contain aggregates only, never captured content.

**QA checklist before Evidence to Done:** the feature works from a clean launch (`make demo`), it works with the seeded demo vault (`scripts/demo/demo-week.json`), it degrades honestly when a model is missing, and it does not write anything outside the app data directory.

## 6. Handoff (people and agents)

`AGENTS.md` already tells agents to use the handoff skill when stopping. The format below is the same for a person leaving for the weekend and an agent ending a session. Save as `docs/handoffs/<date>-<ticket>-<name>.md`.

```markdown
# Handoff <date>: <ticket id> <who or which agent>

- Ticket and branch, last commit:
- Done (with evidence links):
- Not done, and exactly where it stopped:
- How to run and test it (commands):
- Decisions made and why:
- Surprises and failed attempts:
- Next three steps:
- Tool or agent that produced this work:
```

## 7. Weekly rhythm

| When | What | Output |
|---|---|---|
| Monday, 30 min | Review the board. Pick tickets. Anyone planned over 12 hours moves a P1 out. Confirm the week's gate. | Board is current |
| Wednesday, by noon | Each person comments on their Doing ticket: progress, blocked, next. Three lines. | The comment thread is the record |
| Friday, 45 min | Each person shows work (3 minutes, live or recorded). Then retro: start, stop, continue. Tickets move to Done only with evidence. | Recordings linked in tickets; retro notes in the status file |
| Friday afternoon | Draft `docs/status/2026-W<nn>.md` from closed issues; send to instructors. | Weekly status |

Board rules: no card, no merge; at most two tickets in Doing per person; a ticket in Doing for three working days without a comment gets a ping; `blocked` requires a comment saying what unblocks it.

**Beta gates.** Feature lock Fri Oct 2. Feature complete (ugly allowed) Fri Oct 9. Freeze Fri Oct 16: only bug fixes and evidence. Rehearsals Mon Oct 19 and Tue Oct 20. Beta presentation Wed Oct 21 (assumed).

---

## 8. Agentic development with Claude Code and Codex

**Principle: state lives in git, not in a chat.** A ticket is a self-contained prompt, a branch is the workspace, a commit per plan step is the checkpoint, and `docs/handoffs/` is the memory between sessions. Either tool can pick up any ticket because nothing it needs lives only in the other tool's context. Both tools reach the same rules: Claude Code reads `CLAUDE.md`, which points to `AGENTS.md`; Codex reads `AGENTS.md`.

**Budget playbook for 5-hour windows and credits**

- Plan the day by ticket, not by tool. Put the steps that need judgment (design decisions, debugging with an unclear cause, reviewing a diff) on whichever tool has the most budget left; put mechanical steps (copying tested assets, wiring, running commands) on whichever has the least. Do not assume one tool is better; measure it (below).
- Spend fewer tokens by being exact. Give the ticket text and file and line ranges from the plan. Never ask an agent to read `capture/mod.rs` (6,485 lines) whole; use the ranges the plan names. Run `make` targets and paste only the failing lines. Ask for a diff summary instead of a re-print of files. Cap pasted logs at about 100 lines.
- Steps in the plans are sized for one to five minutes of agent work. Finish the step you are on before a limit hits; stop between steps.
- Measure leverage. After each session log `/spend <time>` on the issue and note the tool. Two weeks of that data replaces the assumption in decision D-9 with a number.

**Switch protocol (when a limit warning appears or credits run low)**

1. Finish the current step. Run the ticket's verify command.
2. Commit: `wip(<ticket id>): step N done` on the ticket's branch, and push it.
3. Write `docs/handoffs/<date>-<ticket id>-<tool>.md` using the template in section 6.
4. Change the issue label from `agent::claude` (or `agent::codex`) back to `agent::either`, and comment which tool stopped and why.
5. Open the other tool in the repository on the same branch. Paste the resume prompt from `docs/team/agent-switch.md`.
6. The receiving agent first runs the verify command to learn the baseline, reads `git diff main...HEAD --stat` and the handoff note, and only then continues from the next unchecked step.

**Rules for both tools:** one agent per ticket at a time; never two on one branch; no commits to `main`; no force pushes; no real captures, tokens, or database files in a diff; stop and ask the owner if a step contradicts the code; a human commits and pushes under their own name, and no commit carries an AI co-author trailer (Claude Code and Codex write the change and leave it uncommitted for the human to verify and commit); no em dashes.

---

## Task 1: Bootstrap labels, milestones, and the board (OPS-02)

**Files:**
- Create: `scripts/team/gitlab_bootstrap.sh`

**Interfaces:**
- Produces: 26 labels and 12 milestones in the project, matching the names `plan_to_csv.py` writes in quick actions (Task 4).

The script below was syntax-checked and run in dry-run mode before this plan was written: it prints 32 label calls and 12 milestone calls, and refuses to run for real without a token. It has not been run against your GitLab because that needs a token and network access.

- [ ] **Step 1: Check the GitLab tier (human, 2 minutes)**

In the project, create two labels named `x::one` and `x::two`, apply both to one issue, and see whether GitLab lets both stay. If it removes the first, scoped labels are active (Premium). Either way continue; delete the two test labels afterward.

- [ ] **Step 2: Create the script**

Create `scripts/team/gitlab_bootstrap.sh` (executable):

```bash
#!/usr/bin/env bash
# Create the FNDR label set and 12 weekly milestones in GitLab. Safe to run twice.
#
#   GITLAB_TOKEN=<personal access token with api scope> scripts/team/gitlab_bootstrap.sh
#   scripts/team/gitlab_bootstrap.sh --dry-run        # print what would be created, needs no token
#
# Never commit the token. Host and project can be overridden with GITLAB_HOST and GITLAB_PROJECT
# (the project path is URL-encoded, for example fndr%2Ffndr).
set -euo pipefail

HOST="${GITLAB_HOST:-https://capstone.cs.utah.edu}"
PROJECT="${GITLAB_PROJECT:-fndr%2Ffndr}"
DRY_RUN=0
FAILED=0
[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=1

if [[ $DRY_RUN -eq 0 && -z "${GITLAB_TOKEN:-}" ]]; then
  echo "Set GITLAB_TOKEN to a personal access token with the api scope (never commit it)." >&2
  exit 1
fi

api() {
  local path="$1"; shift
  if [[ $DRY_RUN -eq 1 ]]; then
    echo "DRY POST $path $*"
    return 0
  fi
  local code
  code=$(curl -sS -o /dev/null -w '%{http_code}' -X POST \
    -H "PRIVATE-TOKEN: ${GITLAB_TOKEN}" "${HOST}/api/v4/projects/${PROJECT}/${path}" "$@")
  case "$code" in
    200|201) echo "ok   ${path} $*" ;;
    400|409) echo "skip ${path} $* (already exists)" ;;
    *) echo "FAIL ${code} ${path} $*" >&2; FAILED=1 ;;
  esac
}

LABELS=(
  "status::ready|#5BC0DE" "status::doing|#F0AD4E" "status::review|#A78BFA" "status::evidence|#34D399"
  "ws::capture|#1F77B4" "ws::model|#9467BD" "ws::features|#2CA02C" "ws::ops|#7F7F7F"
  "ws::security|#D62728" "ws::learning|#BCBD22"
  "type::feature|#0E8A16" "type::spike|#FBCA04" "type::bug|#B60205" "type::chore|#C5DEF5"
  "type::docs|#0075CA" "type::learning|#D4C5F9"
  "prio::p0|#B60205" "prio::p1|#FBCA04" "prio::p2|#C2E0C6"
  "phase::beta|#1D76DB" "phase::final|#5319E7"
  "evidence::needed|#E99695" "evidence::attached|#0E8A16"
  "ws::memory|#8C564B" "ws::retrieval|#17BECF" "ws::decisions|#E377C2"
  "blocked|#000000" "agent-ok|#006B75" "needs-human|#D93F0B"
  "agent::either|#C2E0C6" "agent::claude|#D2691E" "agent::codex|#4B0082"
)

MILESTONES=(
  "W01-Baseline|2026-09-21|2026-09-27" "W02-Measure|2026-09-28|2026-10-04"
  "W03-Build|2026-10-05|2026-10-11" "W04-Prove|2026-10-12|2026-10-18"
  "W05-Retro|2026-10-19|2026-10-25" "W06-Foundations|2026-10-26|2026-11-01"
  "W07-FineTune|2026-11-02|2026-11-08" "W08-Preference|2026-11-09|2026-11-15"
  "W09-Agent|2026-11-16|2026-11-22" "W10-Harden|2026-11-23|2026-11-29"
  "W11-Freeze|2026-11-30|2026-12-06" "W12-Submit|2026-12-07|2026-12-13"
)

for entry in "${LABELS[@]}"; do
  api labels --data-urlencode "name=${entry%%|*}" --data-urlencode "color=${entry##*|}"
done

for entry in "${MILESTONES[@]}"; do
  IFS='|' read -r title start due <<<"$entry"
  api milestones --data-urlencode "title=${title}" --data "start_date=${start}" --data "due_date=${due}"
done

exit "$FAILED"
```

- [ ] **Step 3: Dry run**

Run: `chmod +x scripts/team/gitlab_bootstrap.sh && scripts/team/gitlab_bootstrap.sh --dry-run | grep -c '^DRY POST labels'`
Expected: `32`. Then `scripts/team/gitlab_bootstrap.sh --dry-run | grep -c '^DRY POST milestones'` expects `12`.

- [ ] **Step 4: Real run (human, needs a token)**

Create a personal access token with the `api` scope (User settings, Access tokens), then:

```bash
GITLAB_TOKEN=<your token> scripts/team/gitlab_bootstrap.sh
```

Expected: 44 lines beginning `ok` or `skip`, no `FAIL`. Revoke the token afterward if you do not need it again.

- [ ] **Step 5: Create the board lists (human, 5 minutes)**

Issues, Boards, add lists for `status::ready`, `status::doing`, `status::review`, `status::evidence`. Open and Closed are built in. Take a screenshot for the OPS-02 evidence.

- [ ] **Step 6: Commit**

```bash
git checkout -b chore/ops-02-gitlab-bootstrap
git add scripts/team/gitlab_bootstrap.sh
git commit -m "chore(team): script to create GitLab labels and weekly milestones"
```

## Task 2: Issue templates, merge request template, .mailmap, contribution report (OPS-03)

**Files:**
- Create: `.gitlab/issue_templates/Task.md`, `.gitlab/issue_templates/Spike.md`, `.gitlab/issue_templates/Bug.md`
- Create: `.gitlab/merge_request_templates/Default.md`
- Create: `.mailmap`
- Create: `scripts/team/contribution_report.sh`

- [ ] **Step 1: Create the Task template**

`.gitlab/issue_templates/Task.md`:

```markdown
<!-- A ticket that someone else can pick up without asking you is a finished ticket. Fill every section. -->

**Owner (accountable):**
**Executor:** (name, and whether an AI agent is used)
**Due:** (the week's milestone)
**Time expected:** (hours for one competent person, before any agent help)
**Phase and progress:** (Beta or Final, ticket k of N; run `make phase-progress`)

## Why
One or two sentences: what problem this solves and who it helps.

## Where this fits
- Earlier: what was true before this ticket
- Must be closed before you start:
- Next, unblocked by this ticket:

## What to do
Numbered steps, or a link to the exact steps in `docs/superpowers/plans/`.

## Pull request shape
- Branch: `feat/<id>-short-name`
- Scope: what the pull request touches
- Size: S (about 100 lines or fewer), M (100 to 400), or L (split into two merge requests)
- Commits: one per plan step, message `type(scope): what`
- Merge request: the template, `Closes #<this issue>`, `make test` output, evidence, one reviewer who is not the author

## How to verify
A command to run or something observable, with the expected result.

## Evidence to attach before Done
What file or recording proves it works, and where it will live (`docs/evidence/W<nn>/`).

## Definition of done
- [ ] Merge request reviewed by one other teammate
- [ ] `make test` output pasted in the merge request
- [ ] Evidence attached, label changed to `evidence::attached`
- [ ] No real captures, tokens, or database files in the diff
- [ ] Handoff note written if the work continues

## Agent brief and switching between tools
- Read first: `AGENTS.md` and the plan section this ticket points to
- Constraints: strictly local models, no real captures in git, branch and merge request only, no em dashes
- Checkpoint: commit after every plan step so either tool can resume from git alone
- If a 5-hour limit or your credits run low: commit, write a handoff, paste the resume prompt from `docs/team/agent-switch.md` into the other tool
- Stop and ask the owner if: a step contradicts the code you find

/label ~"status::ready" ~"evidence::needed"
```

- [ ] **Step 2: Create the Spike template**

`.gitlab/issue_templates/Spike.md`:

```markdown
<!-- A spike answers one question in a fixed time. It ends with a decision, not with code. -->

## Question
The one thing we need to know.

## Time box
Hours you will spend at most. Stop at the limit and report.

## Method
How you will find out (what you will build, measure, or read).

## Result
Go or no-go, with the measured numbers and a link to the findings note in `docs/evidence/`.

## Decision and next ticket
What we do now because of the answer.

/label ~"type::spike" ~"status::ready" ~"evidence::needed"
```

- [ ] **Step 3: Create the Bug template**

`.gitlab/issue_templates/Bug.md`:

```markdown
## What happened
## What should have happened
## How to reproduce
Exact steps. Include the app version or commit.
## Evidence
Logs (with any personal content removed) or a screen recording.
## Suspected cause
Only if you have evidence. Otherwise leave blank.

/label ~"type::bug" ~"status::ready"
```

- [ ] **Step 4: Create the merge request template**

`.gitlab/merge_request_templates/Default.md`:

```markdown
## Ticket
Closes #

## What changed and why

## How I verified
- [ ] `make test` (paste the summary lines)
- [ ] The ticket's own verify command (paste the command and output)

## Evidence
Links to files under `docs/evidence/` or recordings.

## Privacy check
- [ ] No real captures, tokens, or database files in this diff

## Phase progress
Beta or Final, and the line from `make phase-progress` after this merge.

## AI assistance
Tool used (Claude Code or Codex) and what for, and any switch between them mid-ticket. I reviewed every changed line: yes or no.

## Handoff needed?
If work continues after this merge, link the handoff note.
```

- [ ] **Step 5: Create `.mailmap`**

```
Anurup Kumar <81anurup@gmail.com> anurupkumar18 <81anurup@gmail.com>
Anurup Kumar <81anurup@gmail.com> Anurup Kumar <107144237+anurupkumar18@users.noreply.github.com>
Anurup Kumar <81anurup@gmail.com> Anurup Kumar <u1413345@utah.edu>
Kunj Rathod <rathodkunj2005@gmail.com> kunjrathod2005 <rathodkunj2005@gmail.com>
Kunj Rathod <rathodkunj2005@gmail.com> Kunj Rathod <u1497420@utah.edu>
Kunj Rathod <rathodkunj2005@gmail.com> Kunj Rathod <86590228+rathodkunj2005@users.noreply.github.com>
Felipe Marin <felipe.marin.1697@gmail.com> felipemarin-16 <felipe.marin.1697@gmail.com>
Felipe Marin <felipe.marin.1697@gmail.com> Felipe Marin <u1442515@umail.utah.edu>
Minh Le <anhminh7802@gmail.com> Minh <anhminh7802@gmail.com>
Minh Le <anhminh7802@gmail.com> Minh Le <u1329156@utah.edu>
```

These mappings use addresses already present in the repository history. Anurup's work address on three commits is intentionally left out; those commits already carry the canonical name.

- [ ] **Step 6: Verify the mailmap on the real history**

Run: `git shortlog -sn HEAD </dev/null | head -8`
Expected (verified before this plan was written, counts as of 2026-09-21): four people on top: Anurup Kumar 264, Kunj Rathod 38, Felipe Marin 19, Minh Le 13, then `Claude`, `dependabot[bot]`, `Claude_helper`. Before the mailmap the same command shows 11 rows.

- [ ] **Step 7: Create the contribution report**

`scripts/team/contribution_report.sh` (executable; syntax-checked and run against this repository before this plan was written):

```bash
#!/usr/bin/env bash
# Commits per person since a date. Uses .mailmap, so one person is one row.
# Counts commits, not lines of code, because lines reward verbosity. Pair it with the board's closed issues.
#
#   scripts/team/contribution_report.sh 2026-09-21
set -euo pipefail

SINCE="${1:-2026-09-21}"

echo "# Contributions since ${SINCE}"
echo
echo "| Commits | Author |"
echo "|---|---|"
git shortlog -sn --no-merges --since="${SINCE}" HEAD </dev/null | while IFS=$'\t' read -r count name; do
  printf '| %s | %s |\n' "$(echo "${count}" | tr -d ' ')" "${name}"
done
```

Run: `chmod +x scripts/team/contribution_report.sh && scripts/team/contribution_report.sh 2026-08-01`
Expected: a markdown table with one row per person.

- [ ] **Step 8: Verify the templates appear in GitLab (human)**

After merging, create a new issue in GitLab and confirm the Description template picker lists Task, Spike, and Bug. Screenshot for evidence.

- [ ] **Step 9: Commit**

```bash
git checkout -b chore/ops-03-templates-and-mailmap
git add .gitlab .mailmap scripts/team/contribution_report.sh
git commit -m "chore(team): issue and merge request templates, mailmap, contribution report"
```

## Task 3: TEAM.md, the one page every teammate reads (OPS-04)

**Files:**
- Create: `docs/team/TEAM.md`
- Modify: `docs/README.md` (add a row)

- [ ] **Step 1: Write `docs/team/TEAM.md`**

```markdown
# FNDR team guide

Read this first. It answers: what are we building, what do I do now, and how do I know I am done.

## What we are building (three months from now)

FNDR is the local memory of your work. It watches, remembers, and hands you or any agent exactly the
cited context needed to resume, and the local model behind it measurably improves from feedback while
nothing leaves your Mac. The picture is in `docs/product/vision/` and the plan is in
`docs/superpowers/plans/2026-09-21-beta-final-master-plan.md` (section 4).

- Beta (about Oct 21): Resume Work, Privacy Proof, Intelligence panel, measured pipeline and model. Deja vu if time allows.
- Final (about Dec 14): approve-then-act local agent, a fine-tuned adapter promoted through an eval gate, a user study.

## Who owns what

| Person | Lane |
|---|---|
| Anurup | Lead, model harness, integration, demo |
| Kunj | Native macOS and capture pipeline |
| Minh | Retrieval, evaluation, MCP security |
| Felipe | Design, product surfaces, evidence packet, slides |

Ask the lane owner before changing their area. Ask the lead when two lanes disagree.

## What do I do now?

1. Open the board: Issues, Boards. Look at the `status::ready` column filtered to your name.
2. Take the top ticket. Move it to `status::doing`. Keep at most two in Doing.
3. Read the ticket. Everything you need is in it: why, steps, how to verify, what evidence to attach.
4. Branch: `feat/<ticket-id>-short-name`. Never commit to `main`.
5. Do the work test-first. Run `make test`.
6. Open a merge request with the template. Ask one teammate to review. Move the ticket to `status::review`.
7. After merge, attach the evidence (see below) and move the ticket to `status::evidence`. The reviewer closes it.

If nothing is ready for you, say so in the team chat on Monday. Do not invent work.

## Definition of done

A ticket is done only when all are true:
- Merged, with `make test` output pasted in the merge request
- Reviewed by a teammate other than the author
- Evidence attached, label `evidence::attached`
- No real captures, tokens, or database files in the diff
- A handoff note written if the work continues

## Evidence

Evidence is a file or recording that proves the ticket's verify step. Keep files under
`docs/evidence/W<nn>/` (aggregates and numbers only, never captured content). Attach recordings to the
issue and link them from the evidence file. Numbers come from `make capture-baseline` and `make eval`.

## Handoff

When you stop with work in flight, write `docs/handoffs/<date>-<ticket>-<name>.md` using the template in
`docs/superpowers/plans/2026-09-21-ws4-team-operating-system.md` (section 6). This applies to people
and to AI agents alike.

## Rules that protect the project

- Strictly local models. No cloud LLM at runtime.
- Never commit real screen captures, databases, tokens, or model files.
- One vertical slice at a time. No drive-by refactors.
- Say what you ran to verify. Do not claim what you did not run.
- No em dashes in docs, tickets, or commit messages.

## Weekly rhythm

Monday 30 minutes: plan. Wednesday noon: comment on your Doing ticket (progress, blocked, next).
Friday 45 minutes: demo (3 minutes each) and retro. Friday afternoon: the weekly status goes to instructors.

## Working with AI agents

`AGENTS.md` tells agents how to work here, and both Claude Code and Codex reach it. Put the ticket's Agent brief in
your prompt. Review every line an agent writes. Record the tool in the merge request's AI assistance section. Commit
after every plan step. When a limit or your credits run low, follow `docs/team/agent-switch.md` to hand the ticket to
the other tool without losing work.
```

- [ ] **Step 2: Add a row to `docs/README.md`**

Add: `| [team/TEAM.md](team/TEAM.md) | Team guide: what we build, what to do now, definition of done |` to the "Start here" table.

- [ ] **Step 3: Verify (human, W2)**

Ask each teammate to pick up a `status::ready` ticket using only this page. Note every question they had to ask; each question becomes an edit to this page. Record the count in the W2 retro.

- [ ] **Step 4: Commit**

```bash
git checkout -b docs/ops-04-team-guide
git add docs/team/TEAM.md docs/README.md
git commit -m "docs(team): TEAM.md with workflow, definition of done, evidence, and handoff"
```

## Task 4: Generate the board and the progress report from the manifest (OPS-05)

**Files:**
- Create: `scripts/team/plan_to_csv.py`, `scripts/team/test_plan_to_csv.py`, `scripts/team/phase_progress.py`, `scripts/team/test_phase_progress.py` (copied from the tested assets)
- Modify: `Makefile` (target `phase-progress`)
- Create (by running): the CSV is a build artifact and is not committed

**Interfaces:**
- Consumes: the manifest table in the master plan (section 7.1) with 14 columns `ID | Title | WS | Owner | Executor | Week | Hours | Prio | Deps | Why | Earlier | PR shape | Verify | Evidence`.
- Produces: `plan_to_csv.py` writes `title,description` where the description holds every field in section 4 plus quick actions for labels, milestone, estimate, and the executor as assignee. It also exports `parse_manifest`, `build_description`, `load_by_executor`, `phase_of`, `position_in_phase`, `unblocks`. `phase_progress.py` joins the manifest with closed GitLab issues (titles starting `[ID]`) and prints progress by phase and priority, by workstream, and by executor, plus what each person can start now. GitLab requires labels and milestones to exist first (Task 1).

Both scripts were written test-first and run before this plan was written. `test_plan_to_csv.py` has 19 tests, including six that read the real manifest: every ticket id appears in the plan file its link points to; nobody's P0 hours exceed four weeks of fifteen; people, weeks, and prefixes are known; no dependency is scheduled after the ticket that needs it; the dependency graph has no cycles; and no P0 ticket depends on a P1 ticket. `test_phase_progress.py` has 8 tests.

- [ ] **Step 1: Copy the tested files**

```bash
cp docs/superpowers/plans/assets/2026-09-21/ws4/plan_to_csv.py \
   docs/superpowers/plans/assets/2026-09-21/ws4/test_plan_to_csv.py \
   docs/superpowers/plans/assets/2026-09-21/ws4/phase_progress.py \
   docs/superpowers/plans/assets/2026-09-21/ws4/test_phase_progress.py scripts/team/
```

- [ ] **Step 2: Run the tests**

Run: `python3 scripts/team/test_plan_to_csv.py && python3 scripts/team/test_phase_progress.py`
Expected: `Ran 19 tests ... OK` then `Ran 8 tests ... OK`, no skips. A skip means the master plan was not found from that location. A failure of "not found in" means a ticket in the manifest has no steps in its plan file: add them, do not delete the check. A failure of "needs ... scheduled after" means a dependency is in a later week: fix the manifest.

- [ ] **Step 3: Optional roster for assignees**

Create `docs/team/roster.json` mapping first names to your Utah GitLab usernames, for example `{"Anurup": "<username>", "Kunj": "<username>", "Minh": "<username>", "Felipe": "<username>"}`. Each person puts their own username. Without the file the script still works and prints who to assign by hand. The assignee is the Executor, not the Owner.

- [ ] **Step 4: Generate**

```bash
python3 scripts/team/plan_to_csv.py docs/superpowers/plans/2026-09-21-beta-final-master-plan.md \
  --out /tmp/board-seed.csv --roster docs/team/roster.json
```

Expected: `wrote 47 issues to /tmp/board-seed.csv`. Open the CSV once and check that one description contains all of: Owner, Executor, Time expected, Phase (ticket k of N), Why, Earlier, Must be closed before you start, Next, Branch, Scope, Size, How to verify, Evidence, Agent brief and switching. Add `--weeks W1 W2` to import only the first two weeks.

- [ ] **Step 5: Import (human)**

GitLab project, Issues, the import icon, Import from CSV, choose the file. Expected: a message that 47 issues will be imported (or the count for the weeks you chose). After import the board's `status::ready` column shows the tickets (quick actions applied labels, milestone, estimate, and assignee).

- [ ] **Step 6: Spot-check three issues**

Open `[CAP-01]`, `[FEA-02]`, and `[MEM-03]`. Each must show a milestone, an estimate, the labels (including `agent-ok` and `agent::either`), and every section listed in Step 4. If a label or milestone is missing, Task 1 did not create it: rerun Task 1 and re-import only those issues.

- [ ] **Step 7: Add the progress target and run it**

Append to `Makefile` (and add `phase-progress` to `.PHONY`):

```make
phase-progress:
	python3 scripts/team/phase_progress.py --manifest docs/superpowers/plans/2026-09-21-beta-final-master-plan.md --api
```

Run (needs a token with the `read_api` scope): `GITLAB_TOKEN=<your token> make phase-progress`
Expected: a markdown report with `Beta P0` at `0 of` some number before any ticket is closed, and a "Next up" line per person. Without a token, use the offline form: `python3 scripts/team/phase_progress.py --manifest docs/superpowers/plans/2026-09-21-beta-final-master-plan.md --closed CAP-01 OPS-01`.

- [ ] **Step 8: Commit**

```bash
git checkout -b chore/ops-05-plan-to-csv
git add scripts/team Makefile docs/team/roster.json
git commit -m "chore(team): generate the GitLab board and the phase progress report from the plan manifest"
```

## Task 5: Hardware inventory and bench machine (OPS-06)

**Files:**
- Create: `docs/team/hardware.md`

- [ ] **Step 1: Each person runs**

```bash
sysctl -n machdep.cpu.brand_string; echo "$(( $(sysctl -n hw.memsize) / 1073741824 )) GB"; sw_vers -productVersion
```

- [ ] **Step 2: Write the table**

```markdown
# Hardware

| Person | Chip | RAM | macOS | Can fine-tune a 1B to 2B model (16 GB or more)? |
|---|---|---|---|---|
| Anurup | Apple M1 | 8 GB | (run sw_vers) | Tight |
```

Add one row per person. Name one machine the bench machine: all numbers in `docs/evidence/` for baselines and bake-offs come from it, so they stay comparable. The reference profile is the M1 8 GB (`FNDR_MODEL_PROFILE`).

- [ ] **Step 3: Commit**

```bash
git checkout -b docs/ops-06-hardware
git add docs/team/hardware.md
git commit -m "docs(team): hardware inventory and bench machine"
```

## Task 6: Agent switching protocol (OPS-12)

**Files:**
- Create: `docs/team/agent-switch.md`

**Interfaces:**
- Consumes: the handoff template (section 6), the ticket text generated by Task 4, and the labels from Task 1.
- Produces: the resume prompt every ticket links to.

- [ ] **Step 1: Write `docs/team/agent-switch.md`**

```markdown
# Switching between Claude Code and Codex

Use this when a 5-hour usage window is nearly spent or credits are low. The ticket, the branch, and the handoff
note carry all the state, so either tool can continue.

## Before you switch (about five minutes)

1. Finish the current plan step. Run the ticket's verify command.
2. Commit and push: `wip(<ticket id>): step N done`.
3. Write `docs/handoffs/<date>-<ticket id>-<tool>.md` (template in `docs/superpowers/plans/2026-09-21-ws4-team-operating-system.md`, section 6).
4. On the issue: set the label back to `agent::either` and comment which tool stopped and why.

## Resume prompt (paste into the other tool, with the ticket text)

~~~text
You are continuing ticket <ticket id> on branch <branch> in the FNDR repository.
Read AGENTS.md first. Then read the handoff note docs/handoffs/<file> and run
git diff main...HEAD --stat. The ticket text is below.
Before changing anything, run the ticket's verify command and tell me the result.
Continue from the next unchecked step in the plan section named in the ticket.
Commit after every step. Do not touch main. Stop and ask if a step contradicts the code.
<paste the ticket description here>
~~~

## Saving tokens

- Point the agent at file and line ranges from the plan. Do not have it read `capture/mod.rs` whole.
- Run `make` targets yourself and paste only the failing lines (under about 100).
- Ask for a diff summary, not reprinted files.
- One ticket per session. Stop between plan steps.

## Rules

One agent per ticket at a time. Never two on one branch. No force pushes. No em dashes. A human commits and pushes under
their own name; no commit carries an AI co-author trailer. Log time on the issue with `/spend`, naming the tool, so we can measure what each saves.
```

- [ ] **Step 2: Test the switch on a small real ticket (human, 20 minutes)**

Take `OPS-06` (hardware inventory) or any ticket of two hours or less. Start it in one tool, stop after step 1 using the "Before you switch" list, and finish it in the other tool using only the resume prompt. Expected: the second tool reaches a merge request without asking you to re-explain anything. Every question it had to ask is a gap in the handoff or the resume prompt: fix the file.

- [ ] **Step 3: Record the result and commit**

Put the handoff note and the merge request link on the issue as evidence.

```bash
git checkout -b docs/ops-12-agent-switch
git add docs/team/agent-switch.md
git commit -m "docs(team): protocol for switching between Claude Code and Codex mid-ticket"
```

## Task 7: Weekly status and evidence conventions (OPS-07)

**Files:**
- Create: `docs/status/TEMPLATE.md`, `docs/status/2026-W40.md`
- Create: `docs/evidence/README.md`, `docs/incidents.md`

- [ ] **Step 1: Weekly status template**

`docs/status/TEMPLATE.md`:

```markdown
# Status 2026-W<nn> (Mon <date> to Sun <date>)

## Summary
Three lines: what moved, what is at risk, what we need.

## Done this week
| Ticket | Owner | Evidence |
|---|---|---|

## Metrics (baseline to now)
| Metric | Baseline | Now | Source file |
|---|---|---|---|

## Next week
The top five tickets and their owners.

## Risks and decisions needed

## Contribution
Paste `scripts/team/contribution_report.sh <monday>` output.
```

- [ ] **Step 2: Evidence README**

`docs/evidence/README.md`:

```markdown
# Evidence

One folder per week: `W01/`, `W02/`, and so on. Each ticket's evidence is a file here or a link to a recording.

Rules: numbers and aggregates only. No captured content, no personal data, no tokens. Reports name the machine
and the build type. A claim in a slide or report must point to a file in this folder.

Index of claims (add a row when a claim appears in any slide or report):

| Claim | File | Ticket |
|---|---|---|
```

- [ ] **Step 3: Incidents log**

`docs/incidents.md` starts with the entries already known:

```markdown
# Incidents and reversals

The best interview material is what went wrong and what we changed. One entry per incident.

## 2026-05-17: every capture silently dropped
- What happened: a validation rule counted a diagnostic label as critical, so every extraction with weak grounding was dropped and nothing was stored.
- How we found it: memory count stayed at zero although capture ran.
- Fix: removed the label from the critical set; aligned batch outcomes.
- What we changed after: per-reason skip counters and an unexplained-drops line in the baseline report (WS1).

## 2026-09-21: MCP defaults exposed the memory store
- What happened: Local mode required no token, skipped the Origin check, and allowed any CORS origin.
- How we found it: reading `mcp/mod.rs` while planning, matching the v2 audit's finding.
- Fix: SEC-01.
- What we changed after: adversarial tests named `mcp_rejects_*` run in `make test`.
```

- [ ] **Step 4: First status report**

Copy the template to `docs/status/2026-W40.md` at the end of W2 and fill it from the board. Expected: a file an instructor can read in two minutes.

- [ ] **Step 5: Commit**

```bash
git checkout -b docs/ops-07-status-and-evidence
git add docs/status docs/evidence/README.md docs/incidents.md
git commit -m "docs(team): weekly status template, evidence conventions, incidents log"
```

## Task 8: Beta evidence packet (OPS-09, W4)

**Files:**
- Create: `docs/evidence/beta/README.md`

- [ ] **Step 1: Assemble the index**

`docs/evidence/beta/README.md` contains, in this order, a link for each item:

1. One-page summary: what Beta shows and what it does not.
2. Demo script (from OPS-11) and the recorded backup video link.
3. Metrics table: each Beta target from the master plan section 4, baseline, measured, with the evidence file for each (`capture-baseline.md`, `eval-baseline.md`, `model-bakeoff.md`, `resume-work-latency.md`).
4. Test summary: the last `make test` output and `make py-test` output, with the commit hash.
5. Privacy and security: `sec-01.md`, the Privacy Proof recording, the threat model.
6. Decisions: ADR-015, ADR-016, ADR-017 and the feature lock note.
7. Contribution: `scripts/team/contribution_report.sh 2026-09-21` output and a board export (CSV of closed issues).
8. What we cut and why, and the incidents log.
9. User quotes (from the dogfood diary and the study when available).

- [ ] **Step 2: Verify every claim traces to a file**

Run: `grep -o 'docs/evidence/[A-Za-z0-9_./-]*' docs/evidence/beta/README.md | sort -u | while read f; do test -e "$f" || echo "MISSING $f"; done`
Expected: no output.

- [ ] **Step 3: Commit**

```bash
git checkout -b docs/ops-09-beta-evidence-packet
git add docs/evidence/beta
git commit -m "docs(evidence): Beta evidence packet index"
```

## Task 9: Slides, rehearsals, and backup video (OPS-10, W4)

**Files:**
- Create: `docs/product/beta-slides.md` (outline; the deck itself lives where the team edits it)

- [ ] **Step 1: Slide outline (10 slides for a 5 minute demo plus questions)**

1. Problem: agents forget your context; cloud memory tools upload it.
2. What FNDR is, in one sentence.
3. Related work in one table (Screenpipe, Recall, Rewind and Limitless, Mem0 and Zep) with sources.
4. Architecture: capture to memory to context, with the trust layer.
5. Pipeline scoreboard: baseline to now.
6. Resume Work demo screen.
7. Privacy Proof demo screen.
8. Intelligence: bake-off and eval numbers with intervals.
9. What we cut and what is next (approve-then-act, self-improvement).
10. Team and contribution.

- [ ] **Step 2: Two timed rehearsals**

Mon Oct 19 and Tue Oct 20, timed, recorded. The first is with the team, the second with one non-team person who asks questions. Record the times and the questions asked in `docs/evidence/W04/rehearsals.md`.

- [ ] **Step 3: Backup video**

Record the full demo on the seeded demo vault. Verify it plays with the network off and with the projector resolution. Link it in the packet.

- [ ] **Step 4: Commit**

```bash
git checkout -b docs/ops-10-beta-slides
git add docs/product/beta-slides.md docs/evidence/W04/rehearsals.md
git commit -m "docs(product): Beta slide outline and rehearsal log"
```

## Task 10: Demo script and hard-question prep (OPS-11, W4)

**Files:**
- Create: `docs/product/qa-prep.md`

- [ ] **Step 1: Write the ten questions with a pointer to the evidence for each**

```markdown
# Hard questions

| Question | Where the answer lives |
|---|---|
| How is this different from Microsoft Recall and Screenpipe? | WS3 plan, section 2 (market map), and the related-work slide |
| How do you know it works? | `make eval` report with confidence intervals; `capture-baseline.md` |
| What happens to sensitive data? | Privacy Proof recording; SEC-01 evidence; no screenshots persisted (ADR 004) |
| Why a small local model and is it good enough? | `model-bakeoff.md` and ADR-016 |
| What did you cut and why? | `feature-lock-2026-10-02.md` |
| What was your hardest bug? | `docs/incidents.md` |
| How would you improve the model over time? | WS2 improvement ladder and the promotion gate |
| What are the risks of an agent acting on screen content? | WS3 approve-then-act design rules and the injection tests |
| How does each person's work show up? | Board and `contribution_report.sh` |
| What would you do with three more months? | Final epics in the master plan |
```

- [ ] **Step 2: Write your answer to each in your own words, two sentences each**

The owner presents the model story, so answer the fourth and seventh questions aloud twice. Add the answers under the table. Anything you cannot answer from evidence becomes a ticket.

- [ ] **Step 3: Dry run with a non-team person under five minutes**

Time it. Record what confused them.

- [ ] **Step 4: Commit**

```bash
git checkout -b docs/ops-11-qa-prep
git add docs/product/qa-prep.md
git commit -m "docs(product): hard-question prep for the Beta demo"
```

## Final packet (E-F8, W11 to W12): structure only, detailed at the Beta retro

Report outline (course-facing): abstract; problem and related work; system design (capture, memory, model harness, agent); evaluation (methodology with intervals, results, threats to validity); privacy and ethics statement; user study; limitations; contribution statement; appendices (ADRs, evidence index). Additional deliverables: poster, demo video, clean-Mac install check, repository archive with the evidence folder.

## Self-review

**Spec coverage (ask 4 and ask 8):** board and labels and milestones (Task 1), tickets with what, when, why, verify, evidence, and agent brief (Tasks 2 and 4), agreement between teammates (TEAM.md, Task 3; feature lock in WS3), instructors' view (weekly status, evidence, packets, contribution report), handoff for agentic development (section 6, TEAM.md, MR template), a picture of the 3-month product (`FEA-01` and master plan section 4).

**Placeholder scan:** `docs/team/roster.json` takes each person's own GitLab username; the generator works without it and says who to assign by hand. The token in Task 1 Step 4 is supplied by the human at run time by design.

**Type consistency:** label and milestone names in Task 1 match the constants in `plan_to_csv.py` (`MILESTONES`, `WS_LABELS`, `TYPE_LABELS`). The manifest columns in Task 4 match the master plan's section 7.1 header. Ticket ids OPS-01 to OPS-11 map to tasks here and to master Task 2 (OPS-01).
