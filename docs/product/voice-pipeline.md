# Voice pipeline contract

Status: proposed by VO-02 — 2026-09-28  
Depends on: ADR 016, VO-01 baseline

## Purpose

FNDR has one voice pipeline. The Rust `voice` module is the only owner of a
microphone session and recognition. UI features subscribe to its state; they do
not create a `MediaRecorder`, call a transcription command directly, or own a
separate recognition lifecycle.

This contract applies to the two product voice surfaces defined in ADR 016:

- **Home/Search:** one shared recall field, tap once to start and tap again to
  stop.
- **Screen Guide:** contextual push-to-talk, held while the person speaks and
  stopped on release.

The existing browser recorder and Whisper commands are compatibility paths
until VO-03 through VO-10 replace them. They are not new API surface.

## Ownership and commands

The frontend invokes these Tauri commands. `surface` is shown in status UI and
diagnostics; it is not a permission to create a second session.

```ts
type VoiceSurface = "home_search" | "screen_guide";
type VoiceMode = "toggle" | "push_to_talk";

voice_start({ surface: VoiceSurface, mode: VoiceMode }): Promise<void>;
voice_stop({ sessionId: string }): Promise<void>;
voice_cancel({ sessionId: string }): Promise<void>;
```

- `voice_start` requests microphone and speech-recognition permissions when
  needed, starts the native helper, and allocates an opaque `sessionId`.
- `voice_stop` ends capture and allows the helper to emit a final transcript.
- `voice_cancel` ends capture, discards partial/final text for that session,
  and returns the stream to `idle`.
- Calls with a stale or unknown `sessionId` are no-ops. A caller must never
  stop or cancel another surface's later session.

There is exactly one active session. When a new `voice_start` arrives while a
different session is active, `voice` cancels the active session, emits its
`idle` state, then starts the new one. The UI must visibly name the active
surface and offer Stop/Cancel so this handoff is never hidden.

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
  | { kind: "unavailable"; reason: VoiceUnavailableReason };
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
idle → requesting_permission? → preparing_model → listening
     → final → idle
```

Normal push-to-talk flow is the same, except the press starts the session and
release calls `voice_stop`. Cancel, permission denial, missing language assets,
helper failure, or privacy refusal ends in `error` or `unavailable`, then
`idle`. A second start follows the single-session handoff described above.

## Errors, privacy, and fallback

`VoiceErrorCode` covers recoverable session failures: `permission_denied`,
`recording_failed`, `recognition_failed`, `helper_crashed`, and `cancelled`.
`VoiceUnavailableReason` covers a capability that cannot currently be used:
`private_context`, `speech_recognition_unavailable`, `language_asset_missing`,
and `platform_unsupported`.

Every error or unavailable state includes a concise, person-readable message
and leaves typed input available. Voice audio, level data, and partial text are
transient; they are not stored merely to recognize speech. Screen Guide retains
its existing private/sensitive-context refusal before starting a session.

## Surface behavior after a final transcript

- **Home/Search:** final text replaces the query draft and waits for Enter or
  the visible Search action.
- **Screen Guide:** final text appears in its question review UI and waits for
  explicit confirmation before the Guide request is sent.
- A future command consumer receives only a user-confirmed final transcript;
  it still applies the command risk policy.

## Implementation and test obligations

- VO-03 implements the native helper that streams partial and final text.
- VO-04 owns this state machine, helper lifecycle, event emission, and tests
  the normal, cancel, error, crash/restart, and unavailable paths with a fake
  helper.
- VO-05 declares and validates microphone and speech-recognition permissions.
- VO-06 consumes `voice://state` in shared `useVoice`, `VoiceButton`, and
  `VoiceStatus` UI. VO-07 and VO-08 use that shared control for the one
  Home/Search surface; VO-09 uses it for Screen Guide.

## Required review

Kunj must confirm that the user-confirmed final-transcript handoff is sufficient
for GS-13. Anurup must confirm that this contract can be packaged with the
native helper and app permission model. Their review is required before VO-02
is complete.
