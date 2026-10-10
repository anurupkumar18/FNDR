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
