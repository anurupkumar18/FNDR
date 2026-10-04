# PD-18: interview guide and note template (cloud draft)

Date: 2026-10-04. Branch `claude/train-e-docs`.

## What was produced

- `docs/research/conversations.md` (new; also used by PD-19, PD-20, PD-21):
  - Public-repo rule: no name, employer, school, client, product, URL, file name, screenshot, audio, or video.
  - Interviewer rules: ask about the last time, no pitching, no leading, no demo until the end.
  - A consent note to read aloud.
  - The 20-minute PD-18 guide: warm-up, finding something you saw, getting back into work after an interruption, AI tools, close, with what to listen for in each part.
  - A note template per conversation, two empty PD-18 entries, and a "What we learned" section that feeds the hypothesis quotes in `docs/product/positioning.md`.

## What remains for a human

- The owner holds two 20-minute conversations with knowledge workers outside the team and fills the two entries. Those entries are the ticket's evidence.
- After both, fill "What we learned" and replace the hypothesis lines in the positioning page.

## How to verify

- The guide covers the three topics the ticket names (finding things they saw, resuming after interruption, AI tools) and puts the demo last.
- The template has no field for a name or employer.
- `grep -nP '[\x{2013}\x{2014}]' docs/research/conversations.md` prints nothing.
