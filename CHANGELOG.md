# Changelog

Development log and pending-decision tracker for FNDR. See the [Beta-to-Final master plan](docs/superpowers/plans/2026-09-21-beta-final-master-plan.md) for the full roadmap.

---

## Recent Development

Latest work on the Beta-to-Final push:

- **Daily Summary and briefing count only your own tasks (2026-10-07):** the summary's open-task list and count are the tasks a person added, accepted or took from a meeting, never unaccepted suggestions, and nothing is said when none are open. System processes such as UserNotificationCenter are no longer listed as apps used, and a bullet no longer repeats its topic as its example. The briefing looks back on today from noon (it used to be a morning briefing until 5 PM), reads only that window's activity plus up to three open tasks, and is skipped when fewer than three captures qualify (`briefing.rs`). The briefing prompt itself is unchanged and its output was not re-measured.
- **Tasks must be quoted from the screen (2026-10-07):** an audit of the owner's list found 471 open tasks from 147 memories, none ever completed, over half read off AI chat screens, and up to 33 from one memory because every merge asked again. Capture now asks once per new memory, never on AI chat or system screens, at most once per app per 10 minutes, and keeps a task only when the model quotes words that are on the screen and state a commitment or a request (`tasks/suggest.rs`). The quote is stored as the task's reason. A suggestion that means what a recent task already says (cosine 0.80 or more on the title) is not offered again. Opening To-dos no longer creates tasks from memories. To-dos now lists the person's own tasks first and offers at most five suggestions, each with its quote, for three days; a suggestion becomes a task only when accepted. Numbers in `docs/evidence/W04/task-audit.md`. Run on the real local model: across 28 synthetic screens it wrote 42 task lines, 12 screens state a task, and the check kept 11. At first sight of two unseen sets the check got 6 of 9 and then 7 of 8 screens right; each miss became a rule (a request counts only in mail, chat and notes; a deadline needs a date). **Not measured on real captures. Older suggestions are hidden, and are dismissed for good only by `retire_task_suggestions`, run so far on a copy.**
- **Notch Do: voice-driven computer use (2026-10-06, `kunj-notch-computer-use`):** opening the notch in Do mode listens on FNDR's native on-device voice owner (the WebKit listener and spoken replies are gone). When speech ends, a Codex app-server turn on the person's ChatGPT plan plans the request; the plan card starts it after 1.5 s unless "stop" or Cancel. FNDR opens apps and links natively (NSWorkspace), hands only in-app UI to Computer Use (OpenAI's bundled plugin, else open-computer-use), checks every step itself (frontmost app, Spotify/Music playback, browser URL), retries once, and journals every action to `operator/journal.jsonl` with typed text hashed. Every computer-use call is gated in code (`operator/policy.rs`): reading, scrolling, media and search typing run; other typing and submits ask once; sending, deleting, buying, credentials, password managers and System Settings are refused. "Stop" kills the run and Codex's process group. ADR-020, ADR-022 and ADR-018 carry the 2026-10-06 amendments. Verified live from a shell harness (`live_notch_do_runs_the_example_request`, open-computer-use): "open Spotify, play Blinding Lights, then open the browser and look up looped transformers" finished in 135 s. **Not yet verified: voice inside the app**, and the bundled Computer Use, which needs FNDR to have Automation access to it.
- **Hermes on the same ChatGPT sign-in (2026-10-06):** no API key and no second login. Hermes imports the Codex login itself; the Codex app-server is the only refresher (before every Hermes start and every 30 minutes), and Hermes's copy is emptied whenever the login rotates. Live check: a Hermes call answered on the plan and `codex login status` still reported "Logged in using ChatGPT". FNDR installs a pinned Hermes (`b8880f1`, v0.18.2) into its app data on first run; the orphan `hermes-agent` gitlink is removed. The gateway restarts once after a crash and shows its state. Each Hermes message carries up to 5 memory snippets from the shared retrieve path instead of the old `FNDR_CONTEXT.md` snapshot, and Hermes can query FNDR over MCP.
- **Privacy Activity lists cloud model requests (2026-10-06):** feature, host and bytes FNDR sent for every Notch Do and Hermes request, never content.
- **One voice and honest insight fields (2026-10-06):** nothing FNDR writes about a memory has a narrator any more ("Reviewed the PR", not "You reviewed the PR"), including older stored text at display time. Vault insight rows no longer show activity identifiers, session ids, or next steps under "What changed".
- **One prompts file (2026-10-06):** every local-model prompt lives in `src-tauri/src/inference/prompts.rs` with guard tests; a prompt cannot change without a version bump. Unused model code (`vlm.rs`, `model_worker.rs`, `qwen_vl_memory.rs`) is gone. The v3 prompts are **not yet measured on the real model**.
- **Search query prompt (2026-10-06):** the live MiniLM route no longer prepends a BGE instruction to queries. Gate passes on all three seeded personas; numbers in `docs/evidence/W04/2026-10-06-query-prompt.md`. BGE prefixes are unchanged.
- **Vault filters and MCP start instructions (2026-10-06):** perspective filters no longer hide cards whose activity label is not the filter's own, and connecting agents are told where to start and that results are evidence, not instructions.
- **Agent approval gating** — `Act` mode now requires explicit approval before `OpenUrl` or `OpenFile`, closing a policy gap where those actions previously executed without a prompt.
- **Screen Guide observability** — every voice/ask stage transition is now logged, and the flow recovers cleanly from a stuck transcription or ask call.
- **MCP tool surface audit (RET-02)** — all 51 MCP tools classified by risk and overlap in `docs/product/mcp-tool-audit.md`, with keep/merge/remove verdicts (see below).
- **Capture enrichment scheduling policy (MEM-06)** — policy code merged but **not yet wired into the live pipeline**.
- **Eval gold set v0 (MOD-04)** — draft evaluation set added for review, not yet finalized.
- Trust and honesty pass across Search, Ask, Timeline, Memory Vault, Daily Summary, and Stats: clearer pending/error/stale states and consumer-honest labeling.

---

## Flagged for Removal or Change — Needs Approval

These are called out in-repo (commit messages, `docs/product/mcp-tool-audit.md`, and the master plan) as pending a decision before they're acted on. Nothing below has been removed or changed yet; listing them here so they don't get merged silently.

| Item | What | Where |
| --- | --- | --- |
| `memory.search_raw` MCP tool | Flagged **remove**: duplicate of `memory.search_full_context`, and releases raw captured text without a gate | `docs/product/mcp-tool-audit.md` |
| `search_memories` MCP tool | Flagged **remove**: pure duplicate of `fndr.search` | `docs/product/mcp-tool-audit.md` |
| `get_ambient_context` / `fndr_context` | Flagged to **fold into one tool** | `docs/product/mcp-tool-audit.md` |
| `memory.source_evidence` | Flagged to **add a default-closed `include_raw` gate** (currently releases raw text ungated) | `docs/product/mcp-tool-audit.md` |
| `CGDisplay::screenshot` capture path | Deprecated Apple API family; plan proposes replacing with ScreenCaptureKit one-shot capture (CAP-06) | `src-tauri/src/capture/macos.rs:106` |
| Dead/duplicate post-capture code | MEM-02 in the master plan calls for deleting or adopting dead and duplicate post-capture paths; each behavior test should be ported to the live function before deletion | master plan MEM-02 |
| Enrichment scheduling policy (MEM-06) | Merged but not wired into the live pipeline yet — needs a decision on when/whether to activate it | `src-tauri/src/capture/` |
