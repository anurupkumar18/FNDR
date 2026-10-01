# Command bar actions policy

Date: 2026-09-30
Decision: [ADR 022](../decisions/022-command-bar-actions-policy.md)
Ticket: PD-06 (#263). GS-01 (#201) builds the command-surface contract on this list.

The command bar can open, find, and set things up. It cannot send, delete,
or buy. A tool's risk level is fixed in the registry (GS-03), never chosen by a
model or by text on screen.

## Risk levels

| Level | Meaning |
| --- | --- |
| Runs | Executes as soon as the router picks it. The result is shown afterward and journaled. |
| One tap | An approval card shows the tool name and each argument from the structured call. Nothing runs until the person confirms. |
| Never | Not in the registry. The router cannot select it and no setting enables it this semester. |

## October tool list

| Tool | Arguments | Level | Why this level |
| --- | --- | --- | --- |
| `open_app` | name | Runs | Launches an app the person already has. Ambiguous names ask which one. |
| `open_url` | url (http or https only) | Runs | Opens a page in the default browser. Other schemes are refused. |
| `open_memory_source` | memory_id | Runs | Reopens something FNDR already captured. |
| `reveal_file` | path | Runs | Shows a file in Finder. Does not open or read it. |
| `search` | query | Runs | Read-only retrieval over local memory. |
| `about_this_screen` | question | Runs | Replaces read-only Screen Guide (ADR-014). Answers only, no side effects. |
| `start_timer` | minutes, label | Runs | A local notification and nothing else. |
| `pause_capture` | none | Runs | Reduces what FNDR collects, so it needs no friction. |
| `resume_capture` | none | One tap | Widens collection; the person should choose it on purpose. |
| `paste_text` | text | One tap | Writes into another app. Only into the app focused when the command started. Cannot be undone. |
| `create_reminder` | title, due | One tap | Writes to the person's Reminders. Can be deleted afterward. |
| `run_shortcut` | name | One tap | See the Shortcuts note below. |

Screen Guide's filename lookup (ADR-014) stays as it is: fixed roots, metadata
only, and it feeds `reveal_file` rather than opening anything.

### Shortcuts note

A Shortcut is the person's own automation and FNDR cannot see what it does. One
may send a message or delete files. The card therefore shows only the Shortcut
name, the approval is per run, and there is no "always allow". This is the one
place a one-tap tool can have effects the policy otherwise forbids, and the
person authored those effects. If the W04 dogfood week shows people approving
Shortcuts without reading the name, move `run_shortcut` to Never.

## What never ships this semester

- Sending messages or email, in any app, by any route.
- Deleting files or data, including FNDR's own memories from the command bar.
- Purchases, payments, or anything that moves money.
- Any tool, argument, or target that comes from screen text, OCR, or web page
  content. Captured text is data for answers, never input to the router.
- Arbitrary shell commands, and the developer actions in
  `src-tauri/src/agent/actions.rs` (`RunReadOnlyCommand`, `RunProjectTest`,
  `DelegateToHermes`, `DelegateToClaudeCode`, `ScheduleAgentJob`). They stay in
  the agent surface and are not registered as command-bar tools.
- Changing FNDR's privacy settings, blocklist, or incognito state, other than
  `pause_capture` above.

## Rules that hold for every tool

1. Unknown tool names are refused before any argument is read.
2. Arguments are validated against the tool's schema before an executor runs.
3. The approval card renders from the structured call, not model prose.
4. Incognito and blocklisted contexts refuse every tool that reads the screen
   (`about_this_screen`, `paste_text`).
5. Every call, including refusals, is journaled with its outcome (SK-01).
6. Nothing a model outputs can lower a tool's risk level; it can only ask for more confirmation.

## Open question

Whether `open_url` should confirm when the URL came from a memory rather than
the spoken command. The source is trusted today because the person captured it,
but a captured page can contain a link the person never typed.
