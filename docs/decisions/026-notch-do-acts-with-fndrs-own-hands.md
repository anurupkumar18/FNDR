# ADR 026: Notch Do acts with FNDR's own accessibility code

## Status

Accepted 2026-10-08, by the owner. Direction for Final; the Beta rule in "Decision" applies now.

## Context

Notch Do plans with a Codex turn and acts through a separate computer-use helper that Codex attaches as an MCP server. FNDR decides every helper call in code before it runs (`operator/policy.rs`: runs, asks first, never). That only works when a call is one action FNDR can read: "click element 14", "type this text".

Two helpers were supported (`computer_use.rs`, `detect_backend`):

1. OpenAI's Computer Use, bundled with the ChatGPT app, through `computer-use-client-launcher`.
2. `open-computer-use`, a third-party npm package.

What was found on the owner's Mac on 2026-10-08:

- The bundled plugin (`computer-use` 1.0.1001365) no longer ships the launcher. Its folder holds only assets and skills.
- Its successor, `unified-computer-use` 26.930.61225, exposes one tool, `js`, that runs code the model writes. FNDR cannot decide such a call action by action, so the policy in ADR 022 and ADR 024 cannot be enforced on it.
- `open-computer-use` is not installed. So Notch Do cannot run on this Mac at all, and the settings copy told people to install the ChatGPT app, which does not help.

Two findings from Phase 1 also come from the helper being someone else's process: FNDR classifies a click from a cached label and cannot be sure which element the helper then acts on (C8.9), and typing can land in a field other than the one FNDR classified (N14).

Added the same day, after the owner installed `open-computer-use` 0.3.6 and Notch Do ran: with Screen Recording granted, the helper attaches a picture of the operated app's window to every read, and Codex passes it to the model. It has no switch for this. FNDR now says so and counts it in Privacy Activity, but cannot stop it (`docs/evidence/W03/agent-surfaces-phase1.md`, "Notch Do live runs").

## Options

| | A. Attach the new `js` tool | B. Depend on `open-computer-use` | C. FNDR acts itself |
|---|---|---|---|
| Per-action policy | Not possible | Works today | Works, and on the real element |
| Who FNDR trusts | Model-written code with full control | An unpinned npm package with Accessibility access | Its own code |
| Setup for a person | ChatGPT app | Node, a global npm install, an Accessibility grant to another binary | The Accessibility grant FNDR already asks for |
| C8.9 and N14 | Worse | Stay open | Closed by construction: the element FNDR judged is the element FNDR presses |
| Cost | Small | None | A new module, about a week, plus live testing |
| Survives OpenAI changing its plugin | No | Yes | Yes |

## Decision

**C for Final. B, unadvertised, for Beta. A never.**

1. FNDR never attaches a tool that runs model-written code. If the only Computer Use present is the `js` one, Notch Do says it has nothing to act with.
2. For Beta, Notch Do stays in Labs and off by default (ADR 024, E12). It runs only where `open-computer-use` is already installed. FNDR does not install it, does not bundle it, and the Beta demo does not depend on Notch Do.
3. With no usable helper, "Operate my Mac" cannot be turned on and says why, and the notch does not offer Do. A person is never sent to plan a request that can only fail. (Built 2026-10-08.)
4. For Final, FNDR serves the computer-use tools itself: a small MCP server inside the FNDR binary, attached to the Codex session in place of the helper, built on the accessibility code FNDR already has (`accessibility/mod.rs`, `accessibility/text_tree.rs`, the typing path Autofill uses). It keeps the tool names and argument shapes `operator/policy.rs` already classifies, so the policy and its tests do not change.
5. When item 4 passes the twenty-task set (`docs/evidence/W03/notch-do-task-set.md`), `open-computer-use` support is removed.

## Why

- The whole safety argument for Notch Do is that code, not a model, decides each action. A helper that takes code as input removes the thing being argued.
- A product whose promise is "your work stays on your Mac" should not need a person to install a second binary with full control of the Mac from a package registry.
- The executor is a small surface: read an app's element tree with indexes, press an element, set or type text, send a key, scroll. FNDR already reads trees and types into fields.
- It turns two open findings into non-issues instead of things to test around.

## Shape of the executor (for the build ticket)

```text
Codex session (plans, asks for tool calls)
  -> approval request to FNDR (unchanged)
       -> operator/policy.rs decides: runs / asks first / never (unchanged)
  -> fndr operator-mcp (new, stdio, FNDR's own binary)
       get_app_state(app)        -> indexed element tree, text only
       click(app, element_index) -> AXPress on the element FNDR indexed
       set_value / type_text     -> the Autofill typing path
       press_key, scroll
```

Rules the executor must keep:

- An element index is valid only for the tree it came from. A call that names an index from an older tree is refused, never guessed.
- It reads and acts only on the app named in the step, and never on FNDR, a blocklisted app, or anything while Private Mode is on (the checks in `Guards` today).
- It takes no screenshots. Text and structure only, as today.
- It runs as a child of FNDR and dies with it.

## Consequences

- New work for Final: the executor, its tests against the fixture pages in `src-tauri/tests/fixtures/operator/pages/`, and the twenty-task run.
- The Notch Do live checks owed from Phase 1 move to the executor. Running them earlier needs `open-computer-use` on the test Mac, which is the owner's choice to install.
- The code still looks for OpenAI's old launcher first (`bundled_computer_use`). It is harmless where the launcher is gone and is removed with item 5.

## Update 2026-10-08: item 4, first slice built

`fndr operator-mcp` exists (`src-tauri/src/operator/mcp.rs`, Mac side in `src-tauri/src/accessibility/operate.rs`, started from `main.rs`) and `detect_backend` prefers it (`Backend::Native`, label `fndr_native`). It serves `get_app_state`, `click`, `set_value`, `type_text`, `press_key`, `scroll` and `list_apps` with the argument shapes `policy.rs` reads; the policy and its tests are unchanged.

How each rule is kept:

- Index validity: indexes keep counting up across trees, so an index from an older tree is refused by number alone. Any action drops the app's tree and the next one needs a fresh `get_app_state`. Just before it acts, the server re-reads the element and refuses if it no longer says what was printed (the shifting-buttons case).
- Click is AXPress on the element's handle. Coordinates are refused. Nothing falls back to a screen position.
- FNDR, blocklisted apps and secure fields are refused inside the server as well as by the parent. The server reads the blocklist and the actions switch from settings at start and will not start if they cannot be read.
- The text of a text field is never printed, so a field's contents cannot change how policy reads it. Other elements print their value (`text Edit field = 57`).
- Only what a person can press, choose or type into gets a number (2026-10-09). Text, headings, images and labelled groups are printed without one, so words on a page can be read but can never be named as the target of an action. The server decides this from the role that leads the line (`ACTED_ON` in `mcp.rs`).
- An element an app reports under several parents is printed once (2026-10-09). Chrome lists its toolbar four times over; a live read of a Chrome window is now 40 lines in 0.12 s.
- No pixels are read. The process exits on end of input and polls its parent.

Honest limits:

- Private Mode is not visible to the child. It is enforced by the parent before every approved call (the existing `halt` guard), which holds because every tool is set to `prompt`.
- `press_key` and `scroll` go to the front app through System Events key events after FNDR has brought the named app forward and confirmed it. `scroll` is page keys, not wheel events. `type_text` pastes through the clipboard (the Autofill path without its select-all), which leaves the typed text on the clipboard.
- The old bundled launcher and open-computer-use remain as fallbacks, and can be forced with `FNDR_COMPUTER_USE`. They go when the twenty-task set passes (item 5).

Still open for item 4: a full Codex session through the native server (the `live_notch_do_runs_the_example_request` run with `FNDR_COMPUTER_USE=fndr_native`), the twenty-task set, the fixture pages in a browser, whether Codex's own process chain keeps the Accessibility grant of the FNDR binary, and wheel scrolling.

## Update 2026-10-09: window layout

After a work set opens a doc, a PDF and a page, FNDR can put them side by side. The executor gains four tools and a Rust API, with the same rules as the element tools.

Tools (`src-tauri/src/operator/mcp.rs`, Mac side in `src-tauri/src/accessibility/operate.rs`):

- `list_windows(app?)`: each window's id, app, title, frame and display, and each display's visible frame. Without an app it lists every app FNDR may operate.
- `arrange_windows(layout, windows: [{id, app}], display?)`: `layout` is one of `left_right_split`, `top_bottom_split`, `thirds`, `grid2x2`, `maximize`, `restore_previous`. The first window takes the left or top cell. The display defaults to the one holding most of the first window.
- `move_window(app, window, x, y)` and `resize_window(app, window, width, height)`, kept on the visible part of the display the window lands on.

How the rules are kept:

- A window id is valid only for the list it came from. Ids keep counting up, so an id from an older list is refused by number alone, and any window change drops the list. A window closed since the list is refused, never guessed. A window named under an app it does not belong to, a minimized window, a window named twice and more than six windows are refused.
- FNDR, the blocklist and the policy's sensitive apps (`policy::is_sensitive_app`) are never listed or moved, inside the server as well as by policy.
- Moves are AXPosition and AXSize on the window FNDR listed. Frames are clamped to the display's visible frame (menu bar and Dock excluded), read from NSScreen on the main thread. Each move is read back and reported, so an app that keeps a minimum size shows where its window really went.
- The frame a window had before FNDR first moved it is kept until `restore_previous` puts it back, however many layouts come in between.
- No pixels are read.

Policy (`operator/policy.rs`): `list_windows`, `arrange_windows`, `move_window` and `resize_window` run, because a layout changes nothing inside a window and can be undone with `restore_previous`. Each window in a layout names its app, so policy judges every app from the arguments alone: any sensitive app, more than six windows, no windows, or a layout FNDR does not know is never allowed; a window or a move without its app waits for a tap. The existing classifications are unchanged.

For code inside FNDR, `src-tauri/src/operator/layout.rs`:

```rust
pub fn list_windows(app: Option<&str>, limits: &Limits) -> Result<Vec<WindowRef>, String>
pub fn arrange(windows: &[WindowRef], layout: Layout) -> LayoutOutcome
```

`Limits::from_settings()` gives FNDR's own pid and the blocklist. The caller keeps the run's guards: nothing is listed or moved in Private Mode or with actions switched off. `LayoutOutcome` is `Refused(reason)` when nothing moved, or `Arranged(Vec<Placed>)` with each window's asked and read-back frame. The geometry (`frames`, `clamp`, `screen_holding`, `nearest_screen`) is pure and tested apart from the Mac.

Checked: unit tests for every layout and window count, clamping and two displays; fake-desktop tests for the tools, stale and foreign ids, off-limits apps and restore; the real binary over a pipe; and by hand on TextEdit through the real Accessibility API on 2026-10-09: two documents listed, split left and right (read back as 0,40 900x1129 and 900,40 900x1129 on an 1800-point display with a 40-point menu bar), a stale id refused, a move far off screen clamped to the visible corner, and `restore_previous` put both back to their exact first frames.

Not yet checked: a second physical display, the CoreGraphics fallback (used when the main thread does not answer within half a second; it guesses a 25-point menu bar and does not know the Dock), full-screen windows, and a Codex session that calls these tools.
