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

Note on rule 4: the owner currently works solo on this repo for most of the
manifest and has adopted a different, explicitly chosen default for himself
(committing directly to `main` for most work, branching only for a genuinely
risky change or something he wants cordoned off for review). That is a
deliberate, single-owner exception, not a change to the team rule above.
Once the team is regularly shipping in parallel, the branch-and-review flow
in this section is the one to follow.

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
the other tool without losing work. If two agents are active on different tickets at the same time, see that same
file's "Running truly in parallel" section for the one extra rule sequential handoff does not need.
