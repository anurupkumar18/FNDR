# ADR 027: Work sets, reopening a thread of work from targets FNDR resolved itself

## Status

Accepted 2026-10-09. The owner approved building it on 2026-10-09.

Requirements: [work sets PRD](../superpowers/specs/2026-10-09-work-sets-prd.md).

## Context

A person asks Notch Do to "pull up everything related to the assignment I was working on". Two rules from ADR 024 stop that today, and both are right for what they guard:

- A link opens without a tap only when the person's own words account for it (item 10). A remembered Canvas URL is never in the words.
- Memories go to a cloud planner only when the person turned that on (E2), and even then as text snippets with no reopen targets.

FNDR already knows, on this Mac, which memories belong to a piece of work and where each reopens. The missing piece is a class of action whose target is not written by a model at all.

## Decision

1. **New action class: reopen a memory.** Notch Do plans `reopen_memory` steps from targets FNDR resolved locally (`workset::resolve`), never from a URL, path or id a model wrote. The planner's schema does not offer the action, and `parse_plan` refuses a model plan that contains it.
2. **Nothing leaves the Mac for resolution.** A work-set request (`plan::asks_for_work_set`, decided on this Mac) makes no cloud call: no Codex session, no planner turn.
3. **The plan card lists every item.** At most 6 items, one per distinct target.
4. **Auto-start** only when every step is a plain reopen with no input and there are at most 3 items. Otherwise the person taps Start (or says "go", as for any plan).
5. **Ambiguity is shown, not guessed.** When the two best sets score within a margin, the notch shows up to three and the person picks one by tap, number or name. A pick is the Start for that set, under the same rule as item 4.
6. **Stop always works.** Before the first item and between items; an item already open stays open, and nothing is undone automatically.
7. **Exclusions.** Memories from Private Mode, private or incognito windows, blocklisted apps or sites, FNDR itself, soft-deleted memories and agent notes are never items. The actions kill switch and Private Mode refuse the request. The blocklist is checked again when each item opens.
8. **Verification is FNDR's.** A step's result is the typed `ReopenOutcome` from the shared reopen core, the same one the Vault uses. A missing file, a disconnected drive or a missing app is a failed step with that reason.

Tier: a reopen runs like `open_memory_source` in ADR 022, because the target is the person's own captured place and FNDR opens it the way the Vault does.

## Options considered

| | A. Let the cloud planner see reopen targets | B. Allow remembered URLs in `link_was_asked_for` | C. Resolve locally and plan reopen steps (chosen) |
|---|---|---|---|
| What leaves the Mac | Titles, URLs, paths | Same as today | Nothing |
| Who picks the target | A model | A model, checked against memory | FNDR's code |
| Works with E2 off | No | Partly | Yes |
| Injection risk | A page title can steer the plan | A model can pick any remembered URL | None from model output |
| Cost | Prompt change, egress review | Policy change in a shared rule | New module, one step kind |

A was rejected because it widens the egress rule ADR 024 just closed. B was rejected because it weakens a rule that guards every other link and still leaves the choice of target to a model.

D, a fourth option, was also weighed: open everything a search returns. Rejected: one search mixes threads, so it opens the wrong document next to the right one.

## Consequences

- `workset::resolve` and `workset::open_items` become the API for any surface that reopens a piece of work.
- Notch Do gains a chooser state.
- Resolution quality is bounded by thread grouping and hybrid search; the first evaluation on a copy of the owner's vault is `docs/evidence/W04/work-sets.md`.
- `operator/policy.rs` is unchanged: a reopen never reaches the computer-use tools.
