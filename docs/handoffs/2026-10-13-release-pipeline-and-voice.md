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

**Decided and built the same day (ADR 031 and its addendum).** The Codex session kept `project` as the stored work identity and made capture keep only labels the screen supports. This session joined windows that share a name into one Resume thread at read time (`resume/work_names.rs`, VS-95). Next are VS-96 (see it in the running app and on a real week) and VS-97 (attach by on-screen text, with a guard), both in `docs/team/tickets/anurup-followups-2026-10.md`.

## Later the same night

| Commit | What | Checked by |
| --- | --- | --- |
| `db2f8ba` | A named thread's title is the longest phrase its names spell; names keep a title's capitals | Unit tests; seeded-store evaluation; native debug build on a seeded profile (screenshot in `docs/evidence/W04/named-threads/`) |
| `f542569` | Companion: no pairing code over the network, five wrong codes void it, no LAN server until a phone is paired (ADR 009 addendum) | Route and launch-gate tests; the exploit run before and after on a debug build with a synthetic profile |
| `42609b0` | Search: a literal the query spells out ("1.8%", an identifier) settles near ties | Before and after on three personas, `docs/evidence/W04/literal-match.md` |
| `52ca052` | The retrieval gate no longer depends on the hour it is seeded | Seeder test at 23:30; gate at 22:42 |

Left for others: the 2B model loads at every launch with nothing to do; unloading needs an owned model design and belongs to LM-09 (Kunj). Most remaining search misses are paraphrases; the embedding migration (VS-49 with Minh's EM-09) is the lever, not ranking code. `~/.fndr/companion.json` was rewritten by the test app and names a dead port; nothing reads it but the parked iPhone tooling.

## 2026-10-14: agents and tools

Owner decisions in the decision log (2026-10-14): Notch Do is the one action layer; simple requests are planned on the Mac; Notch Do learns from runs.

| Commit | What |
| --- | --- |
| `76878d1`, `0c87c2e` | Notch Do suite: `scripts/operator/notch-do-cases.json` (now 43 cases) and `notch-do-policy-cases.json` (21), scored by `operator::eval` through `plan::route` |
| `e9736cd` | First live planner recording, 20 of 23; rescored offline on every test run |
| `a897107` | `plan::local_plan`: opening apps and links and web searches run with no Codex session; `plan::settled` drops the redundant open step (recording now 23 of 23, floor 1.0) |
| `2999b88`, `5af9304` | The command-bar stack nothing called is removed; the command bar hands action requests to Notch Do (`omnibar_do`). `2999b88` alone did not compile; `5af9304` two minutes later completes it |
| `790caee` | `operator::runs` and `operator::skills`: ratings, draft cases from wrong runs, skills replayed by name |

Not verified in the running app: the command bar's "Do it" row, a local run, rating a run and running a skill by voice. Each changes something on the Mac (opens an app), so it needs the owner at the screen.

Open, flagged for a decision: "Remove from queue" in Spotify is refused because "remove" is a never-do word; skills have no list, rename or delete screen; the October tool list (paste, reminders, Shortcuts, timers, reveal file) is the backlog of Notch Do step types; `decision/` (Laya) has no callers and the paused documentation pass deletes it.

## 2026-10-15: skills, paste and the command bar

| Commit | What |
| --- | --- |
| `f4fe51c` | Alt+Space is registered again (off since the Alpha demo, so the command bar's "Do it" row could not be reached); a finished run says "Done" once |
| `b56e4e9` | The 10k-row storage benchmark runs on demand: every `cargo test` was rewriting `docs/evidence/W04/storage-indexes.md` from an older template |
| `2d3cbbd` | Settings > AI > Notch Do (skills: run, delete; the newest 20 runs: rate, save as a skill); "Run skill: …" in the command bar; `notch_do_request` replaces `omnibar_do`; `RunRecord::repeatable` fixes skills saved from work sets; the `paste` step |
| `ee44d77` | Suite cases carry synthetic memory snippets for the live planner run (23 of 24, `recordings/2026-10-15-live.json`); a paste with no text pastes what was copied; links and searches in a named browser run on the Mac; "reply" is a never-do word only as a verb; the voice line names the voice |
| `b541ac9` | CI builds Rust against OpenSSL 3: the macOS image carries OpenSSL 4 since 2026-10-13 and llama.cpp's bundled HTTP client does not compile against it, so every cold Rust job failed |

Seen in a debug build on a disposable profile (2026-10-14): "open Notes" typed in the command bar, handed to the notch, planned on the Mac, Notes in front, checked, "Did that work?" shown. Seen in the browser preview: the Settings section (rate, save, run refused while Operate is off, delete). Not seen in the app: rating, saving and running a skill, the command bar's skill row, a paste.

Open:
- In the one native try, the notch came back in Ask with the finished card gone before "Worked" was tapped, so the rating was lost. Not reproduced and not explained; the likeliest cause is a reload of the notch page, which reads the remembered mode. Settings now offers the rating as well, so the loop no longer depends on that card.
- The seed for a native check needs `--pause-capture`; without it the disposable profile captures the screen. `seed_demo` writes the pause only with that flag.
- A paste puts copied plain text back afterwards; an image on the clipboard is replaced.
- Reopen targets: Dia memories should now keep their page URL (`crate::browsers`), not yet seen on a capture. Finder memories still carry no folder path, so a work set opens Finder rather than the folder.
- `computer_use.rs` holds a test that keeps a `MutexGuard` across an await (clippy `await_holding_lock`); it predates this work.

## Process notes

- Worked: a contract test that ties the release image to the tested image; proving the workflow by hand before any tag.
- Did not work: the release workflow has no successful run in its history, so each tag was its first real test. A failed release was patched by uploading files by hand, which hid the failure from the site but not from the updater.
- Correction: run the workflow by hand after any change to a native helper, a bundled resource or the workflow, before tagging.
