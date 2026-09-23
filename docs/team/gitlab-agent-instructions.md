# GitLab board: setup and agent instructions

Every teammate (and every coding agent working for them) moves tickets the same way, from the terminal, with one script: `scripts/team/gitlab_sync.py`. The board is how we see each other's progress, since we rarely meet.

## Boards

| Board | What it shows |
|---|---|
| FNDR team | Every open ticket in four columns: Ready, Doing, Review, Evidence |
| Anurup, Kunj, Minh, Felipe | The same four columns for one person's tickets |
| People | One column per person, for a quick "who is doing what" |

Each person also has a lane hub issue, `[LANE] <Name>: October lane`, which lists every ticket of theirs by week with checkboxes that follow the issues. Links to all boards are printed at the end of `make gitlab-sync` and pinned in the team chat.

If our GitLab plan cannot save a per-person filter on a board, the per-person board links include `?assignee_username=<you>`; bookmark your link.

## One-time setup (each person, 3 minutes)

1. On `capstone.cs.utah.edu`, open your avatar, then Edit profile, then Access tokens. Create a personal access token named `fndr-agent` with the `api` scope and an expiry of 2026-12-20.
2. Store it in your macOS Keychain (you will be asked to type it; it is never shown or saved in a file):

```bash
security add-generic-password -s fndr-gitlab -a "$USER" -w
```

3. Connect to the university VPN, then check it works:

```bash
python3 scripts/team/gitlab_sync.py list
```

Never paste the token into chat, a ticket, a commit, a file, or an agent prompt. The script reads it from the Keychain (or `GITLAB_TOKEN`) and never prints it. To replace it: `security delete-generic-password -s fndr-gitlab`, then repeat step 2.

## The ticket lifecycle

| Status | Means | Who moves it there |
|---|---|---|
| `ready` | Specified and unblocked; nobody has started | The sync, on creation |
| `doing` | Someone is working on it now (at most two per person) | The assignee or their agent |
| `review` | A merge request is open and a teammate is asked to review | The assignee or their agent |
| `evidence` | Merged; the ticket's evidence is attached in a comment | The assignee or their agent |
| `closed` | The reviewer checked the evidence | The reviewer |

## Commands

```bash
python3 scripts/team/gitlab_sync.py list                      # my open tickets
python3 scripts/team/gitlab_sync.py list --status doing       # my tickets in Doing
python3 scripts/team/gitlab_sync.py list --user minhpro001    # someone else's
python3 scripts/team/gitlab_sync.py move VS-05 doing
python3 scripts/team/gitlab_sync.py comment VS-05 "Started. Plan: failing test first, then remove the cutoff."
python3 scripts/team/gitlab_sync.py move VS-05 review
python3 scripts/team/gitlab_sync.py comment VS-05 "MR !57. make qa-retrieval-check: Search Recall@5 0.68 -> 0.77, no query lost."
python3 scripts/team/gitlab_sync.py move VS-05 evidence
```

`move` and `comment` refuse to touch a ticket that is not assigned to the token's owner unless you add `--any` (only the reviewer closing a ticket should need it).

## Instructions to give your coding agent

Paste this block into the agent's instructions (Claude Code: your project or user memory; Codex: your session prompt), with your ticket ID filled in:

```text
You are working on FNDR ticket <ID>. The ticket text is in docs/team/tickets/*.md under "## <ID>".
Follow AGENTS.md. Board rules:
- When you start: python3 scripts/team/gitlab_sync.py move <ID> doing, then comment your plan in two lines.
- Comment progress at each commit that changes behavior: what changed, what you ran, what is next.
- When a merge request is open: move <ID> review and comment the MR link and the verification output.
- After merge: comment the evidence the ticket asks for, then move <ID> evidence.
- Only act on tickets assigned to me. Never change assignees, labels other than status, milestones, or other people's tickets.
- Never create new tickets. If you find a bug or missing work, write it in a comment on <ID> and tell me; I will decide.
- Never put tokens, screen captures, memory text, or personal data in comments.
- If a command fails (VPN down, token expired), stop and tell me; do not retry in a loop.
```

## Changing tickets

Tickets live in `docs/team/tickets/*.md` (format in `docs/team/tickets/README.md`). To add or change one, edit the file in a merge request, then run:

```bash
make gitlab-plan    # validate and show hours per person; no network
make gitlab-sync    # dry run against GitLab
make gitlab-sync APPLY=1          # create what is missing
make gitlab-sync APPLY=1 UPDATE=1 # also rewrite existing descriptions from the files
```

Status, comments, and merge request links belong in GitLab; descriptions belong in the files.
