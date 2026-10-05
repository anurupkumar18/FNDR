# Cloud proposals

Tickets the cloud session proposes during the parallel-session campaign (`docs/superpowers/plans/2026-10-04-parallel-sessions/README.md`, Part 2 rule 6). This folder is inert: `scripts/team/gitlab_sync.py` reads only the top level of `docs/team/tickets/`, so nothing here reaches the board on its own. The local session reviews each proposal, moves accepted ones into `docs/team/tickets/anurup-followups-2026-10.md` (or the owning lane's file), runs `make gitlab-plan`, then `make gitlab-sync APPLY=1`, and deletes them from here. Format and IDs follow `docs/team/tickets/README.md`; each ID is the next free one in its lane.

## VS-40 Make the Rust crate build and test on Linux
- assignee: anurupkumar
- labels: area::vault-search, type::chore, prio::p2
- milestone: W03-Build
- estimate: 3h
- depends: none

**Why.** Cloud sessions and cheaper Linux CI cannot build the crate, so pure retrieval logic can only be verified on a Mac or a `macos-14` runner. The retrieval gate and the 883 lib tests run fine on Linux once five macOS-only call sites are gated. This would also let VS-34 run the gate on `ubuntu-latest`.

**Today.**
- `objc2-screen-capture-kit` is an unconditional dependency in `src-tauri/Cargo.toml` (line 105, above the existing `[target.'cfg(target_os = "macos")'.dependencies]` table), and `objc2` refuses non-Apple targets.
- With that moved, 15 compile errors remain in `src-tauri/src/accessibility/mod.rs`, `src-tauri/src/ocr/vision.rs`, `src-tauri/src/capture/macos.rs`, `src-tauri/src/ipc/commands/notch.rs` (line ~199, `MainThreadMarker`), and `src-tauri/src/ipc/commands/screen_guide.rs` (line ~2641, `app.hide()`).
- Six ungated `#[link(name = ..., kind = "framework")]` attributes (`accessibility/mod.rs` 3, `capture/macos.rs` 2, `ocr/vision.rs` 1) need Apple frameworks at link time.
- Tauri's `externalBin` and the `FNDR Speech Helper.app` resource in `src-tauri/tauri.conf.json` must exist at build time, and `src-tauri/build.rs` builds them with Swift.

**Do.**
1. Move `objc2-screen-capture-kit` under `[target.'cfg(target_os = "macos")'.dependencies]`.
2. `cfg`-gate the five call sites with non-mac stubs: `None`, an "Unknown" context, and `OcrError::InitializationError`.
3. `cfg_attr` the framework links and give the extern functions non-mac stubs, so no linker flag is needed.
4. In `build.rs`, write placeholders for the helper binaries on non-Apple targets instead of building them.
5. Add a Linux CI job (`ubuntu-latest`) that runs `cargo test --locked --lib` and the retrieval gate (`make qa-retrieval-check`).

**Done when.** `cargo test --locked --lib` passes on `ubuntu-latest` in CI with no linker flags, and the macOS build is unchanged.

**Evidence.** The CI run link.

**Note.** These files are owned by the local session (plan Part 2 rule 7), so the cloud did not change them. The cloud measured the fix with a never-committed 55-line shim: lib tests 883 passed, 0 failed, 10 ignored, and `make qa-retrieval-check` ranked identically to the M1 reference.

## VS-41 Check MCP auth for every item of a JSON-RPC batch
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p0
- milestone: W03-Build
- estimate: 1h
- depends: none

**Why.** ADR-017 says `tools/call` needs the bearer token. In Local mode a loopback batch that started with `initialize` skipped the token for every item, so a local process could run any MCP tool without it.

**Today.** `jsonrpc_method_hint` in `src-tauri/src/mcp/mod.rs` returned the first item's method for a batch, and `should_bypass_http_auth` exempts `initialize` and `tools/list` from loopback peers.

**Do.**
1. Failing test first: a loopback batch `[initialize, tools/call]` with no token must get 401; a batch of handshake methods only stays 200.
2. Exempt a batch only when every item is a handshake method.

**Done when.** The test fails on the old code and passes; `cargo test --lib` passes.

**Evidence.** Already done by the cloud on `claude/train-f-new` (b8a2412, PR #26): `docs/evidence/W03/VS-41-cloud.md`.

## VS-42 Feed the persisted insight graph to the entity route, or remove it
- assignee: anurupkumar
- labels: area::vault-search, type::spike, prio::p2
- milestone: W04-Prove
- estimate: 4h
- depends: VS-33

**Why.** VS-33 found that the insight graph is persisted (`graph_nodes`, `graph_edges` in LanceDB, written by the capture flush and idle `commit_graph_updates`), but `retrieve` never loads it. The entity route matches query entities against graph nodes, so it returns nothing on every real query, while the planner still schedules it for project and entity queries. The graph route was taken out of planning for the same reason.

**Do.**
1. Add graph rows to one seeded persona (`seed_demo` writes nodes for its projects and people), so the effect can be measured.
2. Behind a flag, load project and entity nodes once per query (or cache them per store version) and pass them to the entity route.
3. Measure with `make qa-retrieval-check` on all personas, and time the load at 10,000 nodes.
4. If neither recall nor MRR improves, remove the entity route from planning, as VS-33 did for the graph route (anti-bloat gate).

**Done when.** Either the entity route returns graph-backed hits with no Recall@5 drop on any path, or it is out of the planner with the reason recorded.

**Evidence.** Gate output before and after, and the load timing.
