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
- Its click-through overlay uses native full-screen auxiliary collection
  behavior, and capture now verifies the exposed target, retries one apparently
  blank frame, and uses a dedicated Retina-readable OCR profile.
- Voice capture originates in a WebView, while transcription runs through
  FNDR's local speech boundary; packaged audio handling has not been reliable
  enough for shortcut-first use.
- FNDR's memory retrieval does not locate a current file by name on demand.
- Screen Guide has a transient overlay and a bounded fixed-state status item in
  the menu-bar/notch region while the app is running.
- Normal turns remain ephemeral. A person can explicitly arm one display turn
  to save a short-lived local diagnostic bundle for troubleshooting.

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
   capture, rejects or retries an apparently blank frame, and reads it with the
   Screen Guide-specific Apple Vision profile (`minimum_text_height = 0.015`).
   A validated normalized point cue may identify a visible text label;
   icon-only targets are answered without a cue. Answer generation uses the
   configured provider; local remains the default, while optional ChatGPT image
   egress still requires its separate screenshot consent.
6. The overlay shows the answer and, when speech output is enabled, speaks it
   with a local macOS voice before dismissing. FNDR windows excluded for a
   screen turn remain hidden until that visual answer ends, keeping the guided
   app visible and focused. A new interaction supersedes the previous one.
7. **Save next turn** explicitly arms one display-reading turn for five minutes.
   After OCR and its safety gate allow the turn, FNDR saves the exact OCR input
   image, OCR output, and a privacy-reduced timing/context manifest only in
   private FNDR app data. The arm is consumed once, expires if unused, and is
   revoked by Private Mode. The panel shows a typed save/error receipt, storage
   counts, delete, and a backend-owned reveal action. If OCR fails or privacy
   blocks the turn, raw artifacts are not written.

## Non-goals

- Bundling or launching Clicky's Swift application or Cloudflare Worker.
- Sending screen pixels, microphone audio, transcripts, or answers to a cloud
  provider by default.
- Clicking, typing, purchasing, submitting forms, or executing agent actions.
- General full-Mac, full-home, file-content, Library, or arbitrary-volume
  search.
- Reading, previewing, opening, revealing, moving, or modifying a located file.
- Retaining raw pixels, temporary audio, transcripts, answers, or conversation
  turns from normal use. The explicit, bounded one-turn diagnostic bundle is
  not memory history.
- Retaining filename queries or file-match lists as Screen Guide history.
- Automatically exporting, uploading, indexing, embedding, or adding a Screen
  Guide diagnostic to model context.
- Replacing or drawing over the physical notch with a private macOS API, or
  showing questions, filenames, or answers in the status item.
- Importing Clicky branding, analytics, onboarding media, or design system.

## Functional requirements

- FR1: Screen Guide is a first-class sidebar and command-palette destination.
- FR2: It is disabled by default and records only during an explicit press.
- FR3: Settings persist through FNDR's existing configuration file.
- FR4: The shortcut coexists with Autofill and Omnibar shortcuts.
- FR5: The overlay is transparent, non-focusable, click-through, and visible
  across macOS workspaces, including another app's native full-screen Space via
  `FullScreenAuxiliary`, without becoming durable captured context.
- FR6: Incognito, FNDR's own windows, and blocklisted contexts are rejected
  before a guide capture.
- FR7: During a normal turn, screen pixels remain in memory and are dropped
  after the turn. Two things may write them: FR19's explicitly armed
  diagnostic, and the ChatGPT answer path with the separate screenshot
  consent on, which stages one downscaled image in a per-turn folder that
  only this account can read (`0700`, file `0600`). That folder is removed
  when the turn ends and any leftover is removed at the next start.
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
- FR19: **Save next turn** arms for five minutes and is consumed by the next
  display-reading turn, not by filename lookup. Private Mode refuses arming and
  revokes a pending arm or active diagnostic.
- FR20: Diagnostics remain in FNDR app data only, with `0700` directories and
  `0600` files. Startup, periodic five-minute, and lazy cleanup remove abandoned
  partials and completed bundles older than 24 hours. Cleanup keeps at most two
  completed bundles, while active partials and completed bundles share a 64 MiB
  cap. A panel control deletes both and cancels an arm.
- FR21: Raw diagnostic pixels remain in memory until OCR completes and the
  post-OCR safety gate allows the turn. Only then may FNDR write the exact
  screenshot, OCR text and positioned lines. OCR failure retains aggregate
  evidence only. A privacy-blocked manifest has no raw artifacts, app name, or
  bundle identifier.
- FR22: An apparently blank captured frame is retried once after a short
  compositor settle and then fails with an actionable error rather than
  continuing to OCR or inference.
- FR23: Screen Guide uses `minimum_text_height = 0.015`; the durable memory
  pipeline keeps its separate `0.02` OCR default.
- FR24: The current capture backend reads the primary display only. UI and
  errors must not imply that a window on another display was captured.
- FR25: Diagnostic files never enter Memory, LanceDB, retrieval, embeddings,
  model context, telemetry, cloud requests, or automatic export. Diagnostic
  consent does not alter the separately configured answer-provider egress path.
- FR26: The panel reports completed and active-partial counts, total bytes, and
  a typed last-result receipt including whether screenshot and OCR were saved.
  Over-budget or failed writes publish no incomplete bundle.
- FR27: Reveal is an explicit backend-owned action for FNDR's fixed diagnostics
  directory; the renderer supplies no path.

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
  speech layer. Diagnostic persistence requires the separate explicit arm,
  follows FR20 and FR21, and never changes provider or egress settings.
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
| One-turn diagnostic bundle | Explicitly armed, short-lived local screenshot, OCR, and timing evidence for one display-reading turn | ADR 004 / ADR 014 |
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
| Screen Guide diagnostics | Arm, consume, securely store, retain, and delete one-turn evidence | Additive local app-data exception; no schema | Consent, permissions, redaction, retention, deletion |
| `src/domains/screen-guide` | Panel and overlay | Additive panel key/window | UI states and settings |
| shortcut/window/status setup | Register guide, overlay, and status item | Preserve existing shortcuts; fixed phase event | Conflict, stale-state, and privacy tests |

## Data flow

```text
explicit press -> local microphone -> on-device transcription -> classify request
  explicit filename request -> incognito gate -> bounded macOS metadata query
                            -> filename + relative folder
  visible-screen question  -> pre-capture privacy gate -> ephemeral screen pixels
                            -> blank check/retry -> Screen Guide OCR
                            -> configured answer inference -> typed text + optional point cue
both routes -> click-through text overlay + optional local speech
            -> discard pixels/audio/transcript/answer/lookup state

explicit diagnostic arm -> next display-reading turn only -> OCR + safety gate
                        -> allowed + within budget: private raw/manifest bundle
                        -> blocked/failure: aggregate-only or typed error receipt
                        -> startup/periodic/lazy expiry or explicit delete
                        -> never Memory/index/model context/cloud/automatic export

phase changes -> fixed-state OS status item (never user content)
```

## Acceptance criteria

- [ ] Enabling Screen Guide never disables Omnibar or Autofill shortcuts.
- [ ] Press/release produces listening, transcribing, thinking, and answer states.
- [ ] A packaged build transcribes supported recorded audio entirely on-device.
- [ ] Holding the shortcut records only while held; release transcribes and can
      return a local spoken answer without opening the main FNDR window.
- [ ] The target application keeps focus and the overlay never intercepts clicks.
- [ ] The overlay can appear in another app's native full-screen Space without
      activating FNDR or entering the captured frame.
- [ ] A denied private context performs no OCR or inference.
- [ ] Normal turns create no guide screenshot path or guide record. An
      explicitly armed diagnostic creates only the bounded app-data bundle.
- [ ] Diagnostic arming expires after five minutes, is consumed once, refuses
      Private Mode, is revoked by a later Private Mode transition, and can be
      deleted from the Screen Guide panel.
- [ ] Diagnostic directories/files use `0700`/`0600`; startup, periodic, and
      lazy cleanup enforce two completed bundles, 24-hour expiry, and a shared
      64 MiB cap across completed and active-partial data.
- [ ] A post-OCR privacy rejection leaves no diagnostic screenshot, OCR, app
      name, or bundle identifier, and an OCR failure retains no raw capture.
- [ ] The panel truthfully reports partials, bytes, and screenshot/OCR save
      status; an over-budget write leaves no published bundle.
- [ ] Diagnostic files have no Memory, LanceDB, retrieval, embedding, model
      context, telemetry, cloud, or automatic-export path. Reveal opens only the
      backend-owned fixed directory after an explicit user action.
- [ ] A blank capture is retried once and then reports an actionable error.
- [ ] Retina browser/editor fixtures demonstrate the dedicated `0.015` OCR
      profile without changing the durable capture pipeline's `0.02` default.
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
is migrated. Diagnostics also default to off and are one-shot rather than a
saved preference. Startup cleanup removes abandoned partials; periodic
five-minute and lazy cleanup enforce the completed-bundle age/count/byte policy.
Existing installs move the FNDR-owned speech runtime from its legacy Documents
location into private app data on first fallback use. The first implementation
still captures the primary display only; `FullScreenAuxiliary` fixes overlay
participation in native full-screen Spaces but does not select a secondary monitor.
Multi-display capture/overlay parity remains a follow-up hardening slice because
it requires a tested physical/logical coordinate map for mixed-scale and
negative-origin layouts and a monitor-aware ScreenCaptureKit backend.

## Risks

- Hidden WebView microphone behavior must be verified in a packaged macOS build.
- Local Whisper plus local vision/text inference can have high cold-start latency.
- macOS metadata can be stale or incomplete, and folder permission denial can
  legitimately produce no matches.
- Screen content can contain prompt injection; the guide must remain read-only.
- A full-screen overlay can contaminate capture unless hidden or excluded first.
- AppKit and Accessibility can expose a new full-screen target at different
  times; bounded convergence and blank-frame retry reduce, but cannot eliminate,
  compositor/Space transition failures.
- OCR bounds locate text lines rather than precise accessibility hitboxes, and
  semantic target selection still varies with the local model.
- An explicitly saved diagnostic can contain everything visible on the screen;
  the consent warning, private permissions, strict retention, safety redaction,
  truthful receipts, and delete control are mandatory parts of the feature.
- macOS controls exact status-item placement; FNDR can live beside the notch but
  must not assume a particular notch geometry or use private placement APIs.

## Suggested issues

1. Repair packaged local microphone/transcription normalization and cleanup.
2. Add explicit, privacy-bounded filename routing and macOS metadata lookup.
3. Add the fixed-state OS status item and stale-state lifecycle protection.
4. Verify shortcut voice, local speech, file lookup, full-screen handoff,
   one-turn diagnostics, and permission behavior in a packaged macOS build.
5. Replace primary-display capture with a monitor-aware ScreenCaptureKit path
   and harden mixed-scale/negative-origin coordinates in a later slice.
