# PD-03: async rhythm and decision log (cloud draft)

Date: 2026-10-04. Branch `claude/train-e-docs`.

## What was produced

- `docs/team/decision-log.md`: the process (draft, post, two days, record), ten seeded rows, six earlier accepted ADRs as background, and an "Open" table of items that are assumed or proposed but not agreed.
- `docs/team/TEAM.md`, section "Weekly rhythm": replaces the old two-line rhythm with a table (Monday plan post, Wednesday check-in per Doing ticket, Friday scoreboard and 3-minute recorded demo, Friday retro, instructor status), two post templates, and the response norm (reply within one working day; blocked more than a day means ping the lane owner and the lead).

## How the seed rows were chosen

Only decisions already recorded in the repo as accepted or decided: ADRs 014, 015, 017, 020, 021, 022, 023 (status "Accepted"), master plan section 3 D-9 ("Decided by owner"), and month plan section 12 rows 2 and 3 ("Decided" and "Done"). Where a source names no one, "Who agreed" says "not recorded". Nothing was inferred from chat or commit authorship.

## What remains for a human

- The ticket's evidence is the first Monday post link. The owner writes and posts it in the team chat with the template in `TEAM.md`, then pastes the link on PD-03.
- Teammates can correct "who agreed" for any row they took part in; edit the row and cite where the agreement is written.
- The Friday instructor status has no named owner in writing; the lead should assign it.

## How to verify

- Each row's link resolves to a file in the repo whose status line or text matches the row.
- `grep -nP '[\x{2013}\x{2014}]' docs/team/decision-log.md docs/team/TEAM.md` prints nothing.
