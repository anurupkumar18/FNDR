# Voice UX: talking to FNDR in the notch

Date: 2026-10-09. Scope: Notch Do (voice in, spoken progress out) and the
voice parts of Notch Ask. Builds on [ADR 020](../decisions/020-voice-interaction-policy.md)
and its three amendments, [ADR 024](../decisions/024-agent-surfaces-egress-and-action-policy.md)
and [ADR 026](../decisions/026-notch-do-acts-with-fndrs-own-hands.md). Where
this spec changes an amendment, the change is listed in "Decisions that need
an ADR amendment" and is not in force until that amendment is written.

## Problem

Today Notch Do keeps the microphone fully open for the whole run
(`wantsMicrophone` in `src/domains/notch/NotchOperator.tsx`). Any final
transcript heard while a run is in progress becomes a "Switch to ...?" card
(`redirectHeard` in `src/domains/notch/doRun.ts`), and "go", "no" and a dozen
other phrases are live commands. A television, a colleague or FNDR's own
voice can put words on the screen and into the next plan. The owner's rule
(2026-10-09): **when FNDR is working on a task the person asked for, it must
not listen to other things.**

## Goal

One predictable turn: the person speaks, sees what was heard, FNDR plans,
works and talks while it works, and the only thing the microphone can do
during that time is stop it. Text is canonical; speech is a copy of it.

## Non-goals

- Spoken approval. Approval stays a tap or a key (ADR 024 item 8).
- Free barge-in with arbitrary speech while FNDR talks. It needs echo
  cancellation that is verified on real hardware first (see Barge-in).
- A wake word. Voice starts only from an explicit gesture.
- Any cloud speech recognition or synthesis.

## Current code this spec is measured against

| Piece | File | What it does today |
| --- | --- | --- |
| Do reducer | `src/domains/notch/doRun.ts` | Phases `idle, listening, heard, silence, mic_denied, voice_unavailable, planning, plan, running, finished, failed, stopped`. `ENDPOINT_MS` 1200, `SILENCE_MS` 8000, `HEARD_MS` 1200, `AUTO_START_MS` 1500. Classifies stop, go, decline, request. |
| Narration | `src/domains/notch/doNarration.ts` | Lines composed from state; `isEchoOfSpeech` drops heard text that only repeats what was said. |
| Speaker | `src/domains/notch/notchSpeech.ts` | Webview `speechSynthesis`, 300 char cap, one pending line, 2 s echo tail, mute in `localStorage`. |
| Notch Do UI | `src/domains/notch/NotchOperator.tsx` | Mic open from listening through running; redirect card; typed input always enabled. |
| Notch shell | `src/domains/notch/NotchHud.tsx` | Ask and Do segmented control; Alt+N opens Do; Ask still uses the webview `MediaRecorder` path (`notchVoice.ts`). |
| Shared voice | `src/shared/voice/useVoice.ts` | One session per surface over `voice_start/stop/cancel`; events on `voice://state`. |
| Native owner | `src-tauri/src/voice/mod.rs` | One helper process, Private Mode refusal, surface and mode validation. |
| Helper | `src-tauri/helpers/fndr-speech/main.swift` | `SpeechAnalyzer` (macOS 26) or `SFSpeechRecognizer` with `requiresOnDeviceRecognition`; plain `AVAudioEngine`, no voice processing, no route-change handling. |

## The turn-taking state machine

Mic column values:

- **closed**: no audio is captured.
- **open**: full recognition; partial text is shown, the final transcript is used.
- **stop-only**: the stop-word spotter. Audio is recognized on-device inside
  the helper, matched there against the stop vocabulary, and discarded. No
  text leaves the helper process: Rust and the webview receive only
  `stop_word` or `speech_ignored` events with no payload.

| State | Maps to today | Mic | What the person sees | What FNDR says | Inputs that work | Leaves to |
| --- | --- | --- | --- | --- | --- | --- |
| idle | `idle` | closed | Notch mark; "Ask FNDR" on peek | nothing | Click, Alt+N | arming |
| arming | (new) | closed, opening | Orb fades in; "Getting the microphone ready" after 300 ms only | nothing | Esc, Alt+N cancel | listening, error |
| listening | `listening` | open | Waveform at rest; "Listening. Say what to do" | nothing | Speak, type, Esc, Alt+N | hearing, idle (silence), error |
| hearing | `listening` with partial | open | Waveform driven by level; live caption of partial text | nothing | Keep speaking, Esc cancels | thinking (endpoint), listening (cancel) |
| thinking | `heard` + `planning` | stop-only | Heard card with the transcript, then the orb working: "Planning" | nothing during heard; a soft tick on endpoint | Stop, Esc, Alt+N, "stop", Send now (heard only) | awaiting_choice, awaiting_start, working, error, interrupted |
| awaiting_choice | (new) | stop-only | Plan card with a chooser (numbered options) | "Which one? The choices are on screen." | Tap an option, keys 1 to 9, Return on focus, Stop, Esc, "stop" | awaiting_start, working, interrupted |
| awaiting_start | `plan`, `autoStart: false` | stop-only | Plan card, Start focused | "Understood: ... N steps. Some steps may ask first. Tap Start when you are ready." | Start, Return, Cancel, Esc, "stop" | working, interrupted |
| working | `plan` with `autoStart`, `running` | stop-only | Steps with the current one shimmering; mic-off chip "Mic off while working. Say stop to interrupt" | Each step as it starts, retries, refusals | Stop, Esc (global, see below), Alt+N, "stop" | awaiting_approval, finishing, interrupted, error |
| awaiting_approval | `running` with `approval` | stop-only | Approval card, **Don't** focused | "I need your okay to ... Tap Allow." | Allow, Don't, Return on focus, Stop, Esc, "stop" | working, interrupted |
| finishing | `finished` | stop-only | Result and the checked-steps note | The result line | Stop (cuts speech), "stop" | speaking |
| speaking | (new) | stop-only | Same card; orb pulses with speech | Remainder of the result readout | "stop", Stop, Esc cut speech only | listening (follow-up window) or idle |
| interrupted | `stopped` | closed | "Stopped. Nothing else will run." | "Stopped." once, only if the stop was spoken | Listen again, type, Esc closes | listening, idle |
| error | `failed`, `mic_denied`, `voice_unavailable`, `silence` | closed | Typed reason and one action (see journey) | The short reason, unless the mic failed | The action, type, Esc | listening, idle |

Rules the table encodes:

1. **Deaf while working.** From the endpoint of the request (entering
   thinking) until speaking ends, the mic is stop-only. Transcripts are not
   produced outside the helper, not displayed, not routed, not queued and not
   used as plan input. This covers thinking, awaiting_choice, awaiting_start,
   working, awaiting_approval, finishing and speaking.
2. **The only interrupts** in those states are the Stop button, Esc, the
   global shortcut (Alt+N) and the spoken stop word. Cancel on the heard and
   plan cards is the same action as Stop.
3. **Start, choice and approval are never spoken.** Spoken "go", "yes", "no"
   and option names do nothing. Return activates the focused button.
4. **One request at a time.** There is no queue (see below).
5. **Leaving the notch ends the run.** Closing the panel while a run is in
   progress stops it, as today. Alt+N during a run stops the run and leaves the
   panel open on "Stopped"; a second Alt+N closes it.

### The stop-word spotter (decision)

Chosen: a narrow on-device spotter, on by default, with a Settings switch
("Stop by voice").

Why it is needed: during working the operated app is frontmost, so the
notch does not have key focus and its Esc handler never fires; the person's
hands may be away from the trackpad, which is why they used voice. A stop
that needs a click is a stop that comes late.

Why it is safe: a false positive only stops a run, which is the safe
direction. A false negative leaves the Stop button, Esc and Alt+N.

Matching rule, applied inside `fndr-speech` to each recognizer segment that
is bounded by at least 300 ms of silence on both sides:

- Normalize: lowercase, strip punctuation.
- Accept only these shapes, at most four words: `[lead-in] (stop | cancel) [it | that | now] [please]`,
  where lead-in is one of `fndr`, `hey fndr`, `please`, `okay`, `ok`, `no`.
- Anything else, including a sentence that contains "stop" ("stop at the
  second tab and open settings"), is discarded and reported only as
  `speech_ignored`.
- Never matched: "wait", "pause", "hold on", "never mind", "abort". They are
  common in speech around a desk and in media. This narrows today's
  `STOP_PHRASES` in `doRun.ts`.
- English only for now; other locales get the button, Esc and Alt+N, and the
  Settings row says so.

What the person sees on a match: the run stops (same path as the button),
speech is cut, and the status reads "Stopped. Nothing else will run."

What the person sees on any other speech during a deaf state: the chip under
the status line pulses once and reads **"Working on it. Say stop to
interrupt."** for 2.5 s. It is not shown more than once every 6 s. Nothing of
what was said is shown, because nothing of it was kept.

### Second request while working (decision)

Chosen: **no queue**, with the cue above.

- A queued request was formed against a screen the running task is
  changing, and would run without the review ADR 020 requires.
- A one-slot queue means listening to full speech during work, which is the
  thing the owner ruled out.
- The person who wants something else says "stop", then asks. That is two
  words and one more turn, and it is the honest model of what happens.

The typed field follows the same rule: during deaf states it is disabled with
placeholder "Working. Stop first to ask something new." The redirect card
("Switch to ...?") and the `redirectHeard` / `redirectDismissed` inputs are
removed.

### Esc and the global shortcut during a run

- Esc in the notch (when it has key focus) stops the run in every deaf state.
  On the approval card Esc is Stop, not Don't: one key, one meaning.
- While a run is in working or awaiting_approval, FNDR installs a global Esc
  monitor (FNDR already holds the Accessibility grant Do requires). It ignores
  key events FNDR itself posts (`press_key` from the executor) by checking the
  event source process. It is removed when the run ends. Consequence, stated in
  onboarding: pressing Esc in another app during a run stops the run.
- Alt+N stops the run, as above.

## Journey

Each row: what the person sees, hears and can do, the microcopy, and the
failure path. Copy is final unless marked.

| # | Step | Sees | Hears | Can do | Microcopy | Failure path |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | First run, voice intro | Setup step "Talk to FNDR" with a mic illustration and three permission rows | nothing | Continue, Skip | "Hold Option N, say what you want done, and FNDR does it on this Mac. It listens only after you ask, and only for "stop" while it works." | Skip leaves Ask by typing working; Settings > Voice offers the same step later. |
| 2 | Microphone permission | macOS prompt, then row turns to "Allowed" | nothing | Allow, Don't Allow | Row: "Microphone: to hear your request. Audio stays on this Mac." | Denied: row reads "Microphone is off for FNDR" with "Open Microphone Settings" (`openSystemSettings("microphone")`). The notch shows the same and the type field. |
| 3 | Speech Recognition permission (pre macOS 26 path) | macOS prompt | nothing | Allow, Don't Allow | "Speech Recognition: turns your words into text on this Mac." | Denied: "Open Speech Recognition Settings". On macOS 26 the `SpeechAnalyzer` path may need the English model: "Downloading the on-device English speech model (about N MB)" with progress; failure: "The English speech model could not be downloaded. Try again on Wi-Fi, or type instead." |
| 4 | Notifications | macOS prompt | nothing | Allow, Not now | "Notifications: tells you when a task you started finishes while the notch is closed." | Off: no banner; the result waits in the notch. |
| 5 | Accessibility (computer use) | Row with "Open Accessibility Settings"; FNDR polls and turns the row green | nothing | Open settings, Skip | "Accessibility: lets FNDR press buttons and type in other apps when you ask it to. It never acts without showing you the plan first." | Not granted: "Operate my Mac" stays off with the reason (ADR 026 item 3); the notch offers Ask only. Screen Recording is not requested for Do. |
| 6 | Opening the notch | Panel opens; Do or Ask segmented control at top | nothing | Alt+N, click the notch | Peek: "Ask FNDR". | If Do cannot run, Do is hidden (current behavior). |
| 7 | Choosing Do versus Ask | Segmented control "Ask" / "Do"; Do selected after Alt+N | nothing | Click, Left/Right arrows on the control | Under the control, 12 px: Ask "Answers from your memory. Never acts." Do "Acts in your apps. Shows the plan first." | Ask uses the shared voice owner too (P1); until then its mic is the push-to-talk recorder. |
| 8 | Speaking | Waveform, "Listening. Say what to do" | nothing (no start chime; the visual is immediate) | Speak, type, Esc | "Listening. Say what to do" | Mic busy in another app: "Another app is using the microphone." with Try again. |
| 9 | Partial transcript | Live caption under the waveform, 2 lines max, oldest words scroll off | nothing | Keep talking | (the words) | Recognizer stalls more than 3 s with audio above the noise floor: "Still listening" so the person knows it has not frozen. |
| 10 | Endpointing | Caption freezes; tick sound | soft tick (optional, follows mute) | Nothing needed | none | Endpoint is the recognizer's final, or 900 ms with no new partial (down from 1200 ms, P1 after measuring). Hard cap 20 s of speech: "That was long. Here is what I heard." |
| 11 | Heard card | "Heard:" and the transcript; 1.2 s bar | nothing | Send now, Cancel, Esc, "stop", edit by typing | "Heard this. Say stop if it is wrong" | Empty final: back to listening with "I didn't catch that." |
| 12 | Planning | Orb working; "Planning" or "Planning with 3 memories" | nothing | Stop, Esc, "stop" | "Planning. Say stop to cancel" | Planning exceeds 12 s: "Still planning" with Stop emphasized. Failure: error state, row 20. |
| 13 | Plan card with chooser | Numbered options when the plan needs one input it could not resolve ("Which Slack channel?") | "Which one? The choices are on screen." | Tap, 1 to 9, Stop | Title is the question; options are names only | Choice not made in 60 s: run is cancelled with "Cancelled: no choice was made. Nothing ran." |
| 14 | Auto-start | Plan card with the 1.5 s countdown bar | "Understood: ... N steps. Starting." | Cancel, Stop, "stop" | "Starting. Say stop to cancel" | Only plans with no step that can need a yes (ADR 024 E3). |
| 15 | Start | Plan card, Start focused | "... Some steps may ask first. Tap Start when you are ready." | Start, Return, Cancel | "Ready. Press Start" | No "go" by voice. Unstarted after 2 min: cancelled with "Plan expired. Nothing ran." |
| 16 | Running with spoken progress | Step list, current step shimmering, last 4 actions | "Step two of four: open Safari." | Stop, Esc, Alt+N, "stop", Mute voice | Chip: "Mic off while working. Say stop to interrupt" | A step fails: "That step did not work: ..."; retry: "Trying that step again." |
| 17 | Ask-first approval | Approval card, Don't focused, amber edge | "I need your okay to ... Tap Allow." | Allow, Don't, Return, Esc (= Stop) | "Okay to <action>?" Buttons "Don't" and "Allow" | No answer in 60 s: treated as Don't, "Skipped: no answer." |
| 18 | Finished result | Result summary, checked note, "Nothing here is undone automatically." | "Done. ... I checked every step myself. Nothing is undone automatically." | Listen again, type, Esc | Result line from `resultNote` | Notch closed when it finishes: notification "FNDR finished: <summary first 60 chars>" if allowed. |
| 19 | Interrupted run | "Stopped. Nothing else will run." and the steps with their final status | "Stopped." (only after a spoken stop) | Listen again, type | | An action already sent to an app cannot be recalled; the step shows "Stopped during this step. Check <app>." |
| 20 | Error | Typed reason, one action | Short reason | The action | e.g. "I could not plan that: <reason>" | Never a raw stack trace; message capped at 140 chars. |
| 21 | Offline | Listening works (on-device). Planning fails fast | "I can't plan without a connection. Nothing was sent." | Try again, switch to Ask | Same | The transcript is put in the type field so retrying costs no speech. |
| 22 | Signed out of ChatGPT | Error with Reconnect | "I need you to sign in to ChatGPT again." | Reconnect ChatGPT | "Signed out of ChatGPT. Reconnect to plan." | Transcript kept in the field. |
| 23 | Over the plan limit | Error | "Your ChatGPT limit is used up." | Ask, type | "ChatGPT limit reached. Ask still works on this Mac." | No local fallback for acting; Ask is offered instead. |
| 24 | Private Mode | Notch Do: "Private Mode is on. FNDR isn't listening." Mic glyph slashed | nothing | Type in Ask only | as left | Turned on mid-run: run stops (ADR 024 item 9), mic closes, speech is cut, status "Stopped: Private Mode turned on." |
| 25 | Second request while working | Chip pulse "Working on it. Say stop to interrupt." | nothing | Say stop, press Stop | as left | See "Second request while working". |
| 26 | Long silence | After 8 s with no speech: "Didn't hear anything." Mic closes | nothing | Listen again, type | as left | Unchanged from today (`SILENCE_MS`). |
| 27 | Background noise | Caption fills with words the person did not say, or the level stays high with no words | nothing | Esc, type | After 15 s with no endpoint: "It's noisy here. Type instead, or try again closer to the mic." | The heard card is the backstop: nothing is planned without 1.2 s on screen and a stop path. |
| 28 | Headphones or Bluetooth route change | Toast in the notch for 2 s: "Using AirPods microphone" | nothing | Nothing | as left | The helper restarts its engine on `AVAudioEngineConfigurationChange` within 500 ms. If restart fails: error "The microphone changed and could not be reopened." with Listen again. Bluetooth headphones: FNDR prefers the built-in mic so music does not drop to call quality (setting, row 37). |
| 29 | Multiple displays | Notch on the display that holds the pointer when summoned; it stays there for the run | nothing | | | The operated app may be on another display; the step label names the app. Non-notched display: pill style (existing `styleFor`). |
| 30 | FNDR in full-screen Spaces | Notch visible over another app's full-screen Space (FullScreenAuxiliary) | nothing | | | If the notch cannot show (macOS refused), the notification and the global Esc and Alt+N still work. |

## Barge-in while FNDR is speaking

Policy for v1: **stop-only barge-in.** While FNDR speaks, the spotter is
armed; a match cuts speech and stops the run. Any other speech does not
interrupt FNDR and is not heard.

Echo risk: FNDR's voice reaches the microphone. Today `isEchoOfSpeech` drops
heard text whose words all appeared in recent narration, with a 2 s tail
(`notchSpeech.ts`). With stop-only listening most of that risk disappears,
leaving one: FNDR saying a stop word. Guards:

1. **Narration never contains a stop word.** No spoken line includes "stop"
   or "cancel" (today the plan line can say "Say stop" in the status text but
   not in speech; keep it that way). A unit test runs every `narrate` output
   through the spotter's matcher and expects no match.
2. **Duck then cut.** When the helper detects voice onset while FNDR is
   speaking, the speaker drops to 30 % volume within 50 ms. If the segment
   matches, speech is cut; if it ends without a match, volume returns over
   200 ms.
3. **Voice processing (AEC).** Enable `AVAudioInputNode.setVoiceProcessingEnabled(true)`
   in `fndr-speech` and move FNDR's speech output into the same helper
   (`AVSpeechSynthesizer` rendering through the helper's engine) so the echo
   canceller has FNDR's own output as its reference. Webview
   `speechSynthesis` plays through a path the canceller cannot see. Until this
   is measured on the owner's Mac with built-in speakers, AirPods and an
   external display's speakers, full barge-in stays off.

Full barge-in (any speech interrupts FNDR and becomes the next request) is P2
and is allowed only outside deaf states, after the AEC measurement shows the
echo guard is no longer needed.

### Ducking other media

- While **listening and hearing**, other apps' audio is ducked through
  `voiceProcessingOtherAudioDuckingConfiguration` (minimum level), so the
  request is heard over music.
- While **deaf**, other audio is not ducked: the task may be "play my focus
  playlist". The spotter tolerates music; a lyric that is exactly "stop" can
  stop a run, which is the safe direction.
- FNDR's own speech is not ducked by FNDR.

## Latency budgets

Measured from the helper's timestamps and the webview's `performance.now()`;
the budget is p50 / p95 on the owner's Mac, warm.

| Interval | p50 | p95 | Today |
| --- | --- | --- | --- |
| Alt+N to waveform visible | 150 ms | 300 ms | unmeasured |
| Alt+N to mic actually capturing | 400 ms | 900 ms | unmeasured |
| End of speech to heard card | 900 ms | 1300 ms | 1200 ms endpoint plus recognizer |
| End of speech to tick sound | 950 ms | 1350 ms | none |
| Heard card to plan request sent | 1200 ms | 1200 ms | `HEARD_MS` |
| Plan request to plan card | 4 s | 10 s | unmeasured, network bound |
| Plan card to first audible word | 300 ms | 600 ms | unmeasured |
| Stop button, Esc or Alt+N to silence | 100 ms | 150 ms | `speechSynthesis.cancel` |
| Spoken "stop" (end of word) to silence | 350 ms | 600 ms | partial-text path |
| Any stop to no further action in another app | 300 ms | 500 ms | `computerUseStop` |

## Speech chunking and what is never read aloud

- One line is one utterance of at most 140 characters, cut at a sentence end;
  a longer line is split into sentences, each queued.
- Progress lines are not urgent and coalesce: a newer step line replaces a
  waiting one (today's single `pending` slot). Approval, refusal, result and
  failure lines are urgent and cut what is playing.
- Numbers up to ten are spelled out (existing `num`).

Never read aloud, enforced by one `speakable(text)` function in
`doNarration.ts` with tests; the full text stays on screen:

| Content | Spoken as |
| --- | --- |
| Anything from a secure field, or a token-shaped string (20+ chars of base64 or hex, `sk-`, `ghp_`, `AKIA` prefixes) | "a hidden value" |
| URLs | "a link on <host>" |
| Email addresses | "an email address" |
| Digit runs longer than six | "a long number" |
| File paths | the last path component |
| Lists of more than three items | "<n> items, shown on screen" |
| Raw memory text, OCR text, page text | never; "from a memory in <app>, <relative day>" |
| Model prose that is not a step label, result summary or error | never |

## Visual language in the notch

| State | Visual | Reduced motion |
| --- | --- | --- |
| idle | Notch mark, no animation | same |
| arming | Orb fades in over 150 ms | appears at full opacity |
| listening | `VoiceBeam` at rest glow | static ring |
| hearing | `VoiceBeam` driven by input level; caption | three-step level bar, no glow animation |
| thinking | `ThinkingOrb state="working"` | static orb with "Planning" |
| awaiting_choice / awaiting_start | Orb still; plan card; countdown bar only on auto-start | countdown as text "Starting in 1 s" |
| working | Current step row shimmer (left to right, 1.6 s loop); mic-off chip | current step bold with a filled dot, no shimmer |
| awaiting_approval | Card with 2 px amber left edge; orb still | same |
| speaking | Orb pulse on each word boundary (`onboundary`) | no pulse; a speaker glyph next to the caption |
| interrupted | Orb fades out; steps dim except the stopped one | no fade |
| error | Red-tinted mark, never flashing | same |

- **Captions always visible.** Every spoken line is also the status line text
  (`role="status"`), including progress. Muting voice changes nothing on
  screen.
- **VoiceOver.** The status line is `aria-live="polite"`; the approval and
  chooser cards are `role="alertdialog"` with focus moved to the safe button
  (Don't) or the first option. When VoiceOver is running (read from Rust with
  `NSWorkspace.isVoiceOverEnabled`), FNDR's own speech defaults to muted so
  two voices do not talk over each other; the live region carries the same
  lines. The mic-off chip is announced once per run, not on every pulse.
- Contrast: notch text on the black panel at least 4.5:1; the amber edge and
  dots at least 3:1 against the panel.

## Settings and onboarding copy

Settings > Voice (new group, persisted in `config.rs`; replaces the webview
`fndr.notch.do.muted` key, migrating its value once):

| Control | Default | Label and help |
| --- | --- | --- |
| Speak while working | On | "Speak while working. FNDR says each step out loud. Everything it says is also on screen." |
| Stop by voice | On | "Stop by voice. While FNDR works, the mic listens only for "stop" or "cancel", on this Mac, and ignores everything else. English only." |
| Voice | System default | "Voice. Uses the voices installed on this Mac." with a Preview button that says "This is how FNDR sounds." |
| Microphone | Automatic | "Microphone. Automatic uses the built-in mic when you wear Bluetooth headphones, so your music keeps its quality." Options: Automatic, each input device. |
| Lower other audio while listening | On | "Lower other audio while listening." |

Onboarding step "Talk to FNDR" (row 1 of the journey), with three bullets:

- "Press Option N and say what you want done."
- "FNDR shows what it heard and the plan before anything happens."
- "While it works, it only listens for "stop". Esc and the Stop button work too."

## Decisions that need an ADR amendment

These change ADR 020's 2026-10-06 and 2026-10-08 amendments for Notch Do and
must be recorded there before P0 ships:

1. Mic is stop-only from endpoint through speaking (was: open through the run).
2. Spoken "go" no longer starts a plan; spoken "no" no longer declines (Start
   and Don't are taps or keys). Spoken stop words narrow to stop and cancel.
3. The mid-run redirect card is removed; no queue.
4. The mute moves from the webview to `config.rs`.

## Implementation backlog

### P0: the owner's rule

| Item | Files | Done when |
| --- | --- | --- |
| P0.1 ADR 020 amendment with the four decisions above | `docs/decisions/020-voice-interaction-policy.md` | Amendment merged |
| P0.2 Stop-only mode in the native owner: new `VoiceMode::StopWords`, valid for `notch_do`; in this mode Rust drops any `partial` or `final` the helper sends and forwards only `stop_word` and `speech_ignored` | `src-tauri/src/voice/mod.rs` | Unit test: a helper line `{"type":"partial"}` in stop-only mode emits nothing |
| P0.3 Spotter in the helper: `spot` command, matcher from "Matching rule", emits `stop_word` / `speech_ignored` with no text | `src-tauri/helpers/fndr-speech/main.swift`, new `StopWordMatcher.swift` and a test file beside `streaming-transcript-test.swift` | Matcher test covers the accepted shapes and twenty rejected sentences |
| P0.4 `useVoice` accepts `mode: "stop_words"` and exposes `onStopWord` and `onIgnoredSpeech`; new `VoiceState` kinds | `src/shared/voice/useVoice.ts`, `src/shared/voice/useVoice.test.tsx` | Test: stop-only session never calls `onPartial` |
| P0.5 Reducer: remove `redirect`, `redirectHeard`, `redirectDismissed`; `classifyUtterance` keeps request routing only for open-mic states; `STOP_PHRASES` narrowed | `src/domains/notch/doRun.ts`, `doRun.test.ts` | Tests for each deaf state rejecting a request input |
| P0.6 Notch Do switches sessions: open mic in listening and hearing, stop-only from heard through speaking; typed field disabled in deaf states; the "Working on it" chip | `src/domains/notch/NotchOperator.tsx`, `src/domains/notch/Notch.css`, `src/domains/notch/NotchHud.test.tsx` | Test: a partial during running changes nothing on screen |
| P0.7 Esc stops in every deaf state; approval card focuses Don't; Alt+N during a run stops instead of closing | `NotchOperator.tsx`, `NotchHud.tsx`, `src-tauri/src/ipc/commands/notch.rs` (`shortcut_action`) | Tests in `NotchHud.test.tsx` and `notch.rs` |
| P0.8 Narration lint: no stop word in any spoken line | `src/domains/notch/doNarration.test.ts` | Test passes over all `narrate` branches |

### P1: natural voice

| Item | Files |
| --- | --- |
| P1.1 `speakable()` redaction table above | `src/domains/notch/doNarration.ts`, `doNarration.test.ts` |
| P1.2 Global Esc monitor during working and approval, ignoring FNDR-posted events | `src-tauri/src/ipc/commands/notch.rs`, `src-tauri/src/operator/mod.rs` |
| P1.3 Route change handling and the "Using <device>" toast; Automatic microphone choice | `main.swift`, `src-tauri/src/voice/mod.rs`, `useVoice.ts`, `NotchOperator.tsx` |
| P1.4 Voice settings group persisted, mute migration | `src-tauri/src/config.rs`, `src/domains/screen-guide/ScreenGuidePanel.tsx` or the Settings voice section, `notchSpeech.ts` |
| P1.5 Notch Ask on the shared voice owner (`notch_ask`, toggle), retire `notchVoice.ts` | `NotchHud.tsx`, `notchVoice.ts` (delete), `src-tauri/src/voice/mod.rs` |
| P1.6 Chooser card (awaiting_choice) and the plan expiry timers | `doRun.ts`, `NotchOperator.tsx`, `src-tauri/src/operator/plan.rs` |
| P1.7 Latency instrumentation for the budget table | `useVoice.ts`, `NotchOperator.tsx`, `src/shared/activity/voiceActivityTrace.ts` |
| P1.8 Reduced-motion variants and the working shimmer | `Notch.css` |
| P1.9 VoiceOver detection and default mute | `src-tauri/src/voice/mod.rs`, `notchSpeech.ts` |

### P2: after measurement

| Item | Files |
| --- | --- |
| P2.1 Speech output moved into `fndr-speech` with voice processing on, measured on three output routes | `main.swift`, `notchSpeech.ts` (becomes a thin client), `voice/mod.rs` |
| P2.2 Duck then cut on voice onset | `main.swift`, `notchSpeech.ts` |
| P2.3 Full barge-in outside deaf states | `NotchOperator.tsx`, `doRun.ts` |
| P2.4 Stop words in other languages | `StopWordMatcher.swift` |
| P2.5 Endpoint 1200 ms to 900 ms once p95 recognizer final latency is known | `doRun.ts` (`ENDPOINT_MS`) |

## Open question

Whether macOS's voice-processing echo canceller removes speech the webview
plays through `speechSynthesis` at all. This spec assumes it does not, which
is why P2.1 moves synthesis into the helper; one recording on the owner's Mac
with built-in speakers settles it before P2.1 is scheduled.
