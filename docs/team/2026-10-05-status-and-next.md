# Status and next: Oct 5

Local session report. Numbers come from commands run on the M1 and from the cloud outbox; nothing is estimated.

## What shipped (on `main`)

| Area | Result |
|---|---|
| Cloud trains A to F | Merged through the local gate (342e5a2). 15 tickets in `evidence`: VS-02 to VS-11, VS-13, VS-21, VS-25, VS-26, VS-41 (MCP batch auth bypass fixed). BM25 full-text search, one `retrieve` function used by Search, Ask, and MCP, deterministic ranking, phrase filters. |
| Gate on the M1 | `make test`: 478 frontend and 988 Rust tests, 0 failed. Knowledge-worker retrieval check passes (Recall@5 1.000, all paths). Office-pm: Recall@5 unchanged at 0.900, MRR@10 0.661 to 0.638, one paraphrase fell from rank 9 to a miss (M1 versus Linux numerics; VS-63 fixes the gate). |
| VS-30 | Handoff frames no longer seed dedupe history; the dedupe decision (threshold, match kind, distances) is recorded in the journey. Case 1 not yet rerun. |
| VS-15 and VS-16 | `focused_text` reads Accessibility text under a 50 ms and 4,000 node cap, never reads secure fields; capture prefers it at 200+ characters (`FNDR_AX_TEXT=0` disables). Works for Electron apps; Chrome showed no page content (VS-42). Not validated live. |
| VS-31, VS-39, PX-07 | Review token cap 320 to 512 (24% of reviews were truncated); `gitlab_sync.py flag` fills the board columns; PX-07 boundary matrix (34 call sites), 29 tests, two real trace bugs fixed. |

## Needs your manual QA

One 60 minute session, then short follow-ups. Say "go" in the local session when at the Mac.

| # | Step | Time | Tickets |
|---|---|---|---|
| 1 | Case 1 rerun on the public SimBio page | 10 min | VS-30, VS-28 |
| 2 | 30 minute normal use with the metrics dump | 30 min | VS-14, VS-16, VS-31 |
| 3 | 12-app Accessibility matrix with `ax_text_probe` | 10 min | VS-15, VS-42 |
| 4 | Five native activity-trace flows | 10 min | PX-07 |
| 5 | Screen Guide matrix (full-screen, Retina, denied, Private Mode) | 15 min | GS-15 |
| 6 | Vault in the real app, one screen per day | 5 min | VS-23 |
| 7 | Decisions: ADR-018, `fndr.remember` questions, charter, office-pm reference | reading | PD-01, PD-17, VS-35 |
| 8 | Two user conversations | 40 min | PD-18 |

## Risks

- Your board load is 323 nominal hours (p0 157). The freeze is Oct 16; the p0 core is VS-42 to VS-51 and VS-58 to VS-61.
- Capture quality is unproven live: Chrome exposes no web content to Accessibility today.
- "No good match" failed its done-when (no-match scores 0.351, real queries as low as 0.290); parked until chunk scores exist (VS-18 needs EM-03, Minh).
- Memory pressure on the 8 GB Mac blocks builds; VS-64 and VS-65 address it.

## New tracks (VS-42 to VS-68, 27 tickets, filed on the board)

| Track | Tickets | Priority |
|---|---|---|
| A Capture text quality | VS-42 to VS-46 | p0 (VS-46 p1) |
| B One embedder (EmbeddingGemma, one dimension, retire MiniLM and BGE-large) | VS-47 to VS-51 | p0 |
| C Image vector: spike, then index, route, tests, UI | VS-52 to VS-57 | p1, go or no-go at VS-52 |
| D Trust and privacy (egress counts, Keychain, redact versus skip, MCP sweep) | VS-58 to VS-61 | p0 |
| E QA automation and dev loop | VS-62 to VS-65 | p1 |
| F Beta demo | VS-66 to VS-68 | p1 |

Cloud-suitable: VS-47, VS-54, VS-55, VS-56, VS-61, VS-63, VS-64, VS-66, VS-68. Hybrid: VS-49, VS-52. Local: the rest.

## Embedding dimensions, answered

Text is MiniLM 384, image is CLIP 512, chunks are BGE-large 1024, in separate columns or tables never compared with each other. Every search uses only the 384-dim text vectors. The chunk route is off by default and the 512-dim image vector is stored but never searched.
