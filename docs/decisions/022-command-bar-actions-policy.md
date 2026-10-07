# ADR 022: Command bar actions policy

## Status

Accepted: 2026-09-30

## Context

The command bar (GS-08) and voice commands (GS-13) let FNDR act on the Mac, not
just answer. ADR-014 made Screen Guide read-only. The agent surface already has
twelve developer-oriented action kinds and a policy table in
`src-tauri/src/agent/policy.rs`, but nothing states which actions a knowledge
worker's command bar may take. GS-01 cannot write the command-surface contract
until that is fixed.

## Decision

The command bar registers twelve tools in three tiers, listed with arguments
in `docs/product/actions-policy.md`.

- **Runs immediately:** `open_app`, `open_url`, `open_memory_source`,
  `reveal_file`, `search`, `about_this_screen`, `start_timer`, `pause_capture`.
- **One tap to confirm:** `paste_text`, `create_reminder`, `run_shortcut`,
  `resume_capture`.
- **Never this semester:** sending messages or email, deleting files or data,
  purchases, arbitrary shell, and any tool or argument sourced from screen
  text.

Risk levels live in the registry and are not model-selectable.

## Consequences

- GS-03 builds the registry from this list; GS-11 implements the policy check
  once, for native tools and MCP tools alike.
- The developer actions in `agent/actions.rs` are out of the command bar.
- `run_shortcut` is the exception to the no-side-effects rule, because a
  Shortcut is the person's own automation. W04 dogfood (GS-14) decides whether
  it stays one-tap.
- `resume_capture` is one-tap while `pause_capture` runs immediately: the
  asymmetry follows privacy direction, not effort.

## Alternatives considered

- **Everything one-tap.** Simpler to state, but it trains people to approve
  without reading and makes "open Slack" slower than clicking the Dock.
- **Allow send with confirm.** Rejected for this semester: the undo story is
  nil and a single misrouted voice command emails the wrong person.

## Related

ADR-014 (Screen Guide read-only), ADR-020 (voice interaction policy),
`docs/product/actions-policy.md`.

## Amendment 2026-10-06: Notch Do computer use

Decided by Kunj for Notch Do. The "never" tier above is unchanged and applies to computer use. Notch Do adds UI control through a computer-use MCP server, gated per tool call by `src-tauri/src/operator/policy.rs`:

- **Runs:** `open_app`, `open_url`, `wait_until_frontmost` (FNDR-native); reading the screen (`list_apps`, `get_app_state`, `select_text`); scrolling; clicks in media apps and browsers on an element whose label is known and not risky; typing into a search or address field; Return in a search or address field.
- **One confirmation:** typing anywhere else, Return outside a search field, clicks whose target is unknown or outside media apps and browsers, labels such as submit, sign in, post or save, `drag`, `set_value` outside a search field, and secondary actions.
- **Never:** sending (Send labels, Return or Cmd+Return in a messaging app), deleting, purchases, typing into a secure text field, any password manager or app on the sensitive-app list, and unknown tools.

The level is computed from the tool name, its arguments and FNDR's own view of the target element. Nothing the model says is an input, so model output cannot lower a level.

## Amendment 2026-10-07: Notch Do tiers tightened (ADR 024)

Accepted by the owner. The tiers above change as follows:

- **Browsers ask by default.** Runs: following a link, switching a tab, clicking a search box, play and pause, scrolling, navigation keys, typing and Return in a search box FNDR knows has focus. Every other click and key waits for a tap. A label FNDR cannot read (none, or another script) counts as unknown.
- **Links.** A planned link opens without asking only when the person's words account for it: a web search for words they said, or a site they named with nothing attached.
- **Typing.** Runs only while FNDR knows the search box has focus; a key press, a click by position or a changed screen reading ends that.
- **Never, added:** Terminal and other shells, Script Editor, Shortcuts, Automator, Wallet, Disk Utility, Activity Monitor, apps on the person's blocklist, and FNDR itself.
- **Before the plan:** a planning turn cannot use any tool.
- The actions kill switch and Private Mode refuse a plan and end a run in progress.
