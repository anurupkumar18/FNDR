# PD-04: Beta story and what the judges score (cloud draft)

Date: 2026-10-04. Branch `claude/train-e-docs`.

## What was produced

- `docs/product/qa-prep.md`:
  1. What the panel and course reward. The course rubric is not in the repo; the page quotes what the repo does say (instructors inspect the live demo with slides, a written evidence packet, and the board plus commit history) and lists assumed criteria, clearly marked as assumptions.
  2. Every beat of month plan section 9 mapped to the evidence file that proves it today, or "missing" with the ticket that will produce it. Today: one beat proven on the seeded profile (find by a keyword query), two proven only at a test boundary (Resume over MCP, tool registry and risk policy), several half or partial, and the rest missing (downloads by content, streaming voice, write-back, skills, the native blocklist run, MCP and cloud logging, after-numbers, time to recover).
  3. Ten hard questions with short answers, each grounded in a cited file. Numbers only from `VS-01-baseline.md`, `retrieval-baseline-seeded.md`, `vault-health-owner.md`, `voice-baseline.md`, and `resume-work-latency.md`.
  4. A dry-run checklist and a notes template.

## Findings worth the owner's attention

- The seeded query that backs the 0:30 beat is the keyword form; the demo's looser wording is not in the labeled set.
- Preview cannot jump to a PDF page (`docs/product/reopen-qa-matrix.md`, RE-04 follow-up), so "opens the PDF on page 112" is true only for browser PDFs, which have not been run live.
- Write-back (2:20) has a spec ticket (VS-35) but no build ticket.
- The capture privacy gate skips a frame that matches a secret pattern; it does not redact it. The Q&A answer says so.

## What remains for a human

- The dry run itself: a presenter and one person outside the team, under five minutes, notes filled in with the template and committed as `docs/evidence/W04/PD-04-dry-run-<date>.md`. That file is the ticket's evidence.
- Get the actual course rubric from the instructors and replace the assumed criteria.
- Decide which missing beats to cut at the Oct 16 freeze.

## How to verify

- Every evidence path in the beat table exists in the repo; every number in the answers appears in the cited file.
- `grep -nP '[\x{2013}\x{2014}]' docs/product/qa-prep.md` prints nothing.
