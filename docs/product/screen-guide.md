# PRD: FNDR Screen Guide

## Problem

FNDR can remember and retrieve desktop context, but helping with the screen or
finding a document still requires opening the app, typing a query, and mentally
mapping the answer back to another application. Screen Guide already joins
push-to-talk, transient screen context, and spoken answers, but its packaged
on-device transcription path must be dependable before voice can be the primary
interaction. Users also need a small, always-available FNDR presence without a
second application or a persistent window over their work.

## Goal

Make screen-aware guidance and explicit filename lookup feel like one native
FNDR capability: one FNDR process, one permission identity, one settings
surface, and one local privacy model. The user can hold a shortcut, ask a
question by voice, receive a concise local spoken and textual answer, and either
get an optional visual point cue or learn where a matching local file lives.

## Users / actors

- A Mac user who wants help understanding or navigating the visible interface.
- A Mac user who remembers a document name but not its folder.
- FNDR's Screen Guide coordinator, running locally and read-only.
- Existing FNDR capture, OCR, inference, speech, privacy, and macOS metadata
  services.

## Current behavior

- Screen Guide can already answer a typed or recorded question about an
  ephemeral main-display capture.
- Voice capture originates in a WebView, while transcription runs through
  FNDR's local speech boundary; packaged audio handling has not been reliable
  enough for shortcut-first use.
- FNDR's memory retrieval does not locate a current file by name on demand.
- Screen Guide has a transient overlay but no bounded, OS-managed presence in
  the menu-bar/notch region while the app is running.

## Proposed behavior

1. An OS-managed status item keeps FNDR available in the menu bar beside the
   notch when one is present. It reports only a fixed phase such as off, ready,
   listening, processing, success, or attention needed.
2. The user explicitly enables Screen Guide in its FNDR feature panel. Holding
   the configured shortcut, or holding the panel microphone button,
   opens a click-through response overlay and records only for the duration of
   the press.
3. On release, FNDR normalizes the captured audio and transcribes it entirely
   on-device. No microphone audio or transcript is sent to a network service.
4. An explicit request to find a named file takes a local filename-only branch.
   FNDR queries macOS metadata only inside the user's Documents, Desktop, and
   Downloads folders, then returns a filename and relative folder. It does not
   read file contents, open or reveal the result, or scan the rest of the home
   directory.
5. Other questions follow the existing Screen Guide path: FNDR hides its
   overlay, applies the normal privacy policy, obtains a fresh in-memory screen
   capture, and uses local OCR and inference. A validated normalized point cue
   may identify a visible text label; icon-only targets are answered without a
   cue.
6. The overlay shows the answer and, when speech output is enabled, speaks it
   with a local macOS voice before dismissing. FNDR windows excluded for a
   screen turn remain hidden until that visual answer ends, keeping the guided
   app visible and focused. A new interaction supersedes the previous one.

## Non-goals

- Bundling or launching Clicky's Swift application or Cloudflare Worker.
- Sending screen pixels, microphone audio, transcripts, or answers to a cloud
  provider by default.
- Clicking, typing, purchasing, submitting forms, or executing agent actions.
- General full-Mac, full-home, file-content, Library, or arbitrary-volume
  search.
- Reading, previewing, opening, revealing, moving, or modifying a located file.
- Retaining raw pixels, temporary audio, transcripts, answers, or conversation turns.
- Retaining filename queries or file-match lists as Screen Guide history.
- Replacing or drawing over the physical notch with a private macOS API, or
  showing questions, filenames, or answers in the status item.
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
- FR13: Recorded WebView audio is normalized into the format required by the
  bundled local transcriber, and transcription has no cloud fallback.
- FR14: Only an explicit request to find or locate a named file enters the file
  lookup route; ambiguous requests remain screen questions.
- FR15: File lookup uses macOS metadata with fixed Documents, Desktop, and
  Downloads roots, bounded execution/results, and no shell interpretation.
- FR16: File results are validated inside an allowed root and presented as a
  filename plus relative folder, never as the user's absolute home path.
- FR17: File lookup reads no file contents, opens no match, writes no history,
  and does not require Screen Recording permission. Incognito prevents lookup.
- FR18: While FNDR is running, an OS-managed status item beside the notch/menu
  bar displays only bounded fixed states; it never displays user input, file
  metadata, screen text, or answer text.

## Non-functional requirements

- Performance: listening feedback should appear immediately; metadata lookup
  has strict time, output, candidate, and displayed-result bounds; model work
  shares `model_pipeline_lock` so it cannot race FNDR capture inference.
- Reliability: rapid press/release and a new turn must not leave microphone
  tracks, speech, hidden FNDR windows, or stale overlay state running. Native
  stop-acknowledgement and maximum-duration watchdogs destroy/recreate a hung
  overlay instead of trusting its JavaScript timers to release the microphone.
- Security/privacy: live OCR and metadata results are untrusted evidence, the
  guide is read-only, and no guide input or file match is written to LanceDB or
  telemetry. Local Whisper may use its existing bounded temporary audio file,
  which is removed after transcription. Filename lookup is restricted to the
  three disclosed folders and never returns an absolute home path to the UI or
  speech layer.
- Accessibility: every spoken response is also text; the overlay uses a live
  region, the status item has a meaningful tooltip, and motion honors Reduce
  Motion.
- Maintainability: reuse existing capture, OCR, speech, config, window, and
  event primitives rather than adding parallel services.

## Domain language

| Term | Meaning | Existing code/docs |
| --- | --- | --- |
| Screen Guide | FNDR's live, local, read-only screen assistance feature | New domain |
| Guide interaction | One explicit voice or text question and its transient answer | New domain |
| Ephemeral display capture | Pixels held only long enough to answer a guide interaction | ADR 004 |
| Point cue | Validated visual-only target using normalized display coordinates | New domain |
| Scoped file lookup | An explicit filename-only macOS metadata query limited to Documents, Desktop, and Downloads | ADR 014 |
| Notch status item | The OS-managed, fixed-state FNDR status item placed by macOS in the menu-bar/notch region | ADR 014 |
| Companion API | FNDR's separate iPhone/Watch local-network API | `src-tauri/src/companion/` |

## Affected modules and interfaces

| Module | Change | Interface impact | Tests |
| --- | --- | --- | --- |
| `config` | Add normalized Screen Guide settings | Additive TOML field | Defaults and normalization |
| `capture` / `privacy` | Add transient guide capture path | No storage schema change | Pre-capture refusal |
| `inference` | Bounded live-screen answer and point output | Internal reuse | Point parsing/clamping |
| `speech` | Normalize recorded audio for bundled local transcription and speak local answers | Existing local boundary | Format, cleanup, and fallback cases |
| Screen Guide command coordinator | Route explicit file requests to bounded macOS metadata lookup | Internal, additive branch | Classification, validation, privacy, formatting |
| `ipc/commands` | Thin settings, lifecycle, answer, speech commands | Additive Tauri API | Serialization contracts |
| `src/domains/screen-guide` | Panel and overlay | Additive panel key/window | UI states and settings |
| shortcut/window/status setup | Register guide, overlay, and status item | Preserve existing shortcuts; fixed phase event | Conflict, stale-state, and privacy tests |

## Data flow

```text
explicit press -> local microphone -> on-device transcription -> classify request
  explicit filename request -> incognito gate -> bounded macOS metadata query
                            -> filename + relative folder
  visible-screen question  -> pre-capture privacy gate -> ephemeral screen pixels
                            -> OCR/local inference -> typed text + optional point cue
both routes -> click-through text overlay + optional local speech
            -> discard pixels/audio/transcript/answer/lookup state

phase changes -> fixed-state OS status item (never user content)
```

## Acceptance criteria

- [ ] Enabling Screen Guide never disables Omnibar or Autofill shortcuts.
- [ ] Press/release produces listening, transcribing, thinking, and answer states.
- [ ] A packaged build transcribes supported recorded audio entirely on-device.
- [ ] Holding the shortcut records only while held; release transcribes and can
      return a local spoken answer without opening the main FNDR window.
- [ ] The target application keeps focus and the overlay never intercepts clicks.
- [ ] A denied private context performs no OCR or inference.
- [ ] No guide screenshot path or guide record appears in persistent storage.
- [ ] Malformed or out-of-range point output cannot escape as a UI action.
- [ ] Muting or interrupting speech leaves the text response intact.
- [ ] Missing permissions/models produce an actionable local error or fallback.
- [ ] "Find my I-20 document" searches filenames only in Documents, Desktop,
      and Downloads and returns at most a bounded set of names and relative
      folders without opening any result.
- [ ] Ambiguous screen wording does not trigger file lookup, and Incognito
      starts no metadata query.
- [ ] A lookup never reads file contents, scans the full home directory, or
      exposes the absolute home path in UI or speech.
- [ ] The OS-managed FNDR status item remains available beside the notch/menu
      bar and exposes only fixed, non-personal phase labels.
- [ ] Focused frontend and Rust tests plus the repository's relevant full checks pass.

## Rollout / migration plan

The configuration field is additive and defaults to disabled; no user content
is migrated. Existing installs move the FNDR-owned speech runtime from its
legacy Documents location into private app data on first fallback use. The
first implementation targets the primary display; multi-display
capture/overlay parity is a follow-up hardening slice because it requires a
tested physical/logical coordinate map for mixed-scale and negative-origin
layouts.

## Risks

- Hidden WebView microphone behavior must be verified in a packaged macOS build.
- Local Whisper plus local vision/text inference can have high cold-start latency.
- macOS metadata can be stale or incomplete, and folder permission denial can
  legitimately produce no matches.
- Screen content can contain prompt injection; the guide must remain read-only.
- A full-screen overlay can contaminate capture unless hidden or excluded first.
- OCR bounds locate text lines rather than precise accessibility hitboxes, and
  semantic target selection still varies with the local model.
- macOS controls exact status-item placement; FNDR can live beside the notch but
  must not assume a particular notch geometry or use private placement APIs.

## Suggested issues

1. Repair packaged local microphone/transcription normalization and cleanup.
2. Add explicit, privacy-bounded filename routing and macOS metadata lookup.
3. Add the fixed-state OS status item and stale-state lifecycle protection.
4. Verify shortcut voice, local speech, file lookup, and permission behavior in
   a packaged macOS build.
5. Harden multi-display capture/coordinate behavior in a later slice.
