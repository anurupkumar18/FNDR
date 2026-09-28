# ADR 016: Voice interaction policy

## Status

Accepted — 2026-09-28

## Context

FNDR currently exposes voice in two user-facing places: the microphone beside
the Home search field, and Screen Guide. Although the code has Home and Search
recorder implementations, Home search is the product's single recall/search
surface—not two separate microphone capabilities. The current implementations
also differ in when they begin recording and whether a final transcript is
immediately acted on. The VO-01 smoke baseline showed that the legacy local
Whisper path has a meaningful cold-start cost and that a single native path is
needed before voice is expanded.

The UI/UX program's D-04 recommends Screen Guide as the primary voice surface,
and D-21 requires a person to review a final transcript before a voice query or
command executes. This decision supplies the product rules that VO-02 through
VO-10 implement.

## Decision

### Where voice is available

- **Home/Search is one shared recall surface.** It has one microphone beside
  the main query field. Tap once to start recording and tap again to stop; the
  final transcript drafts into that same query field. It does not submit a
  search automatically. Home and Search are product terminology for the same
  surface, even if they are currently separate components in code.
- **Screen Guide is a separate, contextual voice surface.** It uses
  push-to-talk: press and hold to listen, then release to stop. This bounds
  microphone capture to a deliberate gesture and matches the Guide's
  transient, on-screen question.
- No additional panel, notch, meeting, or background microphone control is
  introduced by this work. A later command surface may subscribe to the shared
  voice stream; it does not create another microphone owner.

### Transcript and action behavior

- A partial transcript is display-only. It may update the focused text field
  but cannot query, route, or execute anything.
- A final transcript is always placed in a reviewable text field or confirmation
  card. FNDR performs no search, Screen Guide question, navigation, or command
  merely because transcription completed.
- The person confirms a low-risk recall query with Enter or an explicit visible
  action. For a routed command, the command risk policy still decides whether
  the confirmed command runs, needs one additional approval, or is refused.
- New speech starts cancel the previous voice session. Only one session can
  own the microphone; the UI names the owner and offers Stop/Cancel.

### Spoken answers

- FNDR does **not** speak answers by default. Text remains the canonical answer
  and is always visible.
- A person may opt in to spoken Screen Guide answers in Settings. Starting a
  new request, pressing Stop, leaving the Guide flow, or disabling the setting
  interrupts speech. Spoken output never changes the answer or makes a command
  execute.

### Privacy and failure behavior

- Voice is unavailable while FNDR is in incognito/private mode or Screen Guide
  has refused the current context under its existing privacy policy.
- Permission denial, unavailable on-device language assets, recorder failure,
  cancellation, and recognition failure are explicit states with a typed
  reason and a text-input fallback.
- Voice audio, partial text, and final text follow the existing transient
  Screen Guide boundary: they are not persisted merely for recognition.

## Consequences

- VO-02 defines one event stream and state model for these rules; VO-03 and
  VO-04 create the single native microphone owner.
- VO-06 builds shared voice UI. VO-07 and VO-08 adopt the same Home/Search
  control and must not create a second recorder or microphone affordance.
- GS-13 receives only a user-confirmed final transcript. Screen text, OCR, and
  partial transcripts never become commands.
- The legacy Home/Search `MediaRecorder` implementations are temporary
  compatibility code, not a model for new voice features.

## Verification

- Unit tests cover the state transitions: one active session, partial then
  final, cancel, permission denial, unavailable language asset, and error.
- Native QA confirms push-to-talk bounds capture, final text waits for explicit
  confirmation, and optional speech can be interrupted.
- Native QA confirms the Home/Search tap-to-toggle flow drafts text into the
  one query field and waits for explicit search confirmation.
