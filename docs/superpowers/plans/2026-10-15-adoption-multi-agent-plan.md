# Adoption plan: day-one value, daily outputs, AI-tool memory, trust, footprint, zero-install

Status: proposal and execution plan, 2026-10-15. Nothing here is committed. The owner has not yet seen the decisions below.

Goal: a knowledge worker installs FNDR and gets a correct answer to "where did I see that?" within five minutes, keeps it because it writes the work they dislike writing, and trusts it because the privacy story is visible and the footprint is small.

Out of scope: teams and enterprise, the iPhone companion, Windows.

## Decisions made (veto any of these)

| Decision | Options rejected | Why |
|---|---|---|
| Imports are local-first. v1 reads files already on the Mac. FNDR stores no third-party token and makes no network call to import. | Slack, Gmail and GitHub API connectors with OAuth in v1 | Fits ADR-018 and the Privacy Proof story. API connectors are a later opt-in path that logs egress. |
| Sources in v1: browser history (Chromium family, Safari), recent files, Calendar (EventKit), Mail (Apple Mail local store and dropped `.mbox` or `.eml`), Slack export zip, GitHub through local git repos plus an optional user-run `gh` CLI. | Gmail API, Slack API, Notion, Drive | Each is either local or a file the person already holds. `gh` makes its own network calls under the person's own login, behind an explicit toggle. |
| Every import goes through one function, `import::admit`, that calls `privacy::safety_gate::evaluate`, the user blocklist and the capture surface policy. Connectors cannot reach the store. | A check inside each connector | One choke point can be tested once and cannot be skipped by a new connector. |
| Imported rows are leaf rows, like agent notes: `source_type = "import:<kind>"`, stored whole, found, shown with provenance, deleted, never merged. | Treat imports as screen memories | Merging an email into a screen session would corrupt both. |
| Imported rows follow `import_retention_days` (default 90), not the 7-day screen retention. | Reuse `retention_days = 7` | A 7-day sweep would delete the backfill the same week. |
| Database encryption is not promised. Encrypt screenshots and exports with a Keychain key, detect FileVault and show it in Privacy Proof. | Column encryption in LanceDB | It breaks full-text and vector search. The workstream must say what is and is not encrypted. |
| Footprint targets are set from a measured baseline, not guessed. | Fixed numbers up front | The reference machine is the 8 GB M1; this Mac has 16 GB and heavy swap, so a baseline here would mislead. |

## Architecture

```
sources (browser, files, calendar, mail, slack, git)
   -> ImportSource::scan(cursor) -> ImportItem
   -> import::admit(item)  [safety_gate + blocklist + surface policy + redaction]
   -> ImportRunner (only writer) -> dedupe by (kind, external_id, content_hash)
   -> MemoryRecord leaf row -> embedding -> LanceDB -> shared retrieval
```

- Module: `src-tauri/src/import/` with `mod.rs` (trait, item, runner), `admit.rs`, `cursor.rs`, and `sources/<kind>.rs`.
- The store writer is private to `import/mod.rs`. A test fails if any file under `import/sources/` mentions `lance_store`.
- Cursors live in `storage/state_store.rs`, so a scan resumes and re-running is idempotent.
- IPC: `import_list_sources`, `import_start`, `import_status`, `import_cancel`, `import_forget`. Forgetting one source removes its rows, chunks, embeddings and graph links.
- Reopen targets: file path, URL, `message://` for Mail, Slack permalink or export path, commit URL or repo path.
- Template to follow: `mcp/remember.rs` (admission) and the `AGENT_NOTE_SOURCE_TYPE` leaf-row handling in `storage/schema.rs`.

## Workstreams

Each workstream is a full slice: backend, IPC wrapper, UI where it has one, tests, and a measurement or evidence file. The cross-cutting ones (pipeline, footprint, trust, zero-install) are horizontal on purpose.

### Wave 1 (parallel, no dependency on the import contract)

| ID | Workstream | Owns | Done when |
|---|---|---|---|
| A | Import core and admission pipeline | `src-tauri/src/import/` (not `sources/`), IPC commands, `docs/evidence/W04/import-core.md` | A fake source flows through `admit` to a searchable leaf row. Banking URLs, password managers and secrets are skipped or redacted. Re-run is idempotent. `import_forget` removes everything. The no-direct-store test exists. |
| F | Daily outputs | new `src-tauri/src/outputs/`, `src/domains/outputs/` | Standup, weekly status and timesheet are generated from memories and activity events, every line cites a memory id, and the person can copy, save a file, open a mail draft, post to a Slack webhook they supply, or create a Linear issue. Each send is an approval card through the risk policy. |
| G | Memory layer for AI tools (MCP) | `src-tauri/src/mcp/` additions, `src/domains/setup/` connect cards | One click connects Claude Code, Claude Desktop and Cursor by writing their config with the existing token. Tools return provenance and honor privacy class. A real client call is run once. |
| H | Trust | `src-tauri/src/privacy/`, delete and export commands, Privacy Proof first-run placement | Default exclusions include banking, password managers and private windows. A recording indicator and a pause hotkey exist. Delete by time range removes rows, chunks, vectors and graph links consistently. Export is JSONL plus Markdown with provenance and can be re-imported. Screenshots and exports are encrypted with a Keychain key. FileVault status is shown. |
| I | Footprint | `src-tauri/src/capture/sampling.rs`, `telemetry/`, `docs/evidence/W04/footprint.md` | Baseline CPU, memory and energy are measured first. Capture triggers on app, window, focus and idle changes instead of a fixed rate. Text is stored by default and screenshots are short-lived. Old data is compacted. Capture quality fixtures do not regress. |
| J | Zero-install runtime | `docs/evidence/W04/zero-install.md`, build and bundle config | Inventory of every sidecar, venv, downloader and external runtime at first run, with size and failure rate. A spike of Apple Foundation Models (LM-08) with measured latency and memory. A signed and notarized build checklist run once. Only code that is safe and tested is changed. |
| K | Action layer | `src-tauri/src/agent/` (see the file list below), approval UI | An approval queue shows the structured action before it runs. Each action has undo or a stated reason it has none. An injection suite proves screen text and imported text cannot trigger a call. |

### Wave 2 (after A merges the contract)

| ID | Workstream | Done when |
|---|---|---|
| B | Browser history and recent files | Chromium family, Arc, Dia, Brave, Edge and Safari history, plus files changed in the last 30 days with text extraction and size caps. Secret-bearing files are never read. |
| C | Calendar and Mail | EventKit past events with attendees and notes. Apple Mail `.emlx`, `.mbox` and `.eml` with quoted text stripped, opt-in per mailbox. A Full Disk Access guide where needed. |
| D | Slack export and GitHub | Slack export zip grouped by channel and day. Git repos found under chosen roots, with the person's commits, branches and changed files. Optional `gh` CLI items behind a toggle that logs egress. |
| E | First-run experience | Onboarding step "Bring your past": a card per source with consent and progress, then a guided "Where did I see that?" search from imported data. Time to first correct answer is measured locally. A settings page manages and forgets each source. |

### Who runs what (updated 2026-10-15)

- Claude Code agents (Sonnet, high effort, one worktree each): A, F, G, H, I, J, K.
- Codex sessions (`gpt-6-sol`, high reasoning, one worktree each under `~/FNDR-wt-codex-*`, branches `codex/import-*`): B, C, D. They code against the draft in `~/FNDR-exchange/CONTRACT.md` with a temporary `contract_stub.rs`, so they do not wait for A. The stub is discarded at integration and the `use` path changed to A's module.
- E (first-run experience) waits for A's IPC and the connector reports.

### Context exchange

`~/FNDR-exchange` (outside the repo, never committed) holds `CONTRACT.md` (A writes it), `board.md` (one row per workstream), `reports/<ID>.md` (each starts with a 15-line `Context for peers` block), and `inbox/<ID>.md` (append-only messages). Agents read peers' summaries, not their diffs. A contract change is a message to `inbox/A.md`, never an edit to the contract.

### Wave 3

| ID | Workstream | Done when |
|---|---|---|
| R | Admission-bypass review | An independent reviewer tries to get a record into the store without `admit`, and tries to smuggle text that triggers an action. |
| Q | End-to-end check | The five-minute path runs on a clean seeded profile. `make qa-retrieval-check`, `make qa-vault`, the Privacy Proof, MCP and the exports all pass with imports present. |

## Rules for every agent

1. Read `AGENTS.md` first and follow it, including the portable skills it points to under `.agent-skills/portable-engineering/`. Test first. Reuse before adding. No drive-by refactors. No commented-out code.
2. Work only in your own worktree. Do not commit, do not push, do not stash, do not switch branches. Leave your changes uncommitted; the owner will commit.
3. Do not edit these files, which have uncommitted work in the shared checkout: `src-tauri/src/agent/mod.rs`, `src-tauri/src/agent/risk_policy.rs`, `src-tauri/src/ipc/commands/agent_chats.rs`, `src-tauri/src/ipc/commands/computer_use.rs`, `src-tauri/src/mcp/mod.rs`, `src-tauri/src/operator/plan.rs`, `src-tauri/src/operator/policy.rs`, `src-tauri/src/storage/lance_store/tests.rs`. If you need a change there, write it in your report as a patch for the owner to apply. Workstream K must work around them the same way.
4. Shared registries (`src-tauri/src/lib.rs`, `src-tauri/src/ipc/commands/mod.rs`, `src/shared/ipc/tauri.ts`, the ipcDrift baseline): append your own lines only, and list them in your report so they can be merged by hand.
5. Rust builds: set the environment below, use `CARGO_TARGET_DIR=~/FNDR/src-tauri/target`, run one cargo command at a time, and expect to wait on the build lock. The Mac's C++ toolchain does not find libc++ headers without it.

   ```bash
   export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX27.0.sdk
   export CXXFLAGS="-isystem $SDKROOT/usr/include/c++/v1"
   export CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=~/FNDR/src-tauri/target
   ```

6. Frontend: link `node_modules` into the worktree (`ln -s ~/FNDR/node_modules node_modules`), then run the typecheck and the tests that cover your files.
7. Never read the owner's real vault, browser history, mail or tokens. Use synthetic fixtures built in the test. A real run against the real program happens once, at integration, with the owner's go-ahead.
8. No em or en dashes in docs, tickets or comments. No AI attribution anywhere.
9. Report format: what changed (paths), what you ran and its result, what you did not run, shared-registry lines to merge, patches for blocked files, open questions. Say how each claim was checked: unit test, fake, or the real thing.

## Integration protocol

1. Wave 1 agents finish in separate worktrees. The owner or the lead agent reads each report.
2. Merge A first. Apply shared-registry lines by hand. Run `cargo check --lib`, the module tests, `npm run typecheck`, `npm test`.
3. Start Wave 2 from the merged tree so connectors build on the real contract.
4. Apply the remaining Wave 1 work one workstream at a time, rerunning checks after each.
5. Run Wave 3 on the integrated tree. The owner commits.

## Risks

- A parallel build storm. The shared target directory serializes cargo, so builds queue; the agents wait instead of failing.
- Merge conflicts in the registries. Mitigated by append-only edits and a hand merge.
- Mail and browser history are the most sensitive inputs FNDR has handled. They need per-source consent, a visible forget button, and the bypass review in Wave 3.
- Footprint changes can reduce capture quality. The fixtures and `make capture-baseline` gate them.
- The origin of each workstream is HEAD, which lacks the 69 commits on `origin/main`. The integration step has to rebase the owner's commit first or merge through `scripts/dev/publish-merged.sh`.
