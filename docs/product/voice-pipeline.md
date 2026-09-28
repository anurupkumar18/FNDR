# Voice pipeline contract

Status: proposed by VO-02; router review pending — 2026-09-28
Depends on: [ADR 020](../decisions/020-voice-interaction-policy.md),
[VO-01 partial baseline](../evidence/W02/voice-baseline.md)

## Purpose

FNDR has one interactive voice pipeline. The Rust `voice` module is the only
owner of an interactive microphone session and recognition. UI features
subscribe to its state; they do not create a `MediaRecorder`, call a
transcription command directly, or own a separate recognition lifecycle.
Meetings is the explicit recording-workflow exception described below.

This contract applies to the approved product voice surfaces defined in ADR
020 and records the migration boundary for the already-landed notch paths:

- **Home/Search:** one shared recall field, tap once to start and tap again to
  stop.
- **Screen Guide:** contextual push-to-talk, held while the person speaks and
  stopped on release.
- **Notch Ask and Notch Do:** existing experimental paths. GS-02 must migrate
  each one to this owner or disable it before Beta. Until that decision is
  implemented, the shared owner reports the notch surface as unavailable; the
  current independent listeners are not approved by this contract.
- **Meetings:** an explicitly started recording/transcription workflow, not an
  interactive voice surface. It remains outside `voice://state` and may not
  start merely because FNDR is open.

The existing Home, Search, Screen Guide, and notch browser recorders/listeners,
plus the Whisper commands they call, are compatibility paths until VO-03
through VO-10 and GS-02 replace or disable them. They are not new API surface.

## Ownership and commands

The frontend invokes these Tauri commands. `surface` is shown in status UI and
diagnostics; it is not a permission to create a second session.

```ts
type VoiceSurface = "home_search" | "screen_guide" | "notch_ask" | "notch_do";
type VoiceMode = "toggle" | "push_to_talk";
type VoiceSession = { sessionId: string };

voice_start({ surface: VoiceSurface, mode: VoiceMode }): Promise<VoiceSession>;
voice_stop({ sessionId: string }): Promise<void>;
voice_cancel({ sessionId: string }): Promise<void>;
```

- `voice_start` allocates and returns an opaque `sessionId` before asynchronous
  permission/model states are emitted. It validates the surface/mode pair,
  requests microphone and speech-recognition permissions when needed, and
  starts the native helper. A notch surface emits `unavailable` with
  `policy_not_enabled` until GS-02 enables a contract-compliant mode.
- `voice_stop` ends capture and allows the helper to emit a final transcript.
- `voice_cancel` ends capture, discards partial/final text for that session,
  emits `error { code: "cancelled", ... }`, and returns the stream to `idle`.
- Calls with a stale or unknown `sessionId` are no-ops. A caller must never
  stop or cancel another surface's later session.
- `voice_stop` only ends microphone capture and requests a final transcript.
  It is not the computer-use kill switch. An action-capable surface keeps a
  separate, always-visible Stop that interrupts pending/running actions and
  spoken output as well as cancelling its voice session.

There is exactly one active session. When a new `voice_start` arrives while a
different session is active, `voice` cancels the active session, emits its
terminal and `idle` states using the old `sessionId`, then returns the new
session. The UI must visibly name the active surface and offer Stop/Cancel so
this handoff is never hidden.

## `voice://state` event stream

`voice` emits every state transition through the Tauri event name
`voice://state`. Consumers ignore events whose `sessionId` is not the one they
started. All events use this envelope:

```ts
type VoiceStateEvent = {
  version: 1;
  sessionId: string | null;
  surface: VoiceSurface | null;
  state: VoiceState;
};
```

Every transition after `voice_start` carries that call's `sessionId`, including
the terminal `idle`. `sessionId: null` is reserved for the initial no-session
snapshot, so a late idle event cannot clear a newer surface's state.

`VoiceState` is exactly one of:

```ts
type VoiceState =
  | { kind: "idle" }
  | { kind: "requesting_permission"; permission: "microphone" | "speech_recognition" }
  | { kind: "preparing_model" }
  | { kind: "listening"; level: number }
  | { kind: "partial"; text: string }
  | { kind: "final"; text: string }
  | { kind: "error"; code: VoiceErrorCode; message: string }
  | { kind: "unavailable"; reason: VoiceUnavailableReason; message: string };
```

- `level` is normalized to `0` through `1`; it is for visual feedback only and
  must not be persisted.
- `partial.text` is display-only. It may fill the focused draft field but never
  searches, routes, navigates, or executes a command.
- `final.text` is the recognized final draft. The receiving surface displays it
  in a reviewable field or card; no action occurs solely because `final` was
  emitted.
- `idle` is emitted after final text has been delivered, after cancellation,
  and after terminal `error` or `unavailable` handling. A consumer retains the
  final text it needs before processing `idle`.

## State transitions

Normal toggle flow:

```text
idle → requesting_permission? → preparing_model? → listening
     → final → idle
```

The permission and model-preparation states are emitted only when needed.
Normal push-to-talk flow is otherwise the same, except the press starts the
session and release calls `voice_stop`. An explicit cancel discards buffered
text, emits a typed `cancelled` error, then emits `idle`. Permission denial,
missing language assets, helper failure, or privacy refusal ends in `error` or
`unavailable`, then `idle`. A second start follows the single-session handoff
described above.

## Errors, privacy, and fallback

`VoiceErrorCode` covers recoverable session endings: `permission_denied`,
`recording_failed`, `recognition_failed`, `helper_crashed`, and `cancelled`.
`VoiceUnavailableReason` covers a capability that cannot currently be used:
`private_context`, `speech_recognition_unavailable`, `language_asset_missing`,
`platform_unsupported`, and `policy_not_enabled`.

Every error or unavailable state includes a concise, person-readable message
and leaves typed input available. Voice audio, level data, and partial text are
transient; final text is also not persisted merely to recognize speech. Before
opening a microphone or accepting audio, the shared owner rejects incognito or
private context for every surface. Screen Guide retains its additional
sensitive-context refusal.

Only audio from the active, person-started session may produce partial or final
text. Screen text, OCR, web content, model output, and tool output never enter
the voice event stream or supply command text.

## Surface behavior after a final transcript

- **Home/Search:** final text replaces the query draft and waits for Enter or
  the visible Search action.
- **Screen Guide:** final text appears in its question review UI and waits for
  explicit confirmation before the Guide request is sent.
- **Notch Ask/Do:** they remain unavailable until GS-02 migrates or disables
  them. If migrated, Notch Ask must draft before asking, and Notch Do must show
  the final transcript before routing it; the normal command risk policy still
  applies after confirmation. The legacy continuous listener cannot bypass the
  shared owner.
- A future command consumer receives only a user-confirmed final transcript;
  it still applies the command risk policy.

## Spoken output

Text is always the canonical answer and spoken output defaults to off. A person
may explicitly enable spoken Screen Guide responses. Starting a new request,
pressing Stop, leaving the owning surface, entering private mode, or disabling
the setting interrupts speech. Spoken output is not a microphone owner and
never changes an answer, confirms a transcript, or executes a command.

## Current implementation boundary

This document is a target contract, not proof that main already implements it.
Current Home, Search, Screen Guide, Notch Ask, and Notch Do code has multiple
microphone owners; Search, Screen Guide, and both notch modes can act on final
speech without the review step above. The generic transcription command does
not enforce incognito on its own. Those paths remain non-compliant until their
owning VO/GS tickets replace or disable them.

## Implementation and test obligations

- VO-03 implements the native helper that streams partial and final text.
- VO-04 owns this state machine, helper lifecycle, event emission, and tests
  the normal, cancel, error, crash/restart, and unavailable paths with a fake
  helper. Unmount, app shutdown, private-mode entry, and surface changes cancel
  capture; late helper events are discarded by `sessionId`.
- VO-05 validates and refines the existing microphone and speech-recognition
  usage descriptions and runtime permission checks.
- VO-06 consumes `voice://state` in shared `useVoice`, `VoiceButton`, and
  `VoiceStatus` UI. VO-07 and VO-08 use that shared control for the one
  Home/Search surface; VO-09 uses it for Screen Guide.
- GS-02 migrates or disables Notch Ask and Notch Do. GS-11 and GS-13 ensure a
  confirmed final transcript still passes through the common command risk
  policy rather than executing directly.
- VO-09 changes spoken responses to explicit opt-in and verifies every required
  interruption path.

## Required review

Owner/integration review confirms that the contract can be packaged with the
native helper and app permission model. Kunj must still confirm that the
user-confirmed final-transcript handoff is sufficient for GS-13. Merging this
proposal does not complete VO-02 until that router review is recorded.
