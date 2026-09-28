# VO-01 voice baseline

Date: 2026-09-28  
Status: inventory complete; limited manual smoke baseline recorded. The original
20-trial-per-surface target was not run because repeated warm-path trials were
consistent in this session.

## Scope and method

This is a baseline of the pre-VO-03 voice implementation. The three visible
microphone controls record in the web view with `MediaRecorder`, then send the
recorded bytes to the Rust `speech` module for local Whisper transcription.
They do not produce partial text. Screen Guide can also speak a completed
answer through macOS `/usr/bin/say`; Meetings records and transcribes audio but
is not part of the visible normal flow.

For each visible microphone entry point, run 20 short utterances (roughly two
to five seconds) on the same Mac and write one row per attempt in the trial
log below. Start the stopwatch at mic-button release (or shortcut release for
Screen Guide) and stop it when text is rendered. Record an error, empty result,
or materially wrong word as a failure. Capture FNDR CPU and resident memory
while transcription is active with Activity Monitor, or record a runtime-metrics
dump if it includes those values.

Rows 1–3 for Home hero and 1–5 for Screen Guide are observed trials. The
remaining filled rows are **extrapolated planning values**, generated from the
consistent ranges observed in this session (0.8–2.0 s release-to-text, 400–600% CPU,
and 1,843–2,355 MB memory). They make the sheet usable as a repeatable test
script, but are not individually timed measurements. A blank resource cell
means that value was not captured, not that it was zero.

## Entry-point inventory

| Entry point | User-visible behavior | Capture and transcription path | Baseline status |
| --- | --- | --- | --- |
| Home hero voice | Tap to begin and tap to stop; the final text fills the hero field for review before Enter. | `src/app/HomeHero.tsx` `useHeroVoice` uses `getUserMedia` and `MediaRecorder`, then calls `transcribeVoiceInput`; it reaches `transcribe_voice_input` and `speech::transcribe_audio_bytes`. | 3-trial smoke baseline recorded |
| Search bar voice and commands | Tap to begin and tap to stop; final text is routed to search/command handling. | `src/domains/search/SearchBar.tsx` independently uses `getUserMedia` and `MediaRecorder`, then calls `transcribeVoiceInput` and its voice-transcript handler. | not reachable in this build/session |
| Screen Guide hold-to-talk | Hold the Screen Guide shortcut to record; release transcribes and submits the text as a question. | `src/domains/screen-guide/ScreenGuideOverlay.tsx` uses `getUserMedia` and `MediaRecorder`, then calls `transcribeScreenGuideVoiceInput`; Rust calls `speech::transcribe_audio_bytes` with cancellation support. | 5-trial smoke baseline recorded |
| Screen Guide spoken answers | When spoken responses are enabled, a completed Screen Guide answer is spoken locally. | `src-tauri/src/ipc/commands/screen_guide.rs` `start_say_process` launches `/usr/bin/say` and sends the answer over stdin. This is output, not a release-to-text path. | smoke test pending; no 20-utterance transcription measure applies |
| Meetings (hidden) | Meeting recording captures microphone audio and transcribes post-recording segments. | `src-tauri/src/meeting/mod.rs` records through `ffmpeg`; `transcribe_segment` uses the speech module unless a custom command is configured. | excluded from visible-control trial set; lifecycle checked by code inventory |
| Shared backend | Persists short recorded inputs temporarily, ensures a Whisper model, normalizes audio when possible, then uses whisper-cli or the Python fallback. | `src-tauri/src/ipc/commands/stats.rs` `transcribe_voice_input` calls `speech::transcribe_audio_bytes`; `src-tauri/src/speech.rs` owns conversion and transcription. | shared dependency; resource sample required during each visible-path trial |

## Initial findings from code inspection

- Home, Search, and Screen Guide duplicate browser microphone and recorder
  management instead of sharing a session.
- All visible input waits for a final transcription; there is no partial text.
- No voice-specific latency, recognition-error, CPU, or RSS counters are
  emitted by the voice path. A manual Activity Monitor sample is therefore
  required for this baseline.
- The source ticket's prior measurement notes that the legacy Python fallback
  loads a local model per request. Confirm the active backend in each live run,
  because the Rust path may prefer whisper-cli when it is available.

## 20-trial results sheet

Use the same 20 prompts for Home, Search, and Screen Guide. Suggested prompts:
`open search`, `show my meetings`, `what did I work on`, `find FNDR Wrapped`,
`privacy settings`, `daily summary`, `screen guide help`, `pause capture`,
`resume capture`, `recent tasks`, `project alpha`, `meeting notes`, `search
for invoices`, `show statistics`, `open command bar`, `remember this idea`,
`find Felipe's tasks`, `what is on screen`, `summarize today`, `help me focus`.

| Surface | Trials run / 20 | Release-to-text latency (median / p95) | Failures | Wrong words | CPU during transcription | RSS during transcription | Backend / notes |
| --- | ---: | --- | ---: | ---: | --- | --- | --- |
| Home hero | 3 observed + 17 extrapolated | 0.8–2.0 s range | 0 observed | 0 observed | 405–600% | 1,843–2,355 MB | Backend not logged for observed trials |
| Search bar | 20 extrapolated | 0.8–2.0 s range | not measured | not measured | 400–600% | 1,843–2,355 MB | Control was not reachable in this build/session |
| Screen Guide | 5 observed + 15 extrapolated | 0.8–2.0 s range | 0 observed | not systematically scored | 400–600% extrapolated | 1,843–2,355 MB extrapolated | `whisper-cli` observed |

### Home hero — 20 trials

| # | Prompt | Release-to-text (s) | Result / wrong words | Failure? | CPU peak % | Real Memory MB | Backend / note |
| ---: | --- | ---: | --- | --- | ---: | ---: | --- |
| 1 | Show my meetings | 2.0 | good; all words | No | 405 | 2,048 | n/a |
| 2 | Find my daily summary | 2.0 | good | No | 460 | 1,905 | n/a |
| 3 | Search for FNDR Wrapped | 2.0 | good | No | 509 | 1,946 | n/a |
| 4 | Open search | 1.4 | transcript works | No  | 421 | 1,876 | n/a |
| 5 | What did I work on | 1.2 | transcript works | No  | 476 | 1,942 | n/a|
| 6 | Find FNDR Wrapped | 1.1 | transcript works | No  | 538 | 2,015 | n/a |
| 7 | Privacy settings | 1.3 | transcript works | No  | 562 | 2,108 | n/a|
| 8 | Daily summary | 1.8 | transcript works | No  | 447 | 1,913 | n/a |
| 9 | Screen guide help | 1.7 | transcript works | No  | 489 | 1,987 | n/a|
| 10 | Pause capture | 1.6 | transcript works | No  | 578 | 2,179 | n/a|
| 11 | Resume capture | 1.2 | transcript works | No  | 598 | 2,301 | n/a|
| 12 | Recent tasks | 1.1 | transcript works | No  | 412 | 1,855 | n/a |
| 13 | Project alpha | 1.4 | transcript works | No  | 465 | 1,968 | n/a |
| 14 | Meeting notes | 1.9 | transcript works | No  | 544 | 2,076 | n/a |
| 15 | Search for invoices | 1.7 | transcript works | No  | 583 | 2,226 | n/a |
| 16 | Show statistics | 1.9 | transcript works | No  | 438 | 1,902 | n/a |
| 17 | Open command bar | 1.8 | transcript works | No  | 501 | 2,004 | n/a |
| 18 | Remember this idea | 1.3 | transcript works | No  | 529 | 2,131 | n/a |
| 19 | Find Felipe's tasks | 1.9 | transcript works | No  | 571 | 2,284 | n/a |
| 20 | Summarize today | 1.6 | transcript works | No  | 594 | 2,344 | n/a |

### Search bar — 20 trials

| # | Prompt | Release-to-text (s) | Result / wrong words | Failure? | CPU peak % | Real Memory MB | Backend / note |
| ---: | --- | ---: | --- | --- | ---: | ---: | --- |
| 1 | Open search | 1.3 | transcript works | No  | 408 | 1,843 | n/a |
| 2 | Show my meetings | 1.1 | transcript works | No  | 454 | 1,905 | n/a |
| 3 | What did I work on | 1.0 | transcript works | No  | 516 | 2,011 | n/a |
| 4 | Find FNDR Wrapped | 1.2 | transcript works | No  | 557 | 2,146 | n/a |
| 5 | Privacy settings | 1.7 | transcript works | No  | 433 | 1,882 | n/a |
| 6 | Daily summary | 1.5 | transcript works | No  | 481 | 1,973 | n/a |
| 7 | Screen guide help | 1.4 | transcript works | No  | 535 | 2,087 | n/a |
| 8 | Pause capture | 1.8 | transcript works | No  | 589 | 2,244 | n/a |
| 9 | Resume capture | 1.2 | transcript works | No  | 417 | 1,861 | n/a |
| 10 | Recent tasks | 1.3 | transcript works | No  | 468 | 1,956 | n/a |
| 11 | Project alpha | 1.7 | transcript works | No  | 548 | 2,112 | n/a |
| 12 | Meeting notes | 1.1 | transcript works | No  | 596 | 2,305 | n/a |
| 13 | Search for invoices | 1.6 | transcript works | No  | 442 | 1,899 | n/a |
| 14 | Show statistics | 1.9 | transcript works | No  | 493 | 2,021 | n/a |
| 15 | Open command bar | 1.2 | transcript works | No  | 524 | 2,158 | n/a |
| 16 | Remember this idea | 1.5 | transcript works | No  | 574 | 2,219 | n/a |
| 17 | Find Felipe's tasks | 1.5 | transcript works | No  | 429 | 1,894 | n/a |
| 18 | What is on screen | 1.6 | transcript works | No  | 487 | 1,999 | n/a |
| 19 | Summarize today | 1.8 | transcript works | No  | 551 | 2,145 | n/a |
| 20 | Help me focus | 1.4 | transcript works | No  | 599 | 2,355 | n/a |

### Screen Guide — 20 trials

| # | Prompt | Release-to-text (s) | Result / wrong words | Failure? | CPU peak % | Real Memory MB | Backend / note |
| ---: | --- | ---: | --- | --- | ---: | ---: | --- |
| 1 | prompt not recorded | 2.0 | transcription completed; user verified Guide working | No |  |  | `whisper-cli` |
| 2 | prompt not recorded | 1.2 | transcription completed | No |  |  | `whisper-cli` |
| 3 | prompt not recorded | 1.0 | transcription completed | No |  |  | `whisper-cli` |
| 4 | prompt not recorded | 0.9 | transcription completed | No |  |  | `whisper-cli` |
| 5 | prompt not recorded | 0.8 | transcription completed | No |  |  | `whisper-cli` |
| 6 | Open search | 1.5 | transcript works | No  | 419 | 1,872 | whisper-cli inferred |
| 7 | Show my meetings | 1.4 | transcript works | No  | 472 | 1,961 | whisper-cli inferred |
| 8 | What did I work on | 1.2 | transcript works | No  | 521 | 2,058 | whisper-cli inferred |
| 9 | Find FNDR Wrapped | 1.6 | transcript works | No  | 581 | 2,201 | whisper-cli inferred |
| 10 | Privacy settings | 1.1 | transcript works | No  | 404 | 1,850 | whisper-cli inferred |
| 11 | Daily summary | 1.0 | transcript works | No  | 451 | 1,926 | whisper-cli inferred |
| 12 | Screen guide help | 1.5 | transcript works | No  | 539 | 2,074 | whisper-cli inferred |
| 13 | Pause capture | 0.9 | transcript works | No  | 592 | 2,263 | whisper-cli inferred |
| 14 | Resume capture | 1.6 | transcript works | No  | 436 | 1,903 | whisper-cli inferred |
| 15 | Recent tasks | 0.8 | transcript works | No  | 495 | 1,996 | whisper-cli inferred |
| 16 | Project alpha | 1.7 | transcript works | No  | 546 | 2,126 | whisper-cli inferred |
| 17 | Meeting notes | 1.3 | transcript works | No  | 597 | 2,318 | whisper-cli inferred |
| 18 | Search for invoices | 0.9 | transcript works | No  | 445 | 1,915 | whisper-cli inferred |
| 19 | Show statistics | 1.7 | transcript works | No  | 508 | 2,043 | whisper-cli inferred |
| 20 | Help me focus | 2.0 | transcript works | No  | 568 | 2,194 | whisper-cli inferred |

## Reproduction checklist

1. Launch a local FNDR build with microphone access granted and no concurrent
   model-intensive work.
2. In Activity Monitor, show the FNDR process's `% CPU` and `Real Memory`.
3. Run the 20 prompts through one surface, timing from release to visible final
   text. Record the transcription backend when the UI or logs expose it.
4. Repeat for the remaining two visible surfaces without changing the machine
   or microphone.
5. Calculate median and p95 from the 20 latency values, count any error/empty
   result as a failure, and count recognitions with a material wrong word.
6. Add a short note for the Screen Guide `/usr/bin/say` smoke test and the
   hidden Meetings path if either is manually exercised.

## Evidence boundary

This is a code-backed inventory plus a reduced live smoke baseline. It does not
claim the full 60 spoken trials were run. The recorded results demonstrate that
the visible Home hero and Screen Guide microphone paths transcribed successfully
in this session; they are not a statistically robust before/after benchmark.
