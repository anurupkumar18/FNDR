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

October lanes (from 2026-09-28; tickets in `docs/team/tickets/`, plan in `docs/team/2026-10-month-plan.md`):

| Person | Lane | Tickets |
|---|---|---|
| Anurup | Lead; Vault and search (one retrieval path, real text, keyword plus meaning, evaluation) | `anurup-vault-search.md` |
| Kunj | Command surface (Screen Guide rebuilt), skills, local models | `kunj-command-skills-models.md` |
| Minh | Reopen exactly; embeddings and chunks on every memory | `minh-reopen-embeddings.md` |
| Felipe | Voice everywhere; onboarding and polish; product-oriented tests | `felipe-voice-onboarding-tests.md` |
| Everyone | Product, research, and decisions | `product-decisions.md` |

Boards, token setup, and the commands you and your agent use to move tickets: `docs/team/gitlab-agent-instructions.md`.

Ask the lane owner before changing their area. Ask the lead when two lanes disagree.

## What do I do now?

1. Open your board (Issues, Boards, then your name in the board menu; links in `docs/team/gitlab-agent-instructions.md`) and look at the Ready column.
2. Take the top ticket. Move it to `status::doing`. Keep at most two in Doing.
3. Read the ticket. Everything you need is in it: why, steps, how to verify, what evidence to attach.
4. Branch: `feat/<ticket-id>-short-name`. Never commit to `main`.
5. Do the work test-first. Run `make test`.
6. Open a merge request with the template, ask one teammate to review, and move the ticket to `status::evidence` with the MR link and your verification output in a comment.
7. After merge, attach the evidence the ticket asks for. The reviewer closes the ticket once the evidence checks out.

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

We rarely meet, so the week runs on written posts. Each one is short and goes where the next person will look for it.

| When | What | Where | Who |
|---|---|---|---|
| Monday, before the 30-minute plan | Plan post: the tickets you will move this week (IDs), and anything you need from another lane | Team chat | Everyone |
| Wednesday by noon | Check-in on each ticket in Doing: done, blocked, next (one line each) | Comment on the ticket | Ticket assignee |
| Friday | Scoreboard: retrieval, vault health, voice, and reopen numbers, from the commands and evidence files, never from memory (PD-05 makes it one command) | Team chat | Lead posts |
| Friday | A 3-minute recorded demo of what you moved this week; real output, not a polished mock | Link in the team chat and on the ticket | Everyone |
| Friday, 45 minutes | Demo and retro. The retro cuts p1 first. | Call | Everyone |
| Friday afternoon | Weekly status to instructors | As the instructors ask | Not assigned in writing yet |

Templates, so posts are quick to write and quick to read:

```
Monday plan (W<nn>), <name>
This week: <ticket IDs, p0 first>, <a p1 if time>
Need: <lane owner>, <what> by <day>
Away: <dates and cover, or none>
```

```
Wednesday check-in, <ticket ID>
Done: <what moved since Monday>
Blocked: <what, since when, who you pinged, or none>
Next: <the next step and when>
```

### Response norm

- Reply in the team chat within one working day, even if the reply is "seen, answer by Thursday".
- Blocked for more than one working day: ping the lane owner and the lead in the team chat, and say it on the ticket. Do not wait for Wednesday.
- A decision talked through in chat exists only once it is a row in `docs/team/decision-log.md` (the process is at the top of that file).

## Team charter (draft, pending owner approval)

**Status: draft (PD-17), 2026-10-04.** The owner has not approved it and no teammate has acknowledged it yet. Until each person acknowledges it in the team chat and the lead records that in `docs/team/decision-log.md`, the two lines under "Who owns what" are the rule. Lanes and cross-lane contracts come from section 7 of `docs/team/2026-10-month-plan.md`.

### Who decides what

| Kind of decision | Examples | Who decides |
|---|---|---|
| Inside your lane | How to build a ticket, module layout inside your lane's files, test design, the order of your own p0 tickets, splitting a ticket, the form of your evidence | You, alone. Say what you chose in the merge request. |
| Your lane's scope or promises | Dropping or deferring a p0, moving a p1 into the week, a new model or large dependency, any number we will show outside the team, a change to a demo beat in month plan section 9 | You propose; the lead decides |
| Across lanes or about trust | A cross-lane contract (below), what FNDR stores or what leaves the Mac (ADR-018), the actions policy tiers (ADR-022), the five destinations (ADR-023), the Beta date, this charter and the weekly rhythm | Everyone: written options, then the disagreement rule below |

| Lane | Owner | Decides alone, for example | Contract it publishes | Must agree before that contract changes |
|---|---|---|---|---|
| Vault and search | Anurup | Ranking, fusion, query parsing, evaluation queries, as long as `make qa-retrieval` Recall@5 drops no more than 0.05 on any path | `retrieve` | Kunj (`search` tool), Minh (reopen checks) |
| Reopen and embeddings | Minh | How each app reopens, chunking parameters (with before and after `make qa-retrieval`, month plan section 10), backfill scheduling | `reopen_memory` | Kunj (`open_memory_source`) |
| Command surface, skills, local models | Kunj | Router grammar, executors, prompts, local model settings, inside the actions policy | Risk policy and tool registry | Minh (MCP reopen); tier changes need everyone (ADR-022) |
| Voice, onboarding, tests | Felipe | Voice UI, onboarding copy, test structure, inside the voice policy (ADR-020) | `voice://state` | Kunj (router) |

The lead owns a lane too. When the lead's own lane changes a contract, the same people must agree; being lead does not skip that step.

### When we disagree

1. Either person writes the options down in the repo or on the ticket: two or three options, what each costs, and what would show which is right.
2. If a measurement can settle it in under a day, run it first and attach the output.
3. Post the link in the team chat and tag everyone affected. Comments are open for 48 hours.
4. If there is still no agreement after 48 hours, the lead decides and records the decision, the options, and who disagreed in `docs/team/decision-log.md`.
5. Once it is recorded, everyone builds on it. Reopening it needs new evidence, not a new argument.

### When you are away

- Away for a working day or more: post the dates and who covers for you in the team chat before you go (the "Away" line in the Monday plan post).
- For each ticket you have in Doing: push your branch, write a handoff note (`docs/handoffs/<date>-<ticket>-<name>.md`, see "Handoff" above), and comment on the ticket with the branch, the note, and the next step.
- Your cover answers questions about your contracts and may merge a reviewed fix in your lane. Your cover does not change your lane's scope; that waits for you or goes to the lead.
- Away for three working days or more: move tickets you cannot finish back to Ready, or ask the lead to reassign them. Only the lead reassigns.
- From the Oct 16 freeze to Beta (Oct 21): nobody is away without a named cover.

## Working with AI agents

`AGENTS.md` tells agents how to work here, and both Claude Code and Codex reach it. Put the ticket's Agent brief in
your prompt. Review every line an agent writes. Record the tool in the merge request's AI assistance section. Commit
after every plan step. When a limit or your credits run low, follow `docs/team/agent-switch.md` to hand the ticket to
the other tool without losing work. If two agents are active on different tickets at the same time, see that same
file's "Running truly in parallel" section for the one extra rule sequential handoff does not need.
