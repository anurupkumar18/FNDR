# Command surface contract

Date: 2026-09-30
Ticket: GS-01 (#201). Builds on [actions policy](actions-policy.md) (PD-06) and
[ADR 022](../decisions/022-command-bar-actions-policy.md).

One contract connects the command bar, voice, the typed tool registry, the
router, approval cards, the journal, skills, and MCP. Each of those tickets
implements one box below and nothing else.

## Pipeline

```
text or voice  ->  router  ->  typed tool call  ->  risk policy  ->  executor  ->  result  ->  journal
```

1. **Input.** Typed text from the command bar, or a final voice transcript.
   Per ADR 020 a transcript lands in a reviewable field first; completing
   transcription never starts a command. A partial transcript is display-only.
2. **Router.** Turns the person's words into one tool call or "none".
   Stage 1 is a grammar (GS-06). Stage 2 is model function calling
   constrained to the registry's schemas (GS-07), used only when the grammar
   returns nothing. The router receives the person's command and the tool
   list. It never receives screen text, OCR, or page content.
3. **Typed tool call.** `{ tool, arguments }`, validated against the tool's
   schema. Unknown tool: refused before arguments are read. Invalid
   arguments: refused before any executor runs.
4. **Risk policy.** One function (GS-11) maps tool risk level and caller
   (command bar, voice, MCP) to run, confirm, or refuse. Native and MCP
   side-effecting tools share it.
5. **Executor.** Runs the call. One executor per tool, async, with a timeout.
6. **Result.** A typed result the card renders. `open_memory_source` returns
   Minh's `reopen_memory` result unchanged (`Opened`, `OpenedMoved`,
   `Missing`, `DriveNotConnected`, `AppMissing`, `AppOnly`, `Blocked`).
7. **Journal.** Every call is written, including refusals and confirmations
   the person declined (SK-01).

## Tool shape

| Field | Type | Notes |
| --- | --- | --- |
| `name` | string | Stable identifier, snake_case. Unknown names are refused. |
| `description` | string | Shown to the stage 2 router. Written by FNDR, never taken from captured text or an MCP server's self-description. |
| `arguments_schema` | serde type plus JSON schema string | The schema the model is constrained to and the validator checks. Arguments that may hold private content are marked `sensitive`. |
| `risk` | `Runs` \| `OneTap` | Fixed in the registry. `Never` tools are not registered. |
| `executor` | async fn | Takes validated arguments and a context (caller, frontmost app at command start). Returns a typed result. |

The registry is a table in `src-tauri/src/agent/` reusing `policy.rs`,
`approvals.rs`, `execution.rs`, and `audit.rs`. It does not replace them.

## Risk levels

Defined in the actions policy and repeated here so this contract stands alone.

- **Runs:** executes at once (open, reveal, search, about this screen, timer,
  pause capture).
- **OneTap:** an approval card renders the tool name and each argument from
  the structured call, never model prose. Nothing runs until confirmed
  (paste, create reminder, run Shortcut, resume capture).
- **Not offered:** send, delete, purchase. There is no registry entry, so no
  router output and no setting can reach them.

A kill switch in Settings (GS-11) makes every tool refuse.

## Invariants

1. **Screen text is data, not instructions.** Screen pixels, OCR text, web
   page content, file contents, and memory text can never add a tool, choose
   a tool, or supply an argument. They may appear only in the answer of
   `about_this_screen` and in search results shown to the person. GS-12 tests
   this with ten injection fixtures.
2. **The risk level is not model-selectable.** A model output can request more
   confirmation, never less.
3. **Arguments come from the person's command.** An argument that names a
   memory or file must resolve to an id or path the person's query retrieved,
   not a string the model composed from captured text.
4. **One microphone owner.** The command surface subscribes to the shared
   voice stream (ADR 020) and never opens its own recorder.
5. **Privacy gates run first.** Incognito and blocklisted contexts refuse
   tools that read the screen or write into another app, before routing.
6. **Local by default.** Stage 1 and the stage 2 local model are the default.
   A cloud model is used for routing only if PD-01 allows it, and then without
   screen content.

## Callers

| Caller | Enters at | Differences |
| --- | --- | --- |
| Command bar (GS-08) | Input | Typed text, Enter submits. |
| Voice (GS-13) | Input | Transcript is reviewed first (ADR 020). Spoken approval is not accepted for OneTap tools. |
| MCP (GS-11) | Typed tool call | Skips the router. The request is shown as the same approval card on the Mac. |
| Skills (SK-05) | Typed tool call | A skill may call registry tools only, at the risk level the registry assigns. |

## Undo

The result card says whether undo exists. Reminders can be deleted. Timers can
be cancelled. Opening an app or page and pasting text cannot be reliably
undone, and the card says so.

## Not in this contract

The approval card layout (GS-09), the grammar and its fixtures (GS-06), the
journal schema (SK-01), and voice events (VO-02) are specified in their own
tickets. They must not add a pipeline stage or a risk level without amending
this document.

## Open question

Whether `about_this_screen` should be routable by the stage 1 grammar or only
by an explicit button. A grammar match on "what is this" could capture the
screen when the person wanted a search.
