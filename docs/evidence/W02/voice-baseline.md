# VO-01 voice baseline

Date: 2026-09-28
Status: inventory complete; limited manual smoke baseline recorded. The original
20-trial-per-surface target was not run because repeated warm-path trials were
consistent in this session. The later-added notch paths are inventoried but were
not exercised; no values are inferred for them.

## Scope and method

This is a baseline of the pre-VO-03 voice implementation. Home, Search, Screen
Guide, and the notch currently own separate voice paths. Most record in the web
view with `MediaRecorder`, then send the recorded bytes to the Rust `speech`
module for local Whisper transcription. Notch Do also has a continuous listener
with browser speech recognition when available and a recorded-audio fallback.
Screen Guide can speak a completed answer through macOS `/usr/bin/say`; the
notch uses browser speech synthesis. Meetings records and transcribes audio but
is not part of the visible normal flow.

For each visible microphone entry point, run 20 short utterances (roughly two
to five seconds) on the same Mac and write one row per attempt in the trial
log below. Start the stopwatch at mic-button release (or shortcut release for
Screen Guide) and stop it when text is rendered. Record an error, empty result,
or materially wrong word as a failure. Capture FNDR CPU and resident memory
while transcription is active with Activity Monitor, or record a runtime-metrics
dump if it includes those values.

The results below are limited to the trials actually observed. A blank resource
cell means that value was not captured, not that it was zero.

## Entry-point inventory

| Entry point | User-visible behavior | Capture and transcription path | Baseline status |
| --- | --- | --- | --- |
| Home hero voice | Tap to begin and tap to stop; the final text fills the hero field for review before Enter. | `src/app/HomeHero.tsx` `useHeroVoice` uses `getUserMedia` and `MediaRecorder`, then calls `transcribeVoiceInput`; it reaches `transcribe_voice_input` and `speech::transcribe_audio_bytes`. | 3-trial smoke baseline recorded |
| Search bar voice and commands | Tap to begin and tap to stop; final text is routed to search/command handling. | `src/domains/search/SearchBar.tsx` independently uses `getUserMedia` and `MediaRecorder`, then calls `transcribeVoiceInput` and its voice-transcript handler. | not reachable in this build/session |
| Screen Guide hold-to-talk | Hold the Screen Guide shortcut to record; release transcribes and submits the text as a question. | `src/domains/screen-guide/ScreenGuideOverlay.tsx` uses `getUserMedia` and `MediaRecorder`, then calls `transcribeScreenGuideVoiceInput`; Rust calls `speech::transcribe_audio_bytes` with cancellation support. | 5-trial smoke baseline recorded |
| Screen Guide spoken answers | When spoken responses are enabled, a completed Screen Guide answer is spoken locally. | `src-tauri/src/ipc/commands/screen_guide.rs` `start_say_process` launches `/usr/bin/say` and sends the answer over stdin. This is output, not a release-to-text path. | smoke test pending; no 20-utterance transcription measure applies |
| Notch Ask voice | Tap once to record and again to stop; the final transcript is submitted immediately as a notch question. | `src/domains/notch/NotchHud.tsx` uses `VoiceCapture` from `notchVoice.ts`, then calls `transcribeVoiceInput` and `ask`. | code inventory only; 0/20 trials recorded |
| Notch Do duplex voice | Opening Do mode starts a continuous listener; speech can route a request, approve or deny a pending action, or stop computer use. FNDR replies through browser speech synthesis. | `src/domains/notch/NotchOperator.tsx` uses `DuplexListener` and `Speaker` from `duplexVoice.ts`; the fallback records a clip and calls `transcribeVoiceInput`. | code inventory only; 0/20 trials recorded; requires safety-boundary QA |
| Meetings (hidden) | Meeting recording captures microphone audio and transcribes post-recording segments. | `src-tauri/src/meeting/mod.rs` records through `ffmpeg`; `transcribe_segment` uses the speech module unless a custom command is configured. | excluded from visible-control trial set; lifecycle checked by code inventory |
| Shared backend | Persists short recorded inputs temporarily, ensures a Whisper model, normalizes audio when possible, then uses whisper-cli or the Python fallback. | `src-tauri/src/ipc/commands/stats.rs` `transcribe_voice_input` calls `speech::transcribe_audio_bytes`; `src-tauri/src/speech.rs` owns conversion and transcription. | shared dependency; resource sample required during each visible-path trial |

## Initial findings from code inspection

- Home, Search, Screen Guide, Notch Ask, and Notch Do duplicate microphone or
  recorder ownership instead of sharing a session.
- Home, Search, Screen Guide, and Notch Ask wait for a final transcription.
  Notch Do can emit partial text through browser speech recognition.
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
| Home hero | 3 / 20 | 6.0 s / not calculated (small sample) | 0 observed | 0 observed | 405–509% | 1,905–2,048 MB | Backend not logged for these trials |
| Search bar | 0 / 20 | not tested; no reachable microphone control in this build | — | — | — | — | Code path exists but was not exposed in this session |
| Screen Guide | 5 / 20 | 0.759 s warm median; 18.636 s first run | 0 observed | not systematically scored | not recorded | not recorded | `whisper-cli`; first run is a cold-start outlier |
| Notch Ask | 0 / 20 | not tested | — | — | — | — | Current code submits the final transcript immediately |
| Notch Do | 0 / 20 | not tested | — | — | — | — | Continuous listener and spoken output; exercise only with approval and Stop boundaries in place |

### Home hero — 20 trials

| # | Prompt | Release-to-text (s) | Result / wrong words | Failure? | CPU peak % | Real Memory MB | Backend / note |
| ---: | --- | ---: | --- | --- | ---: | ---: | --- |
| 1 | Show my meetings | 6.0 | good; all words | No | 405 | 2,048 | backend not logged |
| 2 | Find my daily summary | 6.0 | good | No | 460 | 1,905 | backend not logged |
| 3 | Search for FNDR Wrapped | 6.0 | good | No | 509 | 1,946 | backend not logged |
| 4 |  |  |  |  |  |  |  |
| 5 |  |  |  |  |  |  |  |
| 6 |  |  |  |  |  |  |  |
| 7 |  |  |  |  |  |  |  |
| 8 |  |  |  |  |  |  |  |
| 9 |  |  |  |  |  |  |  |
| 10 |  |  |  |  |  |  |  |
| 11 |  |  |  |  |  |  |  |
| 12 |  |  |  |  |  |  |  |
| 13 |  |  |  |  |  |  |  |
| 14 |  |  |  |  |  |  |  |
| 15 |  |  |  |  |  |  |  |
| 16 |  |  |  |  |  |  |  |
| 17 |  |  |  |  |  |  |  |
| 18 |  |  |  |  |  |  |  |
| 19 |  |  |  |  |  |  |  |
| 20 |  |  |  |  |  |  |  |

### Search bar — 20 trials

| # | Prompt | Release-to-text (s) | Result / wrong words | Failure? | CPU peak % | Real Memory MB | Backend / note |
| ---: | --- | ---: | --- | --- | ---: | ---: | --- |
| 1 |  |  |  |  |  |  |  |
| 2 |  |  |  |  |  |  |  |
| 3 |  |  |  |  |  |  |  |
| 4 |  |  |  |  |  |  |  |
| 5 |  |  |  |  |  |  |  |
| 6 |  |  |  |  |  |  |  |
| 7 |  |  |  |  |  |  |  |
| 8 |  |  |  |  |  |  |  |
| 9 |  |  |  |  |  |  |  |
| 10 |  |  |  |  |  |  |  |
| 11 |  |  |  |  |  |  |  |
| 12 |  |  |  |  |  |  |  |
| 13 |  |  |  |  |  |  |  |
| 14 |  |  |  |  |  |  |  |
| 15 |  |  |  |  |  |  |  |
| 16 |  |  |  |  |  |  |  |
| 17 |  |  |  |  |  |  |  |
| 18 |  |  |  |  |  |  |  |
| 19 |  |  |  |  |  |  |  |
| 20 |  |  |  |  |  |  |  |

### Screen Guide — 20 trials

| # | Prompt | Release-to-text (s) | Result / wrong words | Failure? | CPU peak % | Real Memory MB | Backend / note |
| ---: | --- | ---: | --- | --- | ---: | ---: | --- |
| 1 | prompt not recorded | 18.636 | transcription completed; user verified Guide working | No |  |  | `whisper-cli`; cold-start outlier |
| 2 | prompt not recorded | 0.759 | transcription completed | No |  |  | `whisper-cli` |
| 3 | prompt not recorded | 0.731 | transcription completed | No |  |  | `whisper-cli` |
| 4 | prompt not recorded | 0.782 | transcription completed | No |  |  | `whisper-cli` |
| 5 | prompt not recorded | 0.639 | transcription completed | No |  |  | `whisper-cli` |
| 6 |  |  |  |  |  |  |  |
| 7 |  |  |  |  |  |  |  |
| 8 |  |  |  |  |  |  |  |
| 9 |  |  |  |  |  |  |  |
| 10 |  |  |  |  |  |  |  |
| 11 |  |  |  |  |  |  |  |
| 12 |  |  |  |  |  |  |  |
| 13 |  |  |  |  |  |  |  |
| 14 |  |  |  |  |  |  |  |
| 15 |  |  |  |  |  |  |  |
| 16 |  |  |  |  |  |  |  |
| 17 |  |  |  |  |  |  |  |
| 18 |  |  |  |  |  |  |  |
| 19 |  |  |  |  |  |  |  |
| 20 |  |  |  |  |  |  |  |

## Reproduction checklist

1. Launch a local FNDR build with microphone access granted and no concurrent
   model-intensive work.
2. In Activity Monitor, show the FNDR process's `% CPU` and `Real Memory`.
3. Run the 20 prompts through one surface, timing from release to visible final
   text. Record the transcription backend when the UI or logs expose it.
4. Repeat for each remaining visible surface without changing the machine or
   microphone. Exercise Notch Do only in a safe test environment with approval
   and Stop boundaries active.
5. Calculate median and p95 from the 20 latency values, count any error/empty
   result as a failure, and count recognitions with a material wrong word.
6. Add a short note for the Screen Guide `/usr/bin/say` smoke test and the
   hidden Meetings path if either is manually exercised.

## Evidence boundary

This is a code-backed inventory plus a reduced live smoke baseline. It does not
claim the full requested trial matrix was run. The recorded results demonstrate
only that the Home hero and Screen Guide microphone paths transcribed
successfully in this session; they are not a statistically robust before/after
benchmark. Search and both notch paths remain unmeasured.
