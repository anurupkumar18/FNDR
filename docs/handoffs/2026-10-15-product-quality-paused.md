# Product PR resolution and shutdown checkpoint, 2026-10-15

The earlier pause checkpoint is superseded by the owner's request to resolve the four PRs shown in the screenshot. Those four PRs are resolved. The broader product goal is not declared complete.

## Shipped and resolved

- [Home PR 52](https://github.com/anurupkumar18/FNDR/pull/52) and [Settings fixtures PR 54](https://github.com/anurupkumar18/FNDR/pull/54) were already merged. Verified their live states.
- [Search/Vault PR 53](https://github.com/anurupkumar18/FNDR/pull/53) merged as `888ca27` after all checks passed on `6840dca`: frontend/script tests, macOS Rust, all three synthetic retrieval corpora, clippy and CodeQL. Both GitLab and GitHub main independently reported `888ca2766dee8f52d670ac4070bea132df924093` after synchronization.
- [OpenSSL PR 55](https://github.com/anurupkumar18/FNDR/pull/55) closed as superseded. Main already contained `b541ac9` and `3eecbb7`, including the OpenSSL 3 header path and fresh cache key. Main's real macOS Rust job 38785212007 passed. The conflicting duplicate bootstrap was not merged.

## Additional work, not shipped

[PR 56](https://github.com/anurupkumar18/FNDR/pull/56) is draft. The previous uncompiled model-lifetime edit is now a compiled owned-runtime implementation with real model and native evidence. Do not label F9 fully resolved: remaining native scenarios are explicit in [the evidence](../evidence/W01/LRN-01.md#f9-implementation-checkpoint-october-15-2026) and [ADR 032](../decisions/032-owned-model-lifetimes.md).

- Worktree: `/Users/anurupkumar/.codex/worktrees/fndr-product-home-ux/FNDR`, branch `codex/model-lifecycle-shutdown`.
- Implementation commits: `98fc747`, `2ee6a67`, `1a12623`, `2afc524`, `c517ff5`. The last two fix the observed AppKit menu bypass and add readiness/drain logs. They are included with this evidence update for publication.
- CI on the previously published `f9f0e1d` passed frontend/script tests, macOS Rust, all retrieval corpora, clippy and CodeQL. That CI does not cover the later native menu change. The later native bundle compiled and ran against the real app.
- Native final checks: two idle menu quits and a quit after actual Search card generation exited 0; an image import used the existing high-pressure OCR/text fallback and also quit normally. Real vision process teardown passed separately. Native VLM teardown remains unverified.
- Final full library run on `c517ff5`: 1,552 passed, zero failed, 30 ignored, 76.06 seconds with two test threads. A direct cached-executable cancellation rerun was invalidated by another lane replacing that executable; worker markers were absent. A Cargo-managed rerun is queued to verify the later stop-before-drain assertion.

## Ownership and local QA cleanup

The shared `/Users/anurupkumar/FNDR` checkout remains on main with other sessions' uncommitted edits and an old local HEAD. It was inspected but not changed, staged, stashed, reset or switched. Do not use the screenshot's Changes badge as permission to overwrite those edits.

The synthetic Office PM profile's original model symlink has been restored after closing the QA process. Its temporary replacement contained only per-file model references and the isolated projector. The separate `com.fndr.app.qa-lifecycle` profile was a synthetic copy. Remove owned scratch assets and superseded QA bundles after required model checks, and leave no running QA process. No owner vault repair or cleanup is authorized by this checkpoint.

## Resume without repeating discovery

1. Verify PR 56 head, CI, dirty paths and both main tips. Keep the already resolved PRs closed/merged.
2. Finish the remaining native cases using the distinct QA bundle and paused synthetic profile. High host pressure made the real import skip VLM; do not disable that protection or call fallback a VLM pass. The normal menu now has a custom `fireMenuItemAction:` Quit; the old `terminate:` item reproduced SIGABRT.
3. Only after the required native gates pass, mark PR 56 ready, merge normally and independently verify both mirrors. Update the F9 status without replacing failures with narrower successes.
4. Return to the design direction only after the current slice is closed. Unreachable legacy inspector/import components were noticed during navigation; they were not changed in passing.

## Process correction

The native regression exposed an AppKit termination path that unit and model-process tests could not cover. Preserve that distinction. Before any `cua.getApp` call, verify the generated plist identifier and install the guarded synthetic-profile launcher; a lookup can launch an app. Use a fresh bundle path after changing identity so LaunchServices and the computer-use helper cannot retain the old registration. Native full-screen activation made the existing sidebar controls usable; restore the QA window afterwards. Keep Rust work serialized and prefer closing one verified slice before opening another.
