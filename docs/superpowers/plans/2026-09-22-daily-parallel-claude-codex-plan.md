# Daily Parallel Plan: Claude + Codex, 2026-09-22

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close out yesterday's finished-but-unrecorded work, push the GitLab board forward on a curated set of ready tickets, and land two new ambitious features — an approve-then-act "Context Coach" (Clicky-inspired) and a local typed-decision engine with a real Laya-backed decider (Laya-inspired) — as two parallel, mostly-independent workstreams that both integrate through `main`.

**Architecture:** Two agents (Claude, Codex) work the same repo at the same time. Almost everything lands on `main` directly (small tickets, bookkeeping). Exactly two long-lived-for-a-day branches exist so each agent's multi-hour feature work doesn't destabilize `main` mid-flight: `feat/context-coach` (Claude) and `feat/decision-engine-laya` (Codex). Each branch merges back to `main` after every task, not once at the end of the day — this is what keeps integration and QA cheap, per the owner's explicit request.

**Tech Stack:** Rust (Tauri backend, `src-tauri/`), TypeScript/React (`src/`), Python (dev scripts + one new runtime sidecar).

**Spec:** `docs/superpowers/plans/2026-09-21-beta-final-master-plan.md` (full manifest and Final-epic roadmap), `2026-09-21-ws1-capture-pipeline-teardown.md` (CAP-07), `2026-09-21-ws3-feature-thesis-and-research.md` (FEA-05, and Task 6's E-F4 "Approve-then-act agent" placeholder that this plan pulls a safe slice of forward), `2026-09-21-ws6-bounded-decisions-and-fast-local-models.md` (DEC-01, DEC-02). This plan does not restate those tasks — it points at them and only writes new task detail for work no existing doc covers.

## Global Constraints

- No em dashes or en dashes anywhere: chat, docs, tickets, commit messages.
- No AI/Claude/Codex co-author trailers on commits, PRs, or tickets. Commits under the owner's own git identity.
- TDD discipline: read the real current code before writing a test, write a failing test, confirm the failure reason, implement minimally, confirm it passes, run the full suite before committing.
- Branch policy for today: `main` for anything small/single-file (bookkeeping, board hygiene, quick tickets). `feat/context-coach` and `feat/decision-engine-laya` for the two feature workstreams only. No other branches. Merge each task's work back to `main` as soon as it's green, don't batch a whole day of commits into one end-of-day merge.
- Board writes: Claude does all GitLab issue/board writes today, for both agents' work, using the existing `claude-code-2026-09` bot token. Codex stays push-only (confirmed working via SSH already) and reports what it finished; Claude moves the ticket's status label and closes it. This avoids provisioning new GitLab tokens today and keeps the write-audit trail single-actor, while the commit history (which is what actually matters for authorship) stays correctly attributed to the owner either way.
- `cargo fmt -- <file>` has previously reformatted unrelated files. Use `rustfmt <specific files>` directly, check `git status`/`git diff --stat` before committing.
- Safety-critical constraint for the Context Coach workstream specifically: an action's command/arguments must never be derived from OCR'd screen text or other untrusted observed content. The master plan's own Task 6 (E-F4) requires adversarial injection-fixture testing before screen-grounded proposals ship — today's slice deliberately stays inside that boundary by sourcing action input only from an explicit, user-typed goal.

---

## Part 0: Shared bookkeeping (either agent, do first, ~30-45 min)

Four pieces of already-finished work from yesterday are code-complete on `main` but not reflected on the GitLab board, and two of them are missing evidence. Close these out before starting new work so the board actually reflects reality.

### Task 0.1: Close MOD-02 and the extraction-token-budget fix

Both are fully verified already (`full make test passed`, commits `7400dc6`, `3c4491e`, `346c834`, `ad3bcf2`, `b0e72c8` on `main`). No new work needed.

- [ ] **Step 1:** Fetch issue #88 (MOD-02) and its matching extraction-cap issue via the GitLab API, confirm the commits above are the ones referenced.
- [ ] **Step 2:** Post a closing note on each issue with the commit shas and the verification line from Codex's handoff report (`make test` output), then set `state_event=close`.
- [ ] **Step 3:** Move each issue's `status::` label to `status::evidence` before closing (skip `status::doing`/`status::review` since the work already happened) — attach the verification text as the evidence note rather than a separate file, since there's no numeric measurement to tabulate.

### Task 0.2: Produce CAP-05 and MEM-03 evidence, then close both

Both are code-complete (commits `552528f`, `92aced8`) but explicitly flagged by Codex as needing "a real operator session with Accessibility enabled" and "a real metrics-dump session." Do this as one combined live session, since both evidence needs are satisfied by the same run.

- [ ] **Step 1:** Confirm Accessibility permission is already granted to the dev build (System Settings > Privacy & Security > Accessibility) — grant it if this is a fresh machine state.
- [ ] **Step 2:** `git checkout main && git pull --ff-only`, `cd src-tauri && cargo build`.
- [ ] **Step 3:** Launch with the metrics dump enabled: `FNDR_METRICS_DUMP=/tmp/fndr-baseline.ndjson npm run tauri dev` (from repo root), and use the app normally (switch apps, browse, code) for at least 10-15 minutes so `capture.context_ms` gets enough samples for a meaningful p50/p95 and MEM-03's `mem.*` counters accumulate real values.
- [ ] **Step 4:** Stop the app, run `make capture-baseline METRICS=/tmp/fndr-baseline.ndjson OUT=docs/evidence/W03/cap-05-context-baseline.md --machine "$(sysctl -n machdep.cpu.brand_string), $(( $(sysctl -n hw.memsize) / 1073741824 )) GB"` (per CAP-02's Makefile target). This produces both CAP-05's `capture.context_ms` before/after story (before = yesterday's AppleScript-based numbers already in git history if available, after = today's Accessibility-based numbers) and MEM-03's `mem.*` counters in one report.
- [ ] **Step 5:** Copy the same report (or a short excerpt) into `docs/evidence/W03/mem-03-post-capture-counters.md` for MEM-03's separate evidence link, since it's a different ticket.
- [ ] **Step 6:** Commit both evidence files directly to `main`: `git add docs/evidence/W03/*.md && git commit -m "docs(evidence): CAP-05 accessibility baseline and MEM-03 post-capture counters"`.
- [ ] **Step 7:** On GitLab, attach both evidence files' paths to issues #83 (CAP-05) and #105 (MEM-03), move `status::evidence`, close both.

### Task 0.3: Flag the polluted duplicate branches

`feat/mod-02-llm-traces` and `fix/extraction-token-budget` (containing the stray `0465dac` commit) are superseded by what's already on `main`. Don't delete them without the owner's say-so (shared remote branches) — post a one-line note wherever the team tracks this (or just mention it in today's end-of-day report) so nobody builds on the stale ones by accident.

---

## Part 1: Claude lane — Context Coach (branch `feat/context-coach`)

**Why this and not something else:** the Explore-agent audit found FNDR already has almost everything a Clicky-style "AI buddy that watches your screen and points at things" needs, just disconnected: Screen Guide already does grounded screen Q&A with a `[POINT:x,y:label]` mechanism nearly identical to Clicky's (`src-tauri/src/inference/mod.rs:89-98`, `src-tauri/src/ipc/commands/screen_guide.rs:2949-2971` for the grounding-against-observed-OCR step), and the Agent system already has a fully-typed propose/approve/execute/audit skeleton (`src-tauri/src/agent/{actions,policy,approvals,execution,audit}.rs`) that is never called from any Tauri command. The master plan already names this feature — Task 6, "Approve-then-act agent (E-F4)" — but schedules the full adversarial-tested version for weeks 6-9. Today's work is a safe, honest slice: wire the mechanics end to end for one action kind, with the action's input coming only from something the user typed, not from parsed screen content. That's real, demoable, resume-relevant ("policy-gated computer use, approval UX, telemetry" per Codex's own framing) without touching the harder injection-resistant problem the plan correctly defers.

### Task 1: CAP-07 — Privacy Proof data over IPC

**Files:** per `docs/superpowers/plans/2026-09-21-ws1-capture-pipeline-teardown.md`, search for `CAP-07`.

- [ ] Follow that plan's steps exactly (new module, one IPC command, `http_util` egress counter). Branch: work this directly on `main` per today's policy (it's a single new module + one command, not a multi-hour feature) rather than the plan's own suggested `feat/cap-07-privacy-proof-data-over` branch name — small enough to go straight in.
- [ ] Verify: `cargo test privacy_proof` passes, `get_privacy_proof` returns counts and no content.
- [ ] Evidence to `docs/evidence/W03/`, close issue #84, unblocks FEA-05 next.

### Task 2: FEA-05 — Privacy Proof screen

**Files:** per `docs/superpowers/plans/2026-09-21-ws3-feature-thesis-and-research.md`, Task 4 (`src/domains/privacy-proof/PrivacyProof.tsx` + test — the task doc has the complete test and component code already).

- [ ] Follow that task's steps exactly. Straight to `main` (single small component + test).
- [ ] Verify: `npm test -- privacy-proof` passes.
- [ ] Close the matching issue once merged (check the manifest for its number if it's separate from FEA-03's).

### Task 3: Approve-then-act MVP slice (new — not in any existing plan doc)

**Files:**
- Modify: `src-tauri/src/ipc/commands/agent.rs` (add three commands)
- Modify: `src-tauri/src/main.rs` (register the three commands in `generate_handler!`)
- Modify: `src/shared/ipc/tauri.ts` (add three wrapper functions + types, following the existing `runAgentRequest` pattern)
- Modify: `src/domains/workspace/AgentPanel.tsx` (add a "Propose an action" form + approve/execute buttons, gated to `mode === "act"`)
- Test: `src-tauri/src/agent/actions.rs` (extend existing `#[cfg(test)]` module), `src/domains/workspace/AgentPanel.test.tsx`

**Interfaces:**
- Consumes: `AgentAction`, `AgentActionKind`, `AgentActionStatus`, `ActionResult` (`agent/actions.rs`), `policy_for_action(kind: &AgentActionKind, risk_level: &RiskLevel, mode: &AgentMode) -> ActionPolicyDecision` (`agent/actions.rs:96`), `append_agent_action`/`get_agent_action_by_id`/`update_action_status`/`list_actions_for_run` (`agent/audit.rs:391-450`), `is_action_approved(app_data_dir, action_id) -> Result<bool, String>` (`agent/approvals.rs`), `execute_action(action: &AgentAction) -> Result<ActionResult, String>` (`agent/execution.rs:126`, currently only handles `RunReadOnlyCommand` — use this kind for the MVP, not `OpenFile`, since it's the one with real execution logic today).
- Produces: three new IPC commands `propose_agent_action`, `approve_agent_action`, `execute_agent_action`, described below.

- [ ] **Step 1: Write the failing Rust test for the propose step**

Add to `src-tauri/src/agent/actions.rs`:

```rust
#[cfg(test)]
mod approve_then_act_tests {
    use super::*;

    #[test]
    fn a_medium_risk_readonly_command_in_act_mode_requires_approval() {
        let decision = policy_for_action(
            &AgentActionKind::RunReadOnlyCommand,
            &RiskLevel::Medium,
            &AgentMode::Act,
        );
        assert!(decision.allowed);
        assert!(decision.requires_approval);
    }

    #[test]
    fn ask_mode_blocks_every_action_kind() {
        let decision = policy_for_action(
            &AgentActionKind::RunReadOnlyCommand,
            &RiskLevel::Low,
            &AgentMode::Ask,
        );
        assert!(!decision.allowed);
    }
}
```

- [ ] **Step 2: Run to verify it passes immediately**

Run: `cd src-tauri && cargo test approve_then_act_tests`
Expected: PASS — `policy_for_action` already exists and does this correctly; this step just proves the fact before building the IPC layer on top of it. If it fails, the policy table changed since this plan was written; re-read `agent/actions.rs:96-160` before continuing.

- [ ] **Step 3: Write the failing IPC command tests**

Add near the bottom of `src-tauri/src/ipc/commands/agent.rs` (new `#[cfg(test)]` module — check whether one exists first; if IPC command modules in this codebase don't unit-test Tauri commands directly, test the underlying logic function instead of the `#[tauri::command]` wrapper, matching whatever pattern `capture/mod.rs`'s tests use for its `#[tauri::command]`-adjacent functions):

```rust
#[cfg(test)]
mod action_lifecycle_tests {
    use super::*;
    use crate::agent::actions::{AgentAction, AgentActionKind, AgentActionStatus};
    use crate::agent::policy::RiskLevel;
    use tempfile::tempdir;

    #[tokio::test]
    async fn propose_then_approve_then_execute_runs_the_command_and_records_the_result() {
        let dir = tempdir().unwrap();
        let action = propose_action_logic(
            dir.path(),
            "run-1",
            AgentActionKind::RunReadOnlyCommand,
            RiskLevel::Medium,
            "Check repo status",
            serde_json::json!({"command": "git", "args": ["status"]}),
        )
        .unwrap();
        assert_eq!(action.status, AgentActionStatus::NeedsApproval);

        let approved = approve_action_logic(dir.path(), &action.id).unwrap();
        assert_eq!(approved.status, AgentActionStatus::Approved);

        let executed = execute_action_logic(dir.path(), &action.id).await.unwrap();
        assert_eq!(executed.status, AgentActionStatus::Succeeded);
        assert!(executed.result.unwrap().success);
    }

    #[tokio::test]
    async fn executing_before_approval_is_rejected() {
        let dir = tempdir().unwrap();
        let action = propose_action_logic(
            dir.path(),
            "run-2",
            AgentActionKind::RunReadOnlyCommand,
            RiskLevel::Medium,
            "Check repo status",
            serde_json::json!({"command": "git", "args": ["status"]}),
        )
        .unwrap();
        let result = execute_action_logic(dir.path(), &action.id).await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 4: Run to verify it fails**

Run: `cd src-tauri && cargo test action_lifecycle_tests`
Expected: FAIL with "cannot find function `propose_action_logic`" (and the two others) — they don't exist yet.

- [ ] **Step 5: Implement the three logic functions and their IPC command wrappers**

In `src-tauri/src/ipc/commands/agent.rs`, add:

```rust
use crate::agent::actions::{policy_for_action, AgentAction, AgentActionKind, AgentActionStatus};
use crate::agent::approvals::is_action_approved;
use crate::agent::audit::{append_agent_action, get_agent_action_by_id, update_action_status};
use crate::agent::execution::execute_action;
use crate::agent::policy::RiskLevel;
use std::path::Path;

fn propose_action_logic(
    app_data_dir: &Path,
    run_id: &str,
    kind: AgentActionKind,
    risk_level: RiskLevel,
    description: &str,
    input: serde_json::Value,
) -> Result<AgentAction, String> {
    let decision = policy_for_action(&kind, &risk_level, &AgentMode::Act);
    if !decision.allowed {
        return Err(decision.blocked_because.unwrap_or(decision.reason));
    }
    let input_map: std::collections::BTreeMap<String, serde_json::Value> = input
        .as_object()
        .ok_or("input must be a JSON object")?
        .clone()
        .into_iter()
        .collect();
    let status = if decision.requires_approval {
        AgentActionStatus::NeedsApproval
    } else {
        AgentActionStatus::Approved
    };
    let action = AgentAction {
        run_id: run_id.to_string(),
        title: description.to_string(),
        description: description.to_string(),
        kind,
        input: input_map,
        risk_level,
        status,
        ..Default::default()
    };
    append_agent_action(app_data_dir, &action)?;
    Ok(action)
}

fn approve_action_logic(app_data_dir: &Path, action_id: &str) -> Result<AgentAction, String> {
    update_action_status(
        app_data_dir,
        action_id,
        AgentActionStatus::Approved,
        Some(chrono::Utc::now().timestamp()),
        None,
        None,
    )
}

async fn execute_action_logic(app_data_dir: &Path, action_id: &str) -> Result<AgentAction, String> {
    if !is_action_approved(app_data_dir, action_id)? {
        return Err(format!("Action {} has not been approved", action_id));
    }
    let action = get_agent_action_by_id(app_data_dir, action_id)?
        .ok_or_else(|| format!("Action not found: {}", action_id))?;
    let result = execute_action(&action).await;
    let (status, result) = match result {
        Ok(r) => (AgentActionStatus::Succeeded, Some(r)),
        Err(e) => (
            AgentActionStatus::Failed,
            Some(crate::agent::actions::ActionResult {
                success: false,
                output: String::new(),
                error: Some(e),
                duration_ms: 0,
            }),
        ),
    };
    update_action_status(
        app_data_dir,
        action_id,
        status,
        None,
        Some(chrono::Utc::now().timestamp()),
        result,
    )
}

#[tauri::command]
pub async fn propose_agent_action(
    state: State<'_, Arc<AppState>>,
    run_id: String,
    kind: AgentActionKind,
    risk_level: RiskLevel,
    description: String,
    input: serde_json::Value,
) -> Result<AgentAction, String> {
    propose_action_logic(
        state.inner().app_data_dir.as_path(),
        &run_id,
        kind,
        risk_level,
        &description,
        input,
    )
}

#[tauri::command]
pub async fn approve_agent_action(
    state: State<'_, Arc<AppState>>,
    action_id: String,
) -> Result<AgentAction, String> {
    approve_action_logic(state.inner().app_data_dir.as_path(), &action_id)
}

#[tauri::command]
pub async fn execute_agent_action(
    state: State<'_, Arc<AppState>>,
    action_id: String,
) -> Result<AgentAction, String> {
    execute_action_logic(state.inner().app_data_dir.as_path(), &action_id).await
}
```

Check `AgentActionKind`, `RiskLevel`, and `serde_json::Value` all derive/implement whatever Tauri's `generate_handler!` + `specta` (this codebase uses `specta` per the earlier build's `--extern specta` flag) require for command arguments — if `cargo build` complains about missing `Type`/`serde::Deserialize` derives on `AgentActionKind` or `RiskLevel`, add them (check what `AgentMode` already derives, in `agent/policy.rs`, and match it).

- [ ] **Step 6: Register the commands in `main.rs`**

Add `ipc::commands::propose_agent_action, ipc::commands::approve_agent_action, ipc::commands::execute_agent_action,` next to the existing `ipc::commands::run_agent_request` line (`main.rs:780`).

- [ ] **Step 7: Run to verify the Rust tests pass**

Run: `cd src-tauri && cargo test action_lifecycle_tests`
Expected: `2 passed`.

- [ ] **Step 8: Run the full Rust suite to confirm nothing else broke**

Run: `cd src-tauri && cargo test --lib`
Expected: all passing, count higher than before by however many new tests were added.

- [ ] **Step 9: Commit the backend**

```bash
git add src-tauri/src/ipc/commands/agent.rs src-tauri/src/main.rs
git commit -m "feat(agent): wire propose, approve, execute for read-only command actions"
```

- [ ] **Step 10: Add the TS wrapper functions**

In `src/shared/ipc/tauri.ts`, near the existing `runAgentRequest` (search for it), add:

```typescript
export interface AgentAction {
    id: string;
    run_id: string;
    title: string;
    description: string;
    kind: string;
    status: "proposed" | "needs_approval" | "approved" | "running" | "succeeded" | "failed" | "blocked" | "cancelled";
    result: { success: boolean; output: string; error: string | null; duration_ms: number } | null;
}

export async function proposeAgentAction(
    runId: string,
    kind: string,
    riskLevel: "low" | "medium" | "high" | "blocked",
    description: string,
    input: Record<string, unknown>,
): Promise<AgentAction> {
    return invoke<AgentAction>("propose_agent_action", { runId, kind, riskLevel, description, input });
}

export async function approveAgentAction(actionId: string): Promise<AgentAction> {
    return invoke<AgentAction>("approve_agent_action", { actionId });
}

export async function executeAgentAction(actionId: string): Promise<AgentAction> {
    return invoke<AgentAction>("execute_agent_action", { actionId });
}
```

- [ ] **Step 11: Write the failing frontend test**

In `src/domains/workspace/AgentPanel.test.tsx` (check the existing test file's setup/mocking pattern for `runAgentRequest` first, and mirror it exactly):

```tsx
it("proposes, approves, and executes a read-only command action in act mode", async () => {
    vi.mocked(proposeAgentAction).mockResolvedValue({
        id: "a1", run_id: "r1", title: "Check status", description: "Check status",
        kind: "run_read_only_command", status: "needs_approval", result: null,
    });
    vi.mocked(approveAgentAction).mockResolvedValue({
        id: "a1", run_id: "r1", title: "Check status", description: "Check status",
        kind: "run_read_only_command", status: "approved", result: null,
    });
    vi.mocked(executeAgentAction).mockResolvedValue({
        id: "a1", run_id: "r1", title: "Check status", description: "Check status",
        kind: "run_read_only_command", status: "succeeded",
        result: { success: true, output: "On branch main", error: null, duration_ms: 42 },
    });

    render(<AgentPanel isVisible onClose={() => {}} />);
    // ... drive the propose form, click Approve, click Run, assert "On branch main" renders.
    // Fill in exact selectors once Step 12's markup exists — this test is written first
    // per TDD, so its selectors are a contract for Step 12 to satisfy, not a guess.
});
```

- [ ] **Step 12: Implement the UI**

In `AgentPanel.tsx`'s Act-mode branch (near where `agentMode === "act"` is checked around the mode select, line ~750), add a small form: a text input for the goal/description, a fixed dropdown of demo-safe read-only commands (`git status`, `cargo check`, `npm run typecheck` — matching `execution.rs`'s allowlist exactly, so nothing proposed can fail policy), a "Propose" button calling `proposeAgentAction`, and once a `NeedsApproval` action exists in state, "Approve" and "Run" buttons calling `approveAgentAction`/`executeAgentAction` in sequence, rendering the `result.output` when done.

- [ ] **Step 13: Run to verify the frontend test passes**

Run: `npm test -- AgentPanel`
Expected: PASS.

- [ ] **Step 14: Full verification and commit**

Run: `npm run typecheck && npm test && cd src-tauri && cargo test --lib`
Expected: all clean.

```bash
git add src/shared/ipc/tauri.ts src/domains/workspace/AgentPanel.tsx src/domains/workspace/AgentPanel.test.tsx
git commit -m "feat(agent): approve-then-act UI for read-only command actions"
```

- [ ] **Step 15: Merge to main**

```bash
git checkout main && git pull --ff-only && git merge --no-ff feat/context-coach
git push origin main
```

### Task 4: File backlog tickets for the rest of E-F4

The full "Approve-then-act agent" scope (screen-grounded proposals, the ten adversarial injection fixtures, `CopyText`/`PasteText` kinds, a real approval-queue UI domain, a kill switch) stays exactly where the master plan put it — W6-9, decomposed at the Beta retro. Don't build it today. Do file two GitLab issues so today's MVP is visibly the foundation for that later work rather than a one-off:

- [ ] File an issue titled `[E-F4] Approve-then-act: screen-grounded proposals with injection fixtures`, description pointing at `ws3` plan Task 6 verbatim, `status::ready`, phase `final` (not `beta`).
- [ ] File an issue titled `[E-F4] Approve-then-act: approval-queue UI and kill switch`, same phase/pointer.

---

## Part 2: Codex lane — Decision Engine + Laya (branch `feat/decision-engine-laya`)

**Why this and not something else:** DEC-01/DEC-02 are already fully speced (ws6 plan) around a `Decider` trait and a confidence-cascade architecture (cheap tier tried first, escalate only when unsure) — and Laya (the model Codex researched yesterday) is a drop-in fit for that exact shape: typed `choice`/`score`/`noul` outputs with trained-calibrated confidence, in a single forward pass, no text generation to hallucinate. Land DEC-01/DEC-02 as speced first (they're self-contained, don't actually need Laya), then spend the stretch time proving a *real* `Decider` backed by Laya against the plan's own fake baseline, on real M1-8GB numbers rather than Laya's documented T4 numbers.

### Task 1: DEC-01 — Decision inventory and three pilots

**Files:** per `docs/superpowers/plans/2026-09-21-ws6-bounded-decisions-and-fast-local-models.md`, Task 1 (full test code and implementation already written in that doc).

- [ ] Follow that task's five steps exactly (`scripts/model/rank_decisions.py` + test, then the owner fills `docs/product/decision-inventory.md` with the team, scoring the six candidate decisions already listed in that plan's section 6 table, naming three pilots).
- [ ] Verify: `python3 scripts/model/test_rank_decisions.py` passes (2 tests).
- [ ] Branch: `feat/decision-engine-laya` (not the plan's suggested `docs/dec-01-decision-inventory` name — everything for today's Codex lane stays on one branch).
- [ ] Close issue #114 once merged, evidence = the inventory doc.

### Task 2: DEC-02 — Decision core, calibration, coverage-precision report

**Files:** per the same ws6 plan, Task 2 (complete test code and implementation already written, including the `Decider`/`Tier`/`cascade`/`Resolution` Rust types and their 4 tests, and the pre-tested Python calibration/report tools).

- [ ] Follow that task's steps exactly: copy the four already-tested Python files from `docs/superpowers/plans/assets/2026-09-21/ws6/` into `scripts/model/`, run their tests (`11 passed`, `4 passed`), then write `src-tauri/src/decision/mod.rs` with the `Decider` trait, `DecisionOutcome`, `Tier`, `Resolution`, `cascade()`, and the `Fixed` test double (already-written test code is in the plan doc's Task 2 Step 2).
- [ ] Note: the ticket's "must be closed before you start: DEC-01, MOD-05" is a schedule-ordering note in the master plan's week sequencing, not a code dependency — DEC-02's actual task content is self-contained and doesn't call anything from MOD-05 (the eval runner). Doing DEC-01 -> DEC-02 today without MOD-05 is a deliberate, low-risk reordering for today's plan; flag it in the end-of-day report rather than silently skip the note.
- [ ] Verify: `python3 scripts/model/test_decision_calibration.py` and `test_decision_report.py` pass, `cargo test` in the new `decision` module passes (4 tests), a coverage-vs-precision table exists for one pilot in `docs/evidence/W03/decision-pilot-report.md`.
- [ ] Merge to `main` from `feat/decision-engine-laya` once green (same pattern as Task 1's step 15 above). Close issue #115.

### Task 3: Laya sidecar spike (new — not in any existing plan doc)

Laya is Python-only (`pip install laya`), no Rust bindings, no pre-existing ONNX export found (checked: FNDR's other ONNX models are pre-converted `Xenova/*` HF mirrors, and there's no in-repo export tooling to make one for Laya). The codebase already has a working template for exactly this situation: `src-tauri/src/speech.rs` runs Whisper/Orpheus through a per-user venv + subprocess sidecar (`speech_venv_dir()` -> `~/Documents/FNDR Speech/venv`, `find_python3()`, `ensure_venv_ready()`, `Command::new(python).arg(sidecar_script)...`). Mirror that pattern exactly rather than inventing a new one.

**Files:**
- Create: `src-tauri/sidecar/laya_decide_runner.py`
- Create: `src-tauri/src/decision/laya_bridge.rs`
- Modify: `src-tauri/src/decision/mod.rs` (add `pub mod laya_bridge;`, add a `LayaDecider` implementing `Decider`)
- Modify: `src-tauri/Cargo.toml` if `tempfile` isn't already a dev-dependency (check first)
- Test: `src-tauri/src/decision/laya_bridge.rs` (`#[cfg(test)]`)

**Interfaces:**
- Consumes: `Decider` trait, `DecisionOutcome` (Task 2, `decision/mod.rs`).
- Produces: `laya_venv_dir() -> Option<PathBuf>`, `ensure_laya_venv_ready() -> Result<PathBuf, String>`, `run_laya_predict(state: &serde_json::Value, questions: &serde_json::Value) -> Result<serde_json::Value, String>`, `struct LayaDecider { question_key: &'static str, options: &'static [&'static str] }` implementing `Decider`.

- [ ] **Step 1: Write the failing test for the sidecar round-trip**

`src-tauri/src/decision/laya_bridge.rs`:

```rust
use std::path::PathBuf;
use std::process::Command;

pub fn laya_venv_dir() -> Option<PathBuf> {
    dirs::document_dir().map(|root| root.join("FNDR Laya").join("venv"))
}

fn pip_for_venv(venv_dir: &std::path::Path) -> PathBuf {
    venv_dir.join("bin").join("pip")
}

fn python_for_venv(venv_dir: &std::path::Path) -> PathBuf {
    venv_dir.join("bin").join("python3")
}

fn find_python3() -> Option<PathBuf> {
    let candidates: &[&str] = &[
        "/opt/homebrew/bin/python3.13", "/opt/homebrew/bin/python3.12",
        "/opt/homebrew/bin/python3.11", "/opt/homebrew/bin/python3.10",
        "/usr/local/bin/python3.13", "/usr/local/bin/python3.12",
        "/usr/bin/python3", "python3",
    ];
    candidates.iter().map(PathBuf::from).find(|c| {
        Command::new(c).arg("--version").stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
    })
}

pub fn ensure_laya_venv_ready() -> Result<PathBuf, String> {
    let venv_dir = laya_venv_dir().ok_or("Could not determine Documents directory")?;
    if !python_for_venv(&venv_dir).exists() {
        let python3 = find_python3().ok_or("python3 is required for the Laya decider")?;
        if let Some(parent) = venv_dir.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let status = Command::new(&python3)
            .args(["-m", "venv", &venv_dir.to_string_lossy()])
            .status().map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Failed creating FNDR Laya venv".to_string());
        }
        let pip = pip_for_venv(&venv_dir);
        let install = Command::new(&pip).args(["install", "laya"]).status().map_err(|e| e.to_string())?;
        if !install.success() {
            return Err("Failed installing laya into the venv".to_string());
        }
    }
    Ok(venv_dir)
}

pub fn run_laya_predict(
    state: &serde_json::Value,
    questions: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let venv_dir = ensure_laya_venv_ready()?;
    let python = python_for_venv(&venv_dir);
    let dev_sidecar = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sidecar").join("laya_decide_runner.py");
    let payload = serde_json::json!({"state": state, "questions": questions}).to_string();
    let output = Command::new(&python)
        .arg(&dev_sidecar)
        .arg(&payload)
        .output()
        .map_err(|e| format!("Failed to run laya sidecar: {}", e))?;
    if !output.status.success() {
        return Err(format!("laya sidecar failed: {}", String::from_utf8_lossy(&output.stderr)));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("Bad laya sidecar output: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // requires network + a real venv; run explicitly during the spike, not in CI
    fn laya_predict_returns_a_choice_and_confidence_for_a_simple_routing_question() {
        let state = serde_json::json!({"body": "We were billed twice, please refund today."});
        let questions = serde_json::json!({
            "department": {
                "type": "choice",
                "instructions": "Which department should handle this?",
                "criteria": {"billing": "invoices, payments, refunds", "technical": "bugs, outages"}
            }
        });
        let result = run_laya_predict(&state, &questions).unwrap();
        let choice = result["answers"]["department"]["choice"].as_str().unwrap();
        assert_eq!(choice, "billing");
    }
}
```

- [ ] **Step 2: Run to verify it fails for the right reason**

Run: `cd src-tauri && cargo test laya_predict_returns -- --ignored`
Expected: FAIL — `laya_decide_runner.py` doesn't exist yet (`No such file or directory`), not a Rust compile error. This confirms the Rust side type-checks correctly before the Python side exists.

- [ ] **Step 3: Write the sidecar script**

`src-tauri/sidecar/laya_decide_runner.py`:

```python
#!/usr/bin/env python3
"""Run one Laya prediction. Usage: laya_decide_runner.py '<json payload>'
Payload: {"state": {...}, "questions": {...}}. Prints the Laya result JSON to stdout."""
import json
import sys

from laya import Router


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: laya_decide_runner.py '<json payload>'", file=sys.stderr)
        return 1
    payload = json.loads(sys.argv[1])
    router = Router(preload=True)
    result = router.predict(payload["state"], payload["questions"])
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd src-tauri && cargo test laya_predict_returns -- --ignored`
Expected: PASS (this actually installs the venv and downloads Laya's checkpoint on first run — expect it to take a couple of minutes the first time, seconds after).

- [ ] **Step 5: Benchmark cold and warm latency, and RSS, on this machine specifically**

Don't trust Laya's T4 numbers. Write a tiny throwaway script (not committed, scratchpad) that calls `run_laya_predict` 20 times in a loop after the venv is warm, and separately measures the subprocess's peak RSS (`/usr/bin/time -l python3 ...` on macOS gives `maximum resident set size`). Record: cold start time (first call, includes model load), warm per-call latency (median of the next 19), peak RSS.

- [ ] **Step 6: Implement `LayaDecider` as a real `Decider`**

In `src-tauri/src/decision/mod.rs`, add (after the existing `Decider` trait and `DecisionOutcome`/`Tier`/`cascade` from Task 2):

```rust
pub mod laya_bridge;

pub struct LayaDecider {
    pub question_key: &'static str,
    pub instructions: &'static str,
    pub criteria: &'static [(&'static str, &'static str)],
    pub options: &'static [&'static str],
}

impl Decider for LayaDecider {
    fn name(&self) -> &'static str {
        "laya"
    }

    fn options(&self) -> &'static [&'static str] {
        self.options
    }

    fn decide(&self, evidence: &str) -> DecisionOutcome {
        let state = serde_json::json!({"text": evidence});
        let criteria: serde_json::Map<String, serde_json::Value> = self
            .criteria
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
            .collect();
        let questions = serde_json::json!({
            self.question_key: {
                "type": "choice",
                "instructions": self.instructions,
                "criteria": criteria,
            }
        });
        match laya_bridge::run_laya_predict(&state, &questions) {
            Ok(result) => {
                let answer = &result["answers"][self.question_key];
                let choice_label = answer["choice"].as_str().unwrap_or("");
                let confidence = answer["confidence"].as_f64().unwrap_or(0.0) as f32;
                let choice = self.options.iter().position(|o| *o == choice_label).unwrap_or(0);
                let mut probs = vec![0.0; self.options.len()];
                if choice < probs.len() {
                    probs[choice] = confidence;
                }
                DecisionOutcome { choice, probs, confidence, tier: "laya" }
            }
            Err(_) => DecisionOutcome { choice: 0, probs: vec![0.0; self.options.len()], confidence: 0.0, tier: "laya" },
        }
    }
}
```

Pick one of DEC-01's three named pilots (whichever ended up ranked highest by `rank_decisions.py`) to instantiate this against — fill in `question_key`/`instructions`/`criteria`/`options` from that pilot's actual decision shape, not a placeholder.

- [ ] **Step 7: Compare against the fake baseline using DEC-02's own tooling**

Build a small `Vec<Tier>` cascade with a cheap rules-based first tier (reuse the `Fixed` decider pattern from DEC-02's tests, but wired to whatever simple heuristic the pilot decision uses today) and `LayaDecider` as the second tier. Run both over whatever labeled examples DEC-01's pilot writeup names (or a small hand-labeled set of 15-20 if none exist yet), log each `DecisionOutcome` to a JSONL file shaped like `decision_report.py` expects (`{"logits": [...], "label": int}` — Laya's `probs` output maps directly), then run `python3 scripts/model/decision_report.py <output.jsonl> --name laya-pilot --out docs/evidence/W03/laya-vs-fake-decider.md`.

- [ ] **Step 8: Write up findings**

In `docs/evidence/W03/laya-vs-fake-decider.md` (the `decision_report.py` output, plus a short prose section above it), record: cold/warm latency and RSS from Step 5, coverage-vs-precision from Step 7, and an honest verdict — is Laya's calibration good enough on FNDR's actual pilot decision to sit in the cascade for real, or is it a documented spike that needs domain fine-tuning first (Laya's own README says its base checkpoints are weak zero-shot for typed-decision workflows without fine-tuning — expect to report "spike only, not production-ready without fine-tuning" as a legitimate, valuable outcome).

- [ ] **Step 9: Full verification and commit**

Run: `cd src-tauri && cargo test --lib` (the `#[ignore]`d Laya test won't run by default, keeping the suite fast and offline-safe; everything else must still pass).

```bash
git add src-tauri/sidecar/laya_decide_runner.py src-tauri/src/decision/ docs/evidence/W03/laya-vs-fake-decider.md
git commit -m "spike(decision): benchmark a Laya-backed Decider against the DEC-02 baseline"
```

- [ ] **Step 10: Merge to main**

```bash
git checkout main && git pull --ff-only && git merge --no-ff feat/decision-engine-laya
git push origin main
```

---

## End of day: integration, QA, board sync

- [ ] Both branches merged to `main`, both deleted locally and remotely (`git push origin --delete feat/context-coach feat/decision-engine-laya` once merged — these are today's own branches, not shared/ambiguous ones, safe to remove).
- [ ] Full-stack check on the fully-merged `main`: `cd src-tauri && cargo build && cargo test --lib`, then from repo root `npm run typecheck && npm test && npm run build`. Fix anything red before calling the day done.
- [ ] Board sweep: every ticket touched today (#88, extraction-cap issue, #83, #105, #84, FEA-05's issue, #114, #115) should be closed with evidence linked. The two new E-F4 backlog issues from Part 1 Task 4 should exist and be `status::ready`.
- [ ] `make phase-progress` to regenerate the live progress view.
- [ ] Each agent writes an end-of-day report (Codex: reuse the reporting-prompt format from 2026-09-21's handoff — ticket ids, commit shas, verification results, open items, judgment calls, git state).
- [ ] Update `project_beta_final_plan.md` memory with the day's outcome: what closed, what the Laya spike concluded, what E-F4 slice landed and what's still deferred to W6-9.

---

## Self-Review

**Spec coverage:** every open ticket this plan touches (MOD-02, extraction-cap, CAP-05, MEM-03, CAP-07, FEA-05, DEC-01, DEC-02) maps to either an already-complete-needs-closing task (Part 0) or an existing fully-specced plan doc task (pointed at, not duplicated). The two genuinely new pieces (Context Coach MVP, Laya spike) have full bite-sized TDD task detail since no other doc covers them. The remaining 30 backlog tickets are intentionally left alone today — "shoot for the moon" was scoped to two ambitious new-feature pushes plus real board progress, not an attempt at all 38 remaining tickets in one day.

**Placeholder scan:** no TBD/TODO markers. The one spot that looks underspecified on purpose (Step 12's UI markup, Step 6's pilot selection in the Laya task) is deliberately left to the implementer because it depends on that day's own DEC-01 output (which pilot ranks highest) or on reading the existing `AgentPanel.tsx` mode-select markup directly (copying exact JSX here without having read the live file first would risk a wrong placeholder more than deferring the decision to implementation time, which is standard practice for UI insertion points in this codebase's other recent tickets too).

**Type consistency:** `AgentAction`/`AgentActionKind`/`AgentActionStatus`/`ActionResult` names and shapes match `agent/actions.rs` exactly as read from source. `Decider`/`DecisionOutcome`/`Tier`/`Resolution`/`cascade` match the ws6 plan's own DEC-02 Task 2 code exactly (copied from that doc's already-tested Rust, not invented). `propose_action_logic`/`approve_action_logic`/`execute_action_logic` names stay consistent between the failing-test step and the implementation step.
