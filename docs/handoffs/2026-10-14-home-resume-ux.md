# Home and Resume UX checkpoint, 2026-10-14

## Current state

- `codex/home-resume-ux` is the isolated branch for the Home slice and [draft PR 52](https://github.com/anurupkumar18/FNDR/pull/52). The shared `/Users/anurupkumar/FNDR` checkout has other sessions' uncommitted work and was not edited, stashed, reset or switched by this lane.
- The broad product-quality goal remains active. Home's recent-work and direct Continue path are implemented. The follow-up closes an open-time visibility gap and puts the result beside Continue in the native window.
- GitLab's HTTPS API certificate expired, so no GitLab merge request was created. Branch pushes through `origin` reached both GitLab and GitHub; verify both tips again after the follow-up push.

## Delivered and checked

The Home layout, action hierarchy and browser checks are detailed in [home-rebuild.md](../evidence/W04/home-rebuild.md). Continue now goes through the existing work-set opener for one memory, so the current blocklist, quality and deletion visibility, Private Mode, and the actions switch are checked before opening. A refused or vanished source returns a typed `unavailable` result. The line sits beside Continue and wraps on narrow screens.

| Check | Result and limit |
| --- | --- |
| Full frontend suite | 108 files and 844 tests passed before integration; 108 files and 847 tests passed on the combined tree. The native command was faked in frontend tests. |
| Typecheck and native bundle build | Passed. The debug bundle was built with a distinct `com.fndr.app.qa-home-resume` identifier. |
| Focused Rust tests | `cargo test --lib reopen` passed 71 tests before integration. `cargo test --lib workset::` passed 36 tests, one ignored, both before and after integration. The work-set test uses a real store and a fake Mac opener, proving a quarantined memory never reaches the opener. |
| Browser preview | Desktop light and dark plus 390 px dark were checked. Inline feedback remained beside Continue and 390 px had no horizontal overflow. |
| Native QA, synthetic office-pm profile | The bundle identifier, sole process path and `FNDR_DATA_DIR` startup log were verified before computer use. Packaged `tauri://localhost` showed recent work and Continue on the first window. A Zoom source returned `app_missing` beside Continue; a Chrome source returned `opened` and Home said `Opened the latest source`. The destination page content was not checked. |
| Native shutdown | Each isolated debug run exited with a `ggml-metal-device.m` assertion and code 134 on quit. The process stopped. This is open and should be diagnosed separately. |
| Office PM retrieval gate | `make qa-retrieval-check PERSONA=office-pm` passed after reseeding the synthetic profile. Recall@5 was 0.950 for search, ask and retrieve. One of eight no-match queries still read strong; four of 33 real queries read weak. Those are reported counts, not gate failures. |

The first native attempt attached an older installed bundle using the default owner profile and briefly captured screen context. It was quit without editing or cleaning the owner vault. The isolated runs above used only the paused synthetic profile.

## Next steps

1. Publish the combined branch to both remotes and verify both branch tips. Update PR 52 with the native evidence and CI result. The branch contains the previously published PR history and current main through a merge commit, so the push needs no history rewrite.
2. Diagnose the ggml Metal assertion on native shutdown in a separate narrow defect slice. Test an access-refusal result through native UI if a safe synthetic setup is available. Verify a real external destination and work-set arrangement separately.
3. Keep improving other high-value product flows after this Home slice. The current design contract is [DESIGN_DIRECTION.md](../product/DESIGN_DIRECTION.md).

## Process correction

Select a distinct QA bundle and verify its executable path and profile log before computer use. An app lookup by the production bundle ID can attach an older installed app. Rust builds share a cache on this machine and must run one at a time.
