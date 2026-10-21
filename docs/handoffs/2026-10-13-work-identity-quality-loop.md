# Work identity quality loop checkpoint, 2026-10-13

## Current state

- GitHub `gh/main` and GitLab `origin/main` both resolved to `6771f90` after [PR 44](https://github.com/anurupkumar18/FNDR/pull/44), [PR 45](https://github.com/anurupkumar18/FNDR/pull/45), and the ticket repair `7f4ca48`. The work used isolated worktrees. The shared checkout still contains another session's uncommitted work and was not edited by this lane.
- All four GitHub workflows on merged code commit `6771f90` passed: test, retrieval-gate, rust-clippy analyze, and CodeQL Advanced. The full retrieval workflow covers knowledge-worker, office-pm, and software-engineer synthetic profiles.
- The broad product-quality goal remains active. Native capture and usefulness have not been verified by these automated checks.

## Delivered and checked

| Change | Evidence | Limit |
| --- | --- | --- |
| PR 44 clears a model-proposed `project` unless its substantive words occur in captured title or text, even at high model confidence | Capture tests: 74 passed. Memory-quality tests: 46 passed, 1 ignored. `make qa-vault`: 12 checks passed on a disposable vault copy. PR CI passed. | Existing records were not rewritten. A seeded retrieval gate does not exercise model extraction. |
| PR 45 requires project words in one title or text sentence or line, and keeps decimal version names such as `FNDR 2.0` intact | The ten-case synthetic corpus exposed a false grouping before the fix; six supported cases and four unrelated cases pass after it. Capture tests: 75 passed. The combined `project` tests passed 32/32 after the first parallel Resume merge. PR CI and merged-main CI passed. | The corpus tests the deterministic validator, not model recall or native capture. The last vault gate ran before the decimal-version correction. |
| `7f4ca48` removes an invalid VS-95 dependency on proposed ticket VS-69 | `make scripts-test` passed, including 35 GitLab ticket tests, and merged-main frontend CI passed. | This repairs ticket metadata only; no GitLab board mutation was made. |

Claude's parallel Resume changes and [ADR 031 addendum](../decisions/031-evidence-backed-work-identity.md) now join windows by names found in titles, with a guarded text pass, at read time. Its persona scores are recorded in the ADR and were shaped on 86 synthetic entries. The native app has not been checked for that behavior. The read path filters memories with `resume::admits` before naming threads.

## Next steps and ownership

1. **Native Quality Lab, VS-71 and VS-96:** Run the three gold cases in the isolated knowledge-worker Lab profile, then inspect Home and a saved work set for a named cross-app thread. Capture content-safe outcomes and wrong groupings by their shared words. The earlier Lab window was unavailable to desktop control, so this needs operator-visible native evidence. Do not treat the fixture or browser runs as a substitute.
2. **Work identity coverage:** Measure which explicit work names the real extractor emits on synthetic cross-app captures. If assignment is widened, use the existing `project_evidence` and `project_confidence` fields, keep ambiguous screens unlabeled, and recheck Resume and work-set results. Do not rewrite the owner's vault without FNDR closed, a backup, and the owner's go-ahead.
3. **Other existing blockers:** The updater signing password mismatch and GitLab board certificate failure remain as described in [the release handoff](2026-10-13-release-pipeline-and-voice.md) and [the earlier quality handoff](2026-10-13-autonomous-quality-loop.md). They were not rechecked in this slice.

## Process correction

The first broad capture test found a fixture that lacked its expected project name; correcting the fixture exposed the intended new rule. A later ten-case corpus caught a real cross-context false grouping and a decimal-version false rejection before publication. Parallel Rust builds competed for the shared cache twice; stop the local build when another session is compiling, and let remote CI verify the combined tree. A parallel ticket edit broke the frontend job through an unknown dependency; repair that as its own small commit, run `make scripts-test`, then verify the merged main workflows rather than relying on PR checks from an older base.
