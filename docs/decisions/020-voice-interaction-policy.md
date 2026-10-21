# ADR 020: Voice interaction policy

## Status

Accepted policy; implementation pending — 2026-09-28

## Context

FNDR currently exposes separate voice implementations in Home, Search, Screen
Guide, Notch Ask, and Notch Do. Meetings also records speech in a hidden,
meeting-specific flow. Home search is intended to be the product's single
recall/search surface, but Home and Search still have separate recorders. Notch
Ask submits a transcript immediately, while Notch Do keeps a listener open,
routes final speech, and speaks responses. The current implementations differ
in microphone ownership, start and stop behavior, partial-text support, privacy
gating, and whether a final transcript is immediately acted on.

The partial VO-01 smoke baseline showed that the legacy local Whisper path has
a meaningful cold-start cost and that a single native path is needed before
voice is expanded. Search and both notch paths remain unmeasured; this decision
does not treat the partial baseline as a completed performance benchmark.

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
- **The current notch voice paths are experimental compatibility paths, not
  additional microphone owners approved by this policy.** GS-02 must either
  migrate Notch Ask and Notch Do to the shared voice session and confirmation
  rules, or disable them before Beta. The notch may remain a presentation
  surface; it may not keep an independent microphone stack.
- **Meetings remains a separate, explicitly started recording workflow.** It is
  not an interactive search or command microphone and does not run in the
  background merely because FNDR is open.
- No additional panel or background microphone control is introduced. A later
  command surface may subscribe to the shared voice stream; it does not create
  another microphone owner.

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

## Current implementation gaps

These rules are the target contract, not a description of current enforcement:

- Home drafts a final transcript for review, but Search can immediately search,
  navigate, pause, or resume after transcription.
- Screen Guide and Notch Ask immediately submit a completed transcript.
- Notch Do owns a continuous listener while visible and routes final utterances,
  including spoken approval, denial, and Stop.
- Screen Guide currently defaults `speak_responses` to `true`; this policy
  requires the default to become off and legacy/default state to be migrated
  without presenting it as explicit consent.
- The generic `transcribe_voice_input` path does not itself enforce incognito
  state. The shared owner must enforce the privacy gate before opening the
  microphone or accepting audio.

Until these gaps are implemented and verified, product copy and evidence must
not claim that confirmation-first, one-owner, incognito, or spoken-default
rules are already enforced.

## Consequences

- VO-02 defines one event stream and state model for these rules; VO-03 and
  VO-04 create the single native microphone owner.
- VO-06 builds shared voice UI. VO-07 and VO-08 adopt the same Home/Search
  control and must not create a second recorder or microphone affordance.
- VO-09 and GS-02 migrate or disable the existing Screen Guide and notch
  microphone owners. VO-09 also changes spoken answers to explicit opt-in.
- GS-11 applies the command risk policy. GS-13 receives only a user-confirmed
  final transcript. Screen text, OCR, and partial transcripts never become
  commands.
- The legacy Home/Search `MediaRecorder` implementations are temporary
  compatibility code, as are the independent Screen Guide and notch listeners;
  none is a model for new voice features.

## Verification

- Unit tests cover the state transitions: one active session, partial then
  final, cancel, permission denial, unavailable language asset, and error.
- Native QA confirms push-to-talk bounds capture, final text waits for explicit
  confirmation, and optional speech can be interrupted.
- Native QA confirms the Home/Search tap-to-toggle flow drafts text into the
  one query field and waits for explicit search confirmation.
- Native QA confirms Screen Guide and both notch modes share the same microphone
  owner, stop on teardown, obey incognito, and never act on an unconfirmed final
  transcript.
- A settings migration test proves spoken answers default to off without
  relabeling a legacy default as an explicit opt-in.

## Amendment 2026-10-06: Notch Do

Decided by Kunj for the Notch Do surface only (branch `kunj-notch-computer-use`). Home/Search, Screen Guide and Notch Ask keep the rules above.

- Notch Do uses the shared native voice owner (`voice/mod.rs`, surface `notch_do`). The WebKit listener and its spoken replies are removed; Notch Do does not speak.
- Opening the notch in Do mode starts listening. The notch ends the utterance after about 1.2 s without new partial text, or reports silence after 8 s with no speech.
- The final transcript and the planned steps appear on a plan card. The run starts by itself after 1.5 s unless the person says "stop" or taps Cancel; "go" or a tap starts it at once. The plan card is the review step this ADR requires.
- While a run is in progress the notch keeps listening. "Stop" (as partial or final text) or the Stop button kills the run, including an action in flight. Any other new final transcript stops the run and plans the new request.
- Microphone denied, no speech, and speech mid-run are explicit notch states.

## Amendment 2026-10-07: Notch Do start and approval (ADR 024)

Accepted by the owner. Two lines of the amendment above change:

- The plan card starts the run by itself only when no step in it can need a yes (opening an app, a link the person's words account for, playback in a media app). Any other plan waits for a tap or "go".
- Speech never approves an action. A pending approval is answered with a tap or a key; speech can decline it or stop the run. "Stop" is also heard behind a lead-in such as "please".

## Amendment 2026-10-08: Notch Do talks back

Directed by the owner ("computer use should work like Jarvis, pulling up things, talking back to me") and decided under the delegation rule in `AGENTS.md`. It replaces "Notch Do does not speak" in the 2026-10-06 amendment and, for Notch Do only, the spoken-answers and review rules above. Home/Search, Screen Guide and Notch Ask keep them: no spoken answers unless opted in.

- Notch Do speaks short plain status while it works: what it understood and how many steps, each step as it starts, a retry, a step that failed, anything it left out or refused, and the end. The end says how many steps FNDR checked itself and which are only as the model reported, and that nothing is undone automatically. The lines are composed in code from the run's state (`src/domains/notch/doNarration.ts`); no model writes them.
- It is on by default for Notch Do, with a mute switch in the notch (remembered in the webview, `fndr.notch.do.muted`). Muting, Stop, a new utterance, a failure of the microphone and closing the notch cancel speech at once.
- Speech is the webview's system-voice synthesis (`notchSpeech.ts`). It runs in the FNDR process, so there is no child process to orphan, and no text goes to a cloud service.
- The final transcript is still shown on the heard/plan card, which is the review step; it is no longer a separate gate. A plan in which no step can need a yes starts by itself after 1.5 s (unchanged, ADR 024). A plan that can need a yes waits for Start. Any ask-first action is announced aloud and needs a visible Allow; a spoken "no" declines, a spoken "yes" still approves nothing (ADR 024). The policy in `operator/policy.rs`, Private Mode and the blocklist are unchanged, and the person can always Stop, by voice or button.
- The microphone stays open while FNDR talks so "stop" works. Heard text that only repeats what FNDR just said is dropped as echo, except stop, no and go.
- One microphone owner is unchanged (`voice/mod.rs`, surface `notch_do`).

Follow-ups: a spoken yes/no for approvals needs a speaker-independent way to tell the person from the room, which the voice stack does not offer, so it is not built. A persisted Settings switch (today the mute is per webview) needs a field in `config.rs` and `set_screen_guide_settings`. Real audio output has not been heard in the packaged app.

## Amendment 2026-10-09: Notch Do is deaf while it works

Approved by the owner on 2026-10-09 in conversation ("when FNDR is working on a task the person asked for, it must not listen to other things"). It formalizes [the voice UX spec](../product/voice-ux.md) and replaces, for Notch Do only, the lines of the 2026-10-06, 2026-10-07 and 2026-10-08 amendments that it names. Home/Search, Screen Guide and Notch Ask are unchanged.

- **Deaf while working.** From the end of the request (the endpoint that leaves listening) until FNDR has finished speaking the result, the microphone listens only for the stop word. During that time no transcript is produced outside the speech helper, and none is shown, routed, queued or used as plan input. This covers planning, the chooser, the plan card, the run, an approval, the result and its readout. It replaces "the notch keeps listening" (2026-10-06) and "the microphone stays open while FNDR talks" (2026-10-08).
- **The stop-word spotter.** In that time the helper `fndr-speech` recognizes speech on the Mac, matches each segment against the stop vocabulary, and discards it. A segment counts only when it is, at most four words, `[lead-in] (stop | cancel) [it | that | now] [please]`, with the lead-in one of `fndr`, `hey fndr`, `please`, `okay`, `ok`, `no`. Anything else, including a sentence that contains "stop" and the words "wait", "pause", "hold on", "never mind" and "abort", is reported only as a `speech_ignored` event with no payload. No text leaves the helper in this mode; the native owner (`voice/mod.rs`, mode `stop_words`) drops any text a helper sends and forwards only `stop_word` and `speech_ignored`. English only for now.
- **Four interrupts, one action.** The Stop button, Esc, the global shortcut (Alt+N) and the spoken stop word all stop the run and cut speech. Cancel on the heard and plan cards is the same action. Esc on the approval card is Stop, not Don't. While a run is working or waiting on an approval, FNDR watches for Esc in other apps too, ignoring the key presses its own executor sends. Alt+N during a run stops it and leaves the panel open on "Stopped"; a second Alt+N closes it. Closing the panel during a run stops the run.
- **Spoken yes, no and go no longer act.** Start, a choice and an approval are taps or keys. A spoken "go" no longer starts a plan and a spoken "no" no longer declines an approval (both were allowed in 2026-10-07 and 2026-10-08). Spoken option names do nothing.
- **No queue.** Speech that is not the stop word while FNDR works is not kept. The notch shows "Working on it. Say stop to interrupt." for 2.5 s, at most once every 6 s. The mid-run "Switch to ...?" card is removed. The typed field is disabled in those states ("Working. Stop first to ask something new."); typing to narrow a work-set chooser stays, because typed words were read by the person who typed them and start no new request.
- **FNDR's spoken lines never contain a stop word.** Lines are run through `speakable()` before they are spoken, which replaces the words "stop" and "cancel" and the content the spec never reads aloud; a unit test runs every narration line through the stop-word matcher and expects no match. The screen keeps the full text.
- **Barge-in is the stop word only.** Any other speech while FNDR talks neither interrupts it nor is heard. Free barge-in waits for echo cancellation measured on real hardware (spec, P2).
- **The mute moves to `config.rs`** (`notch_do_muted`), so it survives restarts and holds in every window. The older webview flag `fndr.notch.do.muted` is migrated once and removed.

The echo guard (`isEchoOfSpeech`) stays for the open-microphone states.
