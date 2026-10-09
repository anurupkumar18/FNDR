# PRD: Work sets

Decision record: [ADR 027](../../decisions/027-work-sets.md).

## Problem

"Pull up everything related to the assignment I was working on" cannot work in Notch Do today. The planner may open only links the person said aloud (`operator/plan.rs`, `link_was_asked_for`), so a Canvas page FNDR remembers is never opened, and `operator/memory.rs` sends the cloud planner text snippets with no reopen targets, and only when egress switch E2 is on. FNDR already holds everything needed: each memory carries a typed reopen target (`memory/reopen.rs`), `reopen_memory` opens one with moved-file lookup and a Finder reveal for installers, Resume groups memories into threads, and tasks cite the memories they came from.

## Goal

One spoken or typed request opens every remembered target of one thread of work (pages, a PDF at its page, documents, files and folders, apps), resolved on this Mac, with the items listed before anything opens and Stop working throughout.

## Users / actors

The person at the Mac, through Notch Do. Next wave: any surface that wants "reopen this piece of work" (Home, Vault, the command bar) through the same Rust API.

## Current behavior

- Notch Do sends every request to a Codex planner turn. A remembered link waits for a tap or is never planned.
- `reopen_memory` opens one memory from the Vault.

## Proposed behavior

1. A detector decided on this Mac (`plan::asks_for_work_set`, beside `refers_to_past`) recognizes work-set requests: "pull up / open / set up / bring up / get out everything related to X", "the assignment I was working on".
2. For those, Notch Do makes no cloud call. `workset::resolve` ranks candidate threads from Resume threads of the last seven days, open tasks (a due date within three days raises a thread) and a hybrid search of the request's topic words, and returns one of:
   - `Best(WorkSet)`: the plan card lists every item as a `reopen_memory` step;
   - `Ambiguous(Vec<WorkSet>)`: the notch shows up to three sets; the person taps one or says its number or name;
   - `None { why }`: the notch says why.
3. A set holds at most 6 items, one per distinct target (newest first; when two memories share a target, the higher `reopen_rank` wins). It pulls in a thread from another app only when the evidence ties it: within ten minutes and a shared file or two distinctive title words (the Vault's session-link rule), or within thirty minutes and itself a match for the request.
4. The plan starts by itself only when every step is a plain reopen and there are at most 3; otherwise the person taps Start (or says "go").
5. Each step is checked by FNDR from the typed `ReopenOutcome` (opened, opened from its new place, app only, missing, drive not connected, app missing, blocked), never from a model's report. A failed item does not stop the others.
6. Narration says what is opening in plain words: "Opening your Canvas page, the PDF on page 4 and the doc."

## Non-goals

- Restoring window positions or tab order.
- Closing anything, or undoing an opened item.
- Sending any memory, title or target to a cloud model for resolution.
- Changing how `operator/policy.rs` classifies the existing computer-use tools.

## Functional requirements

- FR1: Memories from Private Mode, private or incognito windows, blocklisted apps or sites, FNDR itself, soft-deleted memories and agent notes never become items. The blocklist is checked again when an item opens.
- FR2: A request is refused while the actions kill switch is on or Private Mode is on.
- FR3: The planner's JSON schema does not offer `reopen_memory`; a model-written plan that contains it is refused.
- FR4: `resolve` is deterministic for the same inputs: ties break on recency, then on id.
- FR5: `open_work_set(memory_ids)` re-checks every id and opens at most 6.
- FR6: Every opening is written to the Notch Do journal with its outcome.

## Non-functional requirements

- Performance: resolution reads seven days of memories, open tasks and one hybrid search; no model call.
- Security/privacy: nothing leaves the Mac for resolution; a reopen target comes from capture, never from model output.
- Accessibility: chooser options are buttons with their number and title; voice can pick by number or name.
- Maintainability: ranking and dedupe are pure functions over `MemoryRecord` fixtures, separate from the store.

## Domain language

| Term | Meaning | Existing code/docs |
|---|---|---|
| Work set | The reopenable targets of one thread of work | `src-tauri/src/workset/` |
| Reopen target | The typed place a memory reopens to | `memory/reopen.rs`, `ReopenTarget` |
| Thread | Resume's grouping of memories by project, session, domain or app | `resume/mod.rs` |
| Session link | Two sessions in other apps tied by a shared file or title words within ten minutes | `src/domains/memory-vault/sessionLinks.ts` |

## Affected modules and interfaces

| Module | Change | Interface impact | Tests |
|---|---|---|---|
| `workset/` (new) | Resolve and open | `resolve`, `open_items`, `rank` | Ranking, dedupe, cap, margin, exclusions, fake opener |
| `ipc/commands/memory.rs` | Reopen core shared | `reopen_record` (crate) | Existing reopen tests unchanged |
| `ipc/commands/work_set.rs` (new) | IPC | `resolve_work_set`, `open_work_set` | Through the shared core |
| `operator/plan.rs` | `ReopenMemory` step, detector, verdict from outcome | `StepAction::ReopenMemory` | Detector phrases, verdicts, refusal |
| `ipc/commands/computer_use.rs` | Work-set run without Codex | `Choose` event | Plan building, auto-start rule |
| `src/domains/notch/` | Chooser, narration | `choose` phase | Reducer and narration tests |

## Acceptance criteria

- [ ] "pull up everything related to the assignment I was working on" plans reopen steps with no Codex session.
- [ ] Two equally strong threads produce a chooser, never a guess.
- [ ] More than 3 items wait for Start; 3 or fewer start by themselves.
- [ ] A private, incognito or blocklisted memory never appears.
- [ ] Ten realistic queries are evaluated read-only on a copy of the owner's vault, with hits and misses recorded.

## Test plan

Unit tests on fixture `MemoryRecord`s for ranking and dedupe; a store-backed test with a fake opener; detector phrase tables; reducer and narration tests in Vitest; the ignored `work_set_eval_on_a_vault_copy` test run against a copy of the vault.

## Rollout

Inside Notch Do, which stays in Labs (ADR 024, E12). The IPC commands are available to the next wave of surfaces.

## Risks

- Threads keyed by session split one assignment across apps; the link rule may miss a document whose title shares one word with the page.
- Hybrid search quality bounds resolution quality.

## Open questions

- Whether a chooser should remember the person's pick for the same words next time.
