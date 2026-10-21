# Kunj: feature ownership proposal

Status: proposal for the lead to accept or amend. Nothing here changes a lane until it is merged into `docs/team/TEAM.md`.

## Features Kunj started

Each row is the first commit that introduced the feature, found with `git log --all --reverse -i -G<term>` on 2026-10-15.

| Feature | First commit | Date | Author |
|---|---|---|---|
| Hermes | `f83dc11e` vendored Hermes bundle staging and the first Hermes types | 2026-04-22 | Kunj |
| Screen Guide | "Add native Screen Guide experience" | 2026-09-08 | Kunj |
| Notch HUD | "Add the notch HUD: ask FNDR from the camera housing" | 2026-09-22 | Kunj |
| Computer use | "Talk to the notch and let FNDR operate the Mac" | 2026-09-24 | Kunj |
| Notch Do | "feat(notch-do): voice-driven computer use, and Hermes on one ChatGPT sign-in" | 2026-10-06 | Kunj |

Later commits by other people extend these features. They do not change who started them.

## Proposed rule

For the five features above, and for the new features below:

1. Kunj is the lane owner. `TEAM.md` already says "Ask the lane owner before changing their area".
2. A change by someone else goes through a merge request that names Kunj as reviewer, or a message to Kunj before work starts.
3. Bug fixes that unblock another lane are allowed without asking, and Kunj is told in the merge request.
4. The lead still decides when two lanes disagree.

Paths this covers:

- `src-tauri/src/operator/`, `src-tauri/src/ipc/commands/computer_use.rs`
- `src-tauri/src/ipc/commands/hermes_agent.rs`, `src-tauri/src/agent/skills.rs`, `src-tauri/src/agent/risk_policy.rs`
- `src/domains/notch/`, `src/domains/screen-guide/`, `src/domains/command-palette/`

## New features to own

Ranked by how much of the Final plan each carries. All three sit in the command surface, skills and local models lane (`kunj-command-skills-models.md`).

### 1. Approve-then-act agent (the Final headline)

The Final deliverable in the master plan is an approve-then-act local agent with three verbs. It builds on GS-09 (approval cards and undo), GS-11 (one risk policy for every action) and GS-12 (screen text cannot trigger actions).

- Done when: an approval queue shows the structured action before it runs, every action has an undo or a stated reason it has none, and an injection suite shows that text on screen never triggers a call.
- Evidence: the injection suite as a `cargo test` target, plus a recording of the three verbs.

### 2. Skills from runs

SK-01 to SK-07: journal every command, save a successful sequence as a `SKILL.md`, review and run skills by name, share them with Claude Code, and run only registry tools inside a skill.

- Done when: a successful Notch Do run can be saved, approved and replayed by name, and a skill cannot call anything outside the registry.
- Note: a skills surface for Notch Do landed on 2026-10-15 under another author. Agree who finishes SK-01 to SK-07 before more work starts.

### 3. Power-aware local model scheduling

LM-03, LM-04, LM-06 and LM-08: wire the enrichment policy into capture, put interactive requests ahead of background work, enrich deferred memories when the Mac is idle and on power, and measure Apple's on-device Foundation Models for structuring and routing.

- Done when: `make capture-baseline` shows the share of memories enriched on power and idle, and interactive latency does not regress.

## Not proposed

The model harness and LoRA adapter work (E-F2 in the master plan) is assigned to the lead. Taking it needs his agreement first.

## Open questions

- Does the lead accept the path list above, or does he want a narrower one?
- Who finishes SK-01 to SK-07 now that skills in Settings exist?
