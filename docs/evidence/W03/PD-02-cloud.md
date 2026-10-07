# PD-02: positioning page and competitor teardown (cloud draft)

Date: 2026-10-04. Branch `claude/train-e-docs`.

## What was produced

- `docs/product/positioning.md`: who it is for, the problem in their words (marked as our hypothesis until PD-18 and PD-13 supply real quotes), FNDR in one sentence, three proof points with their evidence files and what not to overstate, what we are not, a six-product teardown (Screenpipe, Rewind and Limitless, Microsoft Recall, Raycast AI, ChatGPT desktop memory, Claude desktop memory), where FNDR loses to all of them today, and a Sources table.

## How facts were checked

- Competitor facts: web pages fetched on 2026-10-04, one source URL per fact in the Sources table. Vendor capability and privacy statements are marked [V] (unverified vendor claim). OpenAI's own help and pricing pages refused our fetch, so ChatGPT prices come from a third-party list and are labeled as such.
- FNDR facts: only from committed evidence (`docs/evidence/W02/VS-01-baseline.md`, `docs/evidence/W03/resume-work-mcp.md`, `docs/evidence/W03/resume-work-latency.md`) and code read on 2026-10-04 (`privacy_proof.rs`, ADR-004, ADR-017, ADR-022). No number was estimated.
- Conflicts found and kept visible: Screenpipe's About page and pricing page list different prices.

## What remains for a human

- Replace the hypothesis quotes with real ones after PD-18 and PD-13.
- Ticket "Done when": every teammate can repeat the one sentence, and OB-03 and the slides use it. That needs the team.
- Re-check prices the week of Beta.

## How to verify

- Open each Sources URL; the cited fact should be on the page as of the access date.
- Each FNDR number in the proof-point table appears in the named evidence file.
- `grep -nP '[\x{2013}\x{2014}]' docs/product/positioning.md` prints nothing.
