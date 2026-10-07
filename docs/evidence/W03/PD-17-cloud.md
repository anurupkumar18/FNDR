# PD-17: team charter (cloud draft)

Date: 2026-10-04. Branch `claude/train-e-docs`. Status: draft, not approved, not acknowledged.

## What was produced

- `docs/team/TEAM.md`, new section "Team charter (draft, pending owner approval)":
  - Three kinds of decision (inside your lane, your lane's scope, across lanes or about trust) and who decides each.
  - One row per lane from month plan section 7: owner, what the owner decides alone, the contract the lane publishes, and who must agree before it changes. Contracts are the four named in section 7 (`retrieve`, `reopen_memory`, `voice://state`, the risk policy).
  - The disagreement rule: written options, a measurement first if it takes under a day, 48 hours for comments, then the lead decides and records it.
  - Handoff when away: post dates and cover, handoff note per Doing ticket, what a cover may and may not do, three days or more means back to Ready or the lead reassigns, named cover required from freeze to Beta.
- `docs/team/decision-log.md`: the charter is listed under "Open", not as a decision.

## What remains for a human

- The owner reads and edits the draft, then removes "draft" from the heading.
- Each teammate (Kunj, Minh, Felipe) acknowledges it in the team chat. The ticket's evidence is those acknowledgements.
- The lead records the acceptance as a row in `docs/team/decision-log.md` with who acknowledged and the date, and removes it from "Open".

## How to verify

- Every lane, owner, and contract in the charter matches month plan section 7.
- The status line in the section says the draft is not approved or acknowledged.
- `grep -nP '[\x{2013}\x{2014}]' docs/team/TEAM.md` prints nothing.
