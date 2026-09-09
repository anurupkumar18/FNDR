# PRD: FNDR Screen Guide

## Problem

FNDR can remember and retrieve desktop context, but helping with the screen in
front of the user still requires opening the app, typing a query, and mentally
mapping the answer back to the source application. Clicky demonstrates a useful
interaction loop—push to talk, inspect the current screen, answer aloud, and
point at the relevant place—but its public implementation is a separate
cloud-first Swift application rather than a library FNDR can safely embed.

## Goal

Make screen-aware guidance feel like a native FNDR capability: one FNDR process,
one permission identity, one settings surface, and one local privacy model.
The user can invoke a transient guide over another app, ask a question by voice
or text, receive a concise answer, and get an optional visual point cue.

## Users / actors

- A Mac user who wants help understanding or navigating the visible interface.
- FNDR's Screen Guide coordinator, running locally and read-only.
- Existing FNDR capture, OCR, inference, speech, and privacy services.

## Current behavior

- Voice input exists in FNDR's Home search surface.
- Screen capture and OCR feed durable memory creation after privacy checks.
- Hidden always-on-top windows exist for Omnibar and Autofill.
- No single workflow joins those capabilities into a live screen interaction.

## Proposed behavior

1. The user explicitly enables Screen Guide in its FNDR feature panel.
2. Holding the configured shortcut, or holding the panel microphone button,
   opens a click-through response overlay and records a short utterance.
3. On release, FNDR transcribes locally, hides its overlay, applies the normal
   privacy policy, and obtains a fresh in-memory screen capture.
4. Local OCR and available local inference answer the question. A validated
   normalized point cue may identify a relevant visible text label; icon-only
   targets are answered without a cue.
5. The overlay shows the answer, optionally speaks it with a local macOS voice,
   and then dismisses. FNDR windows excluded for the turn remain hidden until
   that visual answer ends, keeping the guided app visible and focused. A new
   interaction supersedes the previous one.

## Non-goals

- Bundling or launching Clicky's Swift application or Cloudflare Worker.
- Sending screen pixels, microphone audio, transcripts, or answers to a cloud
  provider by default.
- Clicking, typing, purchasing, submitting forms, or executing agent actions.
- Retaining raw pixels, temporary audio, transcripts, answers, or conversation turns.
- Importing Clicky branding, analytics, onboarding media, or design system.

## Functional requirements

- FR1: Screen Guide is a first-class sidebar and command-palette destination.
- FR2: It is disabled by default and records only during an explicit press.
- FR3: Settings persist through FNDR's existing configuration file.
- FR4: The shortcut coexists with Autofill and Omnibar shortcuts.
- FR5: The overlay is transparent, non-focusable, click-through, and visible
  across macOS workspaces without becoming durable captured context.
- FR6: Incognito, FNDR's own windows, and blocklisted contexts are rejected
  before a guide capture.
- FR7: Screen pixels remain in memory and are dropped after the turn.
- FR8: Voice transcription reuses FNDR's local speech path.
- FR9: Answers use local inference when available and degrade to a bounded,
  grounded screen summary when it is not.
- FR10: Model point syntax is parsed and removed in Rust; React receives only a
  typed point cue that exactly matches and snaps to an Apple Vision text-line
  center supplied as model evidence.
- FR11: Speech output is optional, local, interruptible, and never required for
  the text answer to succeed.
- FR12: State changes are pushed over Tauri events rather than polled.

## Non-functional requirements

- Performance: listening feedback should appear immediately; model work shares
  `model_pipeline_lock` so it cannot race FNDR capture inference.
- Reliability: rapid press/release and a new turn must not leave microphone
  tracks, speech, hidden FNDR windows, or stale overlay state running. Native
  stop-acknowledgement and maximum-duration watchdogs destroy/recreate a hung
  overlay instead of trusting its JavaScript timers to release the microphone.
- Security/privacy: live OCR is untrusted evidence, the guide is read-only, and
  no guide input is written to LanceDB or telemetry. Local Whisper may use its
  existing bounded temporary audio file, which is removed after transcription.
- Accessibility: every spoken response is also text; the overlay uses a live
  region and honors reduced motion.
- Maintainability: reuse existing capture, OCR, speech, config, window, and
  event primitives rather than adding parallel services.

## Domain language

| Term | Meaning | Existing code/docs |
| --- | --- | --- |
| Screen Guide | FNDR's live, local, read-only screen assistance feature | New domain |
| Guide interaction | One explicit voice or text question and its transient answer | New domain |
| Ephemeral display capture | Pixels held only long enough to answer a guide interaction | ADR 004 |
| Point cue | Validated visual-only target using normalized display coordinates | New domain |
| Companion API | FNDR's separate iPhone/Watch local-network API | `src-tauri/src/companion/` |

## Affected modules and interfaces

| Module | Change | Interface impact | Tests |
| --- | --- | --- | --- |
| `config` | Add normalized Screen Guide settings | Additive TOML field | Defaults and normalization |
| `capture` / `privacy` | Add transient guide capture path | No storage schema change | Pre-capture refusal |
| `inference` | Bounded live-screen answer and point output | Internal reuse | Point parsing/clamping |
| `ipc/commands` | Thin settings, lifecycle, answer, speech commands | Additive Tauri API | Serialization contracts |
| `src/domains/screen-guide` | Panel and overlay | Additive panel key/window | UI states and settings |
| shortcut/window setup | Register guide and overlay | Preserve existing shortcuts | Conflict/regression tests |

## Data flow

```text
explicit press -> local microphone -> local transcription
               -> pre-capture privacy gate -> ephemeral screen pixels -> OCR/local inference
               -> typed text + optional point cue -> click-through overlay + local speech
               -> discard pixels/audio/turn state
```

## Acceptance criteria

- [ ] Enabling Screen Guide never disables Omnibar or Autofill shortcuts.
- [ ] Press/release produces listening, transcribing, thinking, and answer states.
- [ ] The target application keeps focus and the overlay never intercepts clicks.
- [ ] A denied private context performs no OCR or inference.
- [ ] No guide screenshot path or guide record appears in persistent storage.
- [ ] Malformed or out-of-range point output cannot escape as a UI action.
- [ ] Muting or interrupting speech leaves the text response intact.
- [ ] Missing permissions/models produce an actionable local error or fallback.
- [ ] Focused frontend and Rust tests plus the repository's relevant full checks pass.

## Rollout / migration plan

The configuration field is additive and defaults to disabled. There is no data
migration. The first implementation targets the primary display; multi-display
capture/overlay parity is a follow-up hardening slice because it requires a
tested physical/logical coordinate map for mixed-scale and negative-origin
layouts.

## Risks

- Hidden WebView microphone behavior must be verified in a packaged macOS build.
- Local Whisper plus local vision/text inference can have high cold-start latency.
- Screen content can contain prompt injection; the guide must remain read-only.
- A full-screen overlay can contaminate capture unless hidden or excluded first.
- OCR bounds locate text lines rather than precise accessibility hitboxes, and
  semantic target selection still varies with the local model.

## Suggested issues

1. Native overlay, settings, shortcut lifecycle, and fake state flow.
2. Local microphone/transcription with cancellation and cleanup.
3. Privacy-gated ephemeral capture and bounded local answer.
4. Structured point cues, motion, and local speech.
5. Multi-display capture/coordinate hardening and packaged-app verification.
