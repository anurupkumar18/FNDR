# OB-07 local onboarding funnel, three isolated test runs

Generated 2026-10-19 from three disposable profiles driven through the production Rust event writer by `ipc::onboarding_funnel::tests::three_isolated_runs_cover_complete_skip_and_error`. This is deterministic harness evidence, not native UI observation or an outside-user study. The event files contained only event kinds, onboarding step names, wall-clock and monotonic times, and durations. The profiles covered completion and a search/select/reopen proxy, a model skip that returned to model setup without completion, and an error with no completion.

Reproduce from the repository root. The test writes only its own scratch profiles when `OB07_EVIDENCE_DIR` is set:

```sh
OB07_EVIDENCE_DIR=/private/tmp/fndr-ob07-test-runs \
  CARGO_BUILD_JOBS=2 cargo test --manifest-path src-tauri/Cargo.toml \
  --lib ipc::onboarding_funnel::tests::three_isolated_runs_cover_complete_skip_and_error
python3 scripts/audit/onboarding_funnel.py \
  --events /private/tmp/fndr-ob07-test-runs/run-1.jsonl \
  --events /private/tmp/fndr-ob07-test-runs/run-2.jsonl \
  --events /private/tmp/fndr-ob07-test-runs/run-3.jsonl
```

Output:

```text
Trials: 3
Completed onboarding: 1
Incomplete onboarding: 2
Errors: 1
Activation proxy: 1
Observed activation: not measured

Skips: bring_past 1; model_download 1

Step                 Started  Finished  Skipped  Errors  Mean duration
welcome                    3         2        0       1        1000 ms
privacy_promise            1         1        0       0        1000 ms
biometrics                 1         1        0       0        1000 ms
model_download             3         1        1       0        1000 ms
permissions                2         2        0       0        1000 ms
bring_past                 2         1        1       0        1000 ms
```

The second run has no `completed` event after skipping the model. This preserves the OB-01 distinction between a Home appearance and a persisted onboarding completion. The single activation proxy means that the local sequence search submitted, result selected, and source reopen succeeded was recorded within ten minutes. It does not establish that the result was relevant or useful; PD-13 must observe that in outside-user sessions. A native UI run with OB-07 enabled remains to be recorded before treating these counts as field data.

On 2026-10-20, Felipe ran a separate native profile through the skip path. The local report showed one incomplete trial, no `completed` event, and six skipped steps. The persisted state was `model_download`, yet the active window showed Home after Skip imports. The frontend had used the requested `complete` state rather than the state normalized and saved by Rust. The follow-up commit makes the save command return the persisted state and keeps the UI on model setup when completion is rejected. A native rerun of that fix is pending; this observed run is not counted in the three harness trials above.
