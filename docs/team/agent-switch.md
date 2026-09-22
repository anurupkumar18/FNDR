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

## Running truly in parallel

The rules above assume sequential handoff: one agent stops, the other resumes
the same ticket on the same branch. FNDR has also run a different mode in
practice: Claude Code and Codex both active at the same time, each on its own
ticket, from the same physical checkout at `~/FNDR` (not two separate clones).
That mode needs one more rule the sequential case does not, learned from a
real incident on 2026-09-22, not a hypothetical.

**What went wrong:** both tools ran `git checkout`/`commit`/`merge` directly
in `~/FNDR`. Since a git checkout has exactly one `HEAD`, one tool's checkout
silently moved the other's `HEAD` to a different branch mid-session. It was
caught because a routine `git checkout main` landed on the wrong branch, and
`git status` turned up the other tool's own uncommitted, in-progress files
sitting in the shared tree. No work was lost (git does not drop committed
history), but an interleaved `git checkout` plus an uncommitted edit from the
other tool is a real corruption risk, not just a cosmetic mix-up. Codex hit
and independently fixed the identical problem in the same session.

**The fix:** when two agents are genuinely active at once, each works from
its own `git worktree`, not the shared `~/FNDR` checkout:

```bash
git worktree add ~/FNDR-<short-lane-name> -b <branch> origin/main
```

Do all commits, builds, and test runs for that lane inside its own worktree
directory. The only operations that should still touch the shared `~/FNDR`
checkout are `git fetch` and `git push` (which read/write refs, not the
working tree, so they cannot collide), plus the final `git merge --no-ff
<branch>` once a lane's work is reviewed and ready to land, done by whichever
tool is doing the merge at that moment, one at a time.

**One shared resource worktrees do not isolate:** this repo builds Rust into
a single shared target directory (`~/.cache/cargo-target-shared/`, not a
per-worktree `target/`) to save disk space. Two worktrees' `cargo build`/
`cargo test` calls still contend for that one directory's build lock. This
is expected and self-resolving: a queued build waits, it does not fail. Do
not kill another lane's `cargo`/`rustc` process to "resolve" a lock wait;
one session did this by mistake this same day and got lucky that the other
lane's build had already finished. Wait, or ask the human running both
sessions to sequence the heavy builds a few minutes apart instead.

**When you are done with a lane:** merge, verify the merged result, then
remove the worktree and delete its branch (`git worktree remove`, `git
branch -d`) so it does not linger. Worktrees that were never yours to create
(another tool's, or ones you find already present) are not yours to remove
either; leave them for their owner.
