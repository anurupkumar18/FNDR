# Decision log

One row per decision the team has agreed to. If it is not in this table, it is not decided, however often it was discussed in chat. Newest at the bottom.

## How a decision gets here

1. The ticket's assignee drafts the options in the repo (an ADR under `docs/decisions/`, or a short section in the ticket's doc).
2. Post the draft link in the team chat and tag the people it affects.
3. Give everyone two days to comment. Silence after two days counts as agreement (`docs/team/tickets/product-decisions.md`).
4. If someone disagrees and it is not settled in writing, the lead decides (`TEAM.md`: "Ask the lead when two lanes disagree").
5. Add a row here in the same merge request that marks the ADR accepted. "Who agreed" names the people who said yes in writing, or says "silence after two days" and the date the clock ran out.

A row never claims agreement that is not written down somewhere. Where the source does not say who agreed, the row says "not recorded".

## Decisions

Seeded on 2026-10-04 from decisions already recorded in the repo as accepted or decided. Where the source carries no date, the date is the commit that added it.

| Date | Decision | Options considered | Who agreed | Link |
|---|---|---|---|---|
| 2026-09-08 | Screen Guide is opt-in, read-only, and local-first; screen pixels stay in memory for one explicit request. Amended 2026-09-28 for bounded one-turn diagnostics. | Vendoring the Clicky app (rejected in the ADR's context: cloud audio, image, and answer services, second app identity) | Not recorded | [ADR-014](../decisions/014-local-screen-guide.md) |
| 2026-09-21 | This repo (v1) is the Beta and Final product. FNDR v2 is a read-only knowledge source. | v2 as mainline (v2 PRD addendum of 2026-09-05) or v1 | Owner (Anurup), named as decider. Team review was to happen on the merge request; no reviewer is recorded. | [ADR-015](../decisions/015-v1-product-v2-knowledge-source.md), master plan D-1 |
| 2026-09-21 | MCP requires a bearer token by default in every mode, including Local. Only the loopback `initialize` and `tools/list` handshake is exempt. | Not listed as options. `FNDR_MCP_REQUIRE_AUTH=0` stays as a development opt-out, off by default. | Owner (Anurup), named as decider. No reviewer recorded. | [ADR-017](../decisions/017-mcp-auth-default.md) |
| 2026-09-21 | The owner is the accountable DRI for the capture, post-capture, model, retrieval, and decision workstreams and may delegate execution. (The capacity rule in the same row is still proposed.) | Not recorded | Owner (Anurup) | Master plan section 3, D-9 |
| 2026-09-23 | Retrieval ownership: Anurup owns retrieval and its evaluation; Minh owns embedding coverage and reopen. | Not recorded | Recorded as "decided"; who agreed is not recorded | Month plan section 12, row 2 |
| 2026-09-23 | The notch HUD branch is merged into `main` (350105c); GS-02 reconciles it with the command surface. | Not recorded | Not recorded | Month plan section 12, row 3 |
| 2026-09-28 | Voice policy: one microphone owner; Home and Search tap to toggle; Screen Guide push to talk; a final transcript is always reviewed before anything runs; no spoken answers by default. Implementation pending. | Not listed. Builds on UI/UX decisions D-04 and D-21. | Not recorded | [ADR-020](../decisions/020-voice-interaction-policy.md) |
| 2026-09-28 | Activity traces show only observed steps, never content or chain-of-thought, and are never persisted. | Raw logs in the UI; a product-wide polling or telemetry system (both rejected in the ADR's context) | Not recorded | [ADR-021](../decisions/021-privacy-safe-activity-traces.md) |
| 2026-09-30 | Command bar actions policy: twelve tools in three tiers (runs, one tap, never). Nothing sends, deletes, or buys this semester. (PD-06) | Everything one tap; allow send with confirm (both rejected in the ADR) | Not recorded | [ADR-022](../decisions/022-command-bar-actions-policy.md), [actions policy](../product/actions-policy.md) |
| 2026-10-01 | Five destinations (Home, Search and Ask, Memory Vault, Daily Brief, Trust and Settings) plus a Labs group. No mounted panel is deleted. (PD-15) | Not listed. Builds on UI/UX decisions D-01, D-05, D-06, D-07. | Not recorded | [ADR-023](../decisions/023-five-destinations-and-labs.md) |
| 2026-10-06 | Notch Do: native voice owner, listen on open, plan card that auto-starts after 1.5 s, voice or button Stop kills the run. | Keep the WebKit listener; require a tap on every transcript | Kunj (lane owner) | [ADR-020 amendment](../decisions/020-voice-interaction-policy.md) |
| 2026-10-06 | Notch Do computer use: tiered per-call policy (runs, one confirmation, never) in code; ADR-022's never tier unchanged. | Approve every click (the 09-24 build) | Kunj (lane owner) | [ADR-022 amendment](../decisions/022-command-bar-actions-policy.md) |
| 2026-10-07 | Agent surfaces: one egress rule, one consent pattern, a durable request log, and one stop for Hermes, Screen Guide and Notch Do. Related memories to cloud providers off by default; Hermes answers only; approval by tap; browsers ask by default; plans auto-start only when no step can need a yes. | Leave as is; one gateway module; merge the two policy tables | Owner, with Kunj (lane owner) | [ADR 024](../decisions/024-agent-surfaces-egress-and-action-policy.md) |
| 2026-10-07 | ADR-018 option B ratified for Notch Do, Hermes and Screen Guide's ChatGPT answers only. Everything else stays local. | Accept ADR-018 as a whole; revert the amendment | Owner | [ADR-018 amendment](../decisions/018-reasoning-tier.md) |
| 2026-10-08 | ADR-018 accepted as a whole: a cloud model only on a turn the person starts, on a surface where they chose the provider (Agent page, Notch Do, Screen Guide's ChatGPT answer). Every background task stays local for Beta and Final; Ask and the router stay local for Beta. | The staged per-task plan waiting on PD-08; local only with the three paths removed | Owner | [ADR-018](../decisions/018-reasoning-tier.md) |
| 2026-10-08 | Notch Do acts with FNDR's own accessibility code for Final. For Beta it stays in Labs and runs only where open-computer-use is already installed. FNDR never attaches a tool that runs model-written code, which rules out the ChatGPT app's current Computer Use. | Attach the new tool; bundle or install the npm helper | Owner | [ADR 026](../decisions/026-notch-do-acts-with-fndrs-own-hands.md) |
| 2026-10-08 | Agent chat history is bounded by size, never by age: the newest 200 chats, 400 messages in each. | A 30-day expiry like the request log; no bound | Owner | [ADR 024](../decisions/024-agent-surfaces-egress-and-action-policy.md) |
| 2026-10-08 | A card line is in voice when it is a past-tense statement of what happened or a present-tense statement of what the screen held. A narrator, an instruction or a guessed intention is never in voice. The extraction prompt keeps "Describe what is visible". | Ask the model for a past-tense action in every summary (it invents one when the screen shows none); rewrite the sentences by rule; compose every card line from the title only | Owner | [Voice and fallback](../evidence/W04/voice-and-fallback.md) |
| 2026-10-08 | VS-88 deferred: the BGE prompts stay as they are until ADR 019 picks the chunk model. One code path for the prompts and a test that pins them are in. | Download BGE and measure now (a path no profile runs, on a model that may be replaced); change the prompts unmeasured; build a table-clearing rebuild guard now | Owner | [VS-88](tickets/anurup-embeddings-retrieval-2026-10.md) |
| 2026-10-08 | VS-92: a session summary is composed from its moments on read (counts, length, files, the most detailed earlier sentence). No model writes it and no session record is stored. | The on-device model writes a stored session record when a session goes quiet (it invented content on the same job for the briefing, and a stored record can disagree with its moments); a stored record composed by rule (new record type and migration for text that can be derived) | Owner | [VS-92](tickets/anurup-embeddings-retrieval-2026-10.md) |
| 2026-10-06 | ADR-018 option B for Notch Do and Hermes only, on the ChatGPT sign-in; Codex app-server is the agent loop and the only token refresher. | Hermes as the loop; a token proxy | Kunj (lane owner) | [ADR-018 amendment](../decisions/018-reasoning-tier.md) |

## Earlier accepted ADRs (background, before this log)

These carry an "Accepted" status line and still shape the code. They predate the team process above, so they have no "who agreed".

| ADR | Status as written | Note |
|---|---|---|
| [006](../decisions/006-mcp-deployment-modes.md) MCP deployment modes | Accepted (Phase 1, transport hardening) | Its relaxed Local-mode auth is superseded by ADR-017 |
| [008](../decisions/008-parent-child-chunk-rag.md) Parent-child chunk RAG | Accepted | Chunk table is empty on the owner profile (`docs/evidence/W02/VS-01-baseline.md`) |
| [010](../decisions/010-embedding-document-manifest.md) Embedding documents and manifests | Accepted | |
| [011](../decisions/011-event-driven-ui-status.md) Event-driven UI status | Accepted | |
| [012](../decisions/012-required-model-gating.md) Required-model gating | Accepted (Gate 0) | |
| [013](../decisions/013-release-channel-and-auto-update.md) Release channel and auto-update | Accepted (Gate 0) | |

## Open: not decided, do not treat as agreed

| Item | State | Where |
|---|---|---|
| Team charter: decision rights, disagreements, handoff when away | Draft, pending owner approval and each teammate's acknowledgement | PD-17, `docs/team/TEAM.md` section "Team charter" |
| Beta date Wed Oct 21 and Final week of Dec 14 | Assumed; confirm with instructors | Master plan D-4, month plan section 12 row 8 |
| Companion parked until after Final | Assumed | Master plan D-2 |
| No cloud model generates training labels | Proposed | Master plan D-3 |
| Hosted decision models never see captured content | Proposed | Master plan D-8 |
| GitLab is the source of truth; GitHub is a mirror | Proposed | Master plan D-6 |
| Agent write-back is notes only; no pulling from external tools this month | Recommended, with defaults | Month plan section 12 rows 5 and 6 |
