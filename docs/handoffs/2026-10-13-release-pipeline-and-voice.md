# Release pipeline and voice determinism, 2026-10-13

## Current state

- GitHub and GitLab `main` were 151 commits apart (GitHub ahead, almost all the voice, work-set and Home work of 2026-10-09 to 13). GitLab was fast-forwarded to the GitHub tip `5b53512`, whose test run was green. Both carry the commits below.
- This lane ran in the worktree `.claude/worktrees/release-lane` (branch `claude/release-lane`) and pushed with `git push gh HEAD:main` and the same to GitLab. A Codex session ran beside it on Home (`codex/fix-home-section-storage`, merged as PR 40).
- The shared checkout `~/FNDR` is still on `d99204c`, more than 180 commits behind, because the paused documentation pass of 2026-10-08 left uncommitted edits in `src-tauri/src/lib.rs` and `src-tauri/src/mcp/mod.rs`, which incoming commits touch. Its owner is the session in `docs/handoffs/2026-10-08-fndr-documentation-doctor.md` (untracked there). Nothing in it was touched. An app built from that checkout is five days old.
- The GitLab API refused connections with an expired TLS certificate, so no board ticket was read or moved. Git over SSH worked.

## Shipped

| Commit | What | Checked by | Not checked |
| --- | --- | --- | --- |
| `ffff8af` | Release job builds on `macos-26` like the Rust tests; a tag that does not name the app version is refused; `workflow_dispatch` builds without publishing; `src/dev/releaseDrift.test.ts` | Contract test under `npm test`; one run by hand compiled, bundled and ad-hoc signed the app | Everything after the updater signature: `codesign --verify`, the artifact upload, a tag push |
| `bce4e81` | The speech registry names a voice when it starts speaking; each NotchHud test has its own registry | Registry unit test; Notch and voice tests under five shuffle seeds; full frontend suite under two; test CI green on `main` | Not heard on speakers |

## Blocker for the owner

The run by hand stops at `incorrect updater private key password`. `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` must be reset from the backed-up key; the commands are in `docs/setup/releasing.md`. Until then the updater manifest and the site's plain download link return 404. The `v0.3.5` release holds a hand-uploaded DMG that says version 0.3.0.

## Next, in order

1. Owner resets the two updater secrets. Then `gh workflow run release.yml --ref main` and read the signature check and the `FNDR-dmg-unpublished` artifact.
2. Set version `0.3.6` in the five files `releaseDrift.test.ts` names, tag `v0.3.6`, push the tag to GitHub. Confirm `latest.json` and `FNDR-latest.dmg` answer 200 and that an installed copy offers the update. This publishes a public release, so it needs the owner's go-ahead.
3. The release build took 45 minutes cold. `Swatinem/rust-cache` saves only after a job succeeds, so the next green run should be faster; measure before changing anything.
4. Resume or drop the paused documentation pass so the shared checkout can fast-forward.
5. Open items in `docs/handoffs/2026-10-08-test-determinism.md`, updated today.

## What caps product quality, measured 2026-10-13

Read from the owner's vault, counts only. These decide what is worth building next more than any open ticket does.

- **The vault is starved.** 162 memories. FNDR captured on 9 of the 27 days from 2026-09-17, none after 2026-10-09. Every figure measured on this vault, here and in earlier evidence, rests on that.
- **Work has no identity outside a code repository.** 21 of 162 memories carry a project, because a project is detected only from a repository path. Resume threads fall back to the window title: 71 threads, 54 with one or two memories, one with 64. Work sets, Home's recent threads and meeting prep are all built on those threads.
- **The insight graph adds nothing to search.** Every one of its 659 nodes has one source memory. VS-69 was built without the graph, measured, and dropped: no change in Recall@5 or MRR@10 on three personas (`docs/team/tickets/proposed/cloud-proposals.md`).
- **The synthetic gate is at its ceiling.** Recall@5 is 1.000, 0.950 and 1.000, so it can catch a regression and cannot show an improvement.

Next product decision, for the owner: define a piece of work for a knowledge worker (a document and what was open with it, a course, a recurring meeting) and assign it at capture with evidence, then rebuild Resume threads on it. Until the vault has weeks of real days, measure it on the persona corpora extended with cross-app work.

## Process notes

- Worked: a contract test that ties the release image to the tested image; proving the workflow by hand before any tag.
- Did not work: the release workflow has no successful run in its history, so each tag was its first real test. A failed release was patched by uploading files by hand, which hid the failure from the site but not from the updater.
- Correction: run the workflow by hand after any change to a native helper, a bundled resource or the workflow, before tagging.
