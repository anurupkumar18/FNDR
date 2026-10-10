# Handoff: test determinism lane, state on 2026-10-08

## Goal of the lane

Every test gives the same result on any machine and in any order: inside or outside a git work tree, with any home folder, time zone, language, thread count or test order. A test run changes nothing outside its own temp folder.

## What shipped

| Commit | Change | How it was checked |
|---|---|---|
| `dd25d9c` | The action lifecycle test runs `pwd`, not `git status`. The two `mcp::tests` that stop and start the MCP server share the `serial_test` key `mcp_server`. | Paired run of the two server tests: 9 of 20 passed before, 30 of 30 after. `make test-clean` three times, all green. |
| `18b910d` | Test builds keep the MCP discovery file and bearer token in a folder under the system temp dir (`fndr_home()` in `src-tauri/src/mcp/mod.rs`), not in `~/.fndr`. | Full library suite with `HOME` set to an empty folder: green, nothing written there. A test asserts both paths are outside `~/.fndr`. |
| `790e0f1` | Product fix: an approved read-only command that exits non-zero is stored as `Failed`, not `Succeeded`. | New test, red first, then green. Unit test only. Nothing in the frontend calls `executeAgentAction` today. |
| `9bd1e1c` | Five library tests use `tempfile::tempdir()`; the MCP test home is one fixed folder. | Count of `fndr-*` entries in the temp dir before and after a full run: equal. |
| `e963e4d` | `vitest.config.ts` pins the locale that test workers start with. | `npm test` with `LANG` and `LC_ALL` set to German, French and Arabic: 1, 1 and 6 failures before, none after. |

## How to tell whether it still holds

The library suite passed in each of these on the commits above. Build once, copy the test binary out of the shared target folder, and run the copy (see the process note below).

| Variation | Command, with `$EXE` the copied binary |
|---|---|
| Outside a git work tree | `make test-clean` |
| Empty home | `HOME=<empty folder> $EXE` |
| Time zones | `TZ=Pacific/Kiritimati $EXE`, `TZ=Pacific/Honolulu $EXE` |
| One thread, sixteen threads | `$EXE --test-threads=1`, `$EXE --test-threads=16` |
| Another working directory | `cd / && $EXE` |
| Another language | `LANG=de_DE.UTF-8 LC_ALL=de_DE.UTF-8 $EXE` |
| Short `PATH` | `PATH=/usr/bin:/bin $EXE` |

The Python script tests (`make scripts-test`) passed with an empty home, two time zones and in a full `git archive` copy. `npm test` passed in three time zones, three languages, a non-git copy, serially (`--no-file-parallelism`) and with `--sequence.shuffle` seeds 1 and 3.

## Open, in order

1. **Two frontend tests fail now and then with shuffled order.** `npx vitest run --sequence.shuffle --sequence.seed=2` failed 2 of 4 runs, a different test each time: `ScreenGuidePanel > does not let an older press rejection cancel a newer hold` ("Release to ask" button not found) and a peer directory test, `previews a bounded task for a saved peer without sending until asked`. The same seed passed the other two runs, and the Screen Guide file alone passed 6 of 6, so this is timing, not order. Not diagnosed. Start by running each file in a loop under CPU load.
2. **`MemoryCardsPanel > requests the full all-app browse limit and renders returned cards` is slow.** It renders 1,500 cards, took 1.3 to 2.7 s alone and hit the 5 s timeout once while a second vitest run was going. Either render fewer cards for the assertion or give the test its own timeout, after measuring.
3. **Integration tests under `src-tauri/tests` were not run through the variations.** Building 20 more test binaries from a second path needs several GB and the disk had 11 GB free. Run them from the main checkout, where the binaries already exist.
4. **Rust test order.** `--shuffle` needs a nightly compiler. One thread and sixteen threads both pass, which covers parallel collisions but not order.
5. **A new test that starts the MCP server without `#[serial_test::serial(mcp_server)]` brings the race back.** Nothing enforces the key. A cheap guard would be a test-only lock taken inside `start` and `stop`; not built, because two tests do not justify it yet.
6. **`config::tests::data_dir_override_reads_env_and_ignores_blank` sets `FNDR_DATA_DIR` for the whole process.** Nothing else in the library tests reads it today. Mark it serial if a second reader appears.
7. **About 860 old `fndr-*` folders sit in the system temp dir** (94 MB, most from September). Some may belong to a running session, so they were left alone. The owner can remove them when no test is running.

## Process notes

- **The shared cargo target folder is shared across checkouts of this repo too.** `~/.cargo/config.toml` points every build at one folder, and a worktree and the main checkout produce the same test binary name. A loop that reruns `target/.../fndr_lib-<hash>` can silently start running another session's build partway through: the test count changed from 1299 to 1312 mid-loop and three "failures" were the other tree's. Correction: after `cargo test --lib --no-run`, copy the binary to a scratch folder and run the copy.
- **`make test-clean` with a new `FNDR_CLEAN_DIR` adds a full set of crate artifacts to the shared target folder.** Reuse one clean folder path.
- This lane ran in an app-made worktree (`claude/jolly-poincare-79a1d8`) and was pushed to `main` from there. The main checkout had other sessions' uncommitted edits to `src-tauri/src/mcp/mod.rs`; they were not touched.
