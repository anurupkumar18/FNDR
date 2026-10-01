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
