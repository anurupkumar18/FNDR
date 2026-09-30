# ADR 014: Local-first Screen Guide with ephemeral visual context

## Status

Accepted; amended 2026-09-28 for bounded diagnostics and native capture
stabilization.

## Context

The requested Clicky integration crosses FNDR's capture, speech, inference,
privacy, window, and shortcut boundaries. The public Clicky source is a
standalone Swift/AppKit application whose normal path sends microphone audio to
AssemblyAI, display images and conversation history to Anthropic, and answers to
ElevenLabs. Vendoring that app would create a second lifecycle and permission
identity, duplicate services FNDR already owns, and violate FNDR's local-first
default.

FNDR already has local screen capture and OCR, local Whisper transcription,
local text/vision inference, optional local TTS, global shortcuts, hidden Tauri
windows, and event-driven UI status. The missing capability is a bounded live
interaction coordinator. The next vertical slice must also make packaged speech
input dependable, answer explicit file-location questions without reading the
documents, and give the background app a subtle presence near the Mac's notch.

## Decision

Implement the experience as an FNDR-owned **Screen Guide** domain.

- The feature is opt-in and read-only.
- Holding the configured shortcut or panel microphone records only for the
  duration of the press. Voice and typed questions share one transient
  interaction coordinator.
- Recorded WebView audio is normalized for the bundled local Whisper runtime.
  Transcription remains on-device with no cloud fallback; optional answers use
  the existing local macOS speech boundary.
- The coordinator routes only explicit requests to find or locate a named file
  into a separate filename lookup branch. Ambiguous "find" language continues
  through visible-screen guidance rather than widening filesystem access.
- Filename lookup invokes macOS metadata search directly, without a shell, and
  with fixed Documents, Desktop, and Downloads roots plus time, output,
  candidate, and result bounds. Canonicalized matches must remain within one of
  those roots. The branch never searches the full home directory, Library, or
  an arbitrary additional root.
- File lookup reads metadata needed to return a filename and relative parent
  folder only. It does not read document contents, open or reveal a result,
  request Full Disk Access, or expose an absolute home path to UI or speech.
  Incognito prevents the metadata process from starting.
- Privacy policy runs before screen pixels are obtained. The guide refuses
  incognito, internal FNDR, and blocklisted contexts.
- Normal Screen Guide turns do not persist display pixels, OCR, questions,
  answers, or the in-memory conversation ring. The existing local Whisper
  boundary may create a bounded temporary audio file and removes it after
  transcription. The explicit one-turn diagnostic exception is defined below.
- Local inference remains the default. The existing optional ChatGPT answer
  path runs only when the person selects it, and pixels are included only with
  the separate screenshot consent. There is no silent cloud fallback and no
  imported Clicky Worker. Arming diagnostics never enables or changes egress,
  and files from a diagnostic bundle are never uploaded.
- Heavy inference is serialized through `model_pipeline_lock`.
- The response boundary is typed. Apple Vision text-line centers are supplied
  as normalized location evidence. Model `[POINT:...]` text is accepted only
  when it exactly selects one of those locations, then snapped back to the
  observation and stripped from spoken text. A point cue cannot click or act.
- The response overlay is a pre-created transparent, non-focusable,
  click-through Tauri window. It is hidden before guide capture and never
  becomes a durable memory source. Other visible FNDR windows stay excluded
  until the visual turn ends, so they cannot cover the app being described;
  previously visible FNDR surfaces are then restored without taking focus from
  the target application. Its native collection behavior includes
  `FullScreenAuxiliary`, so it can accompany another app in a native
  full-screen Space without becoming the active application.
- After FNDR hides, bounded context probes wait for AppKit and Accessibility to
  agree on the exposed target instead of trusting one fixed delay. A captured
  frame is checked for dimensions, alpha, and luminance variation; an apparently
  blank frame is retried once after a short compositor settle, then rejected
  with an actionable error rather than sent to OCR or inference.
- Screen Guide uses a dedicated Apple Vision OCR profile with
  `minimum_text_height = 0.015` so ordinary Retina browser and editor text is
  eligible. The durable memory-capture default remains `0.02`; Screen Guide
  stabilization does not silently change stored-memory OCR behavior.
- Display capture still targets the primary display. Full-screen Space support
  does not imply multi-display support: a target on another display must be
  moved to the primary display until the monitor-aware ScreenCaptureKit
  follow-up is implemented and native-tested.
- Native microphone watchdogs begin before the renderer requests audio, require
  a stop acknowledgement after every returned MediaStream is closed, and
  destroy/recreate the WebView after a missed acknowledgement or hard privacy
  transition. A hung renderer therefore cannot extend microphone capture.
- Settings live in FNDR's existing config. Screen Guide shortcut registration
  must restore/preserve Omnibar and Autofill registrations rather than relying
  on independent handlers after `unregister_all()`.
- UI state is emitted at state changes, following ADR 011; no status poller is
  introduced.
- FNDR owns one OS-managed status item in the menu-bar/notch region. It maps the
  guide lifecycle to a small fixed vocabulary (off/ready, listening,
  transcribing, finding, found, or attention needed). It never renders the
  question, transcript, screen text, filename, path, answer, or error detail.
  macOS controls its exact placement; FNDR does not draw over the physical notch
  or use a private placement API.
- “Companion” remains reserved for FNDR's iPhone/Watch Companion API.

### Explicit one-turn diagnostics

- **Save next turn** arms one display-reading turn for five minutes and is
  consumed when that turn starts. Filename-only lookup does not consume it.
- Raw pixels remain in memory until OCR completes and the post-OCR safety gate
  allows the turn. Only then may the bundle contain the exact screenshot
  supplied to OCR, exact OCR text and positioned lines, plus a bounded manifest
  of stage timings, display geometry, aggregate image statistics, outcome, and
  privacy-reduced context probes. OCR failures retain aggregate evidence only.
- Bundles live only under private FNDR app data, with `0700` directories and
  `0600` files. Startup, periodic five-minute, and lazy cleanup remove abandoned
  partials and completed bundles older than 24 hours. At most two completed
  bundles are retained; active partials and completed bundles share a 64 MiB
  cap. The panel reports both kinds, their total bytes, and the typed result of
  the last save, and can delete them while also cancelling an unused arm.
- Diagnostic files never enter Memory, LanceDB, retrieval, embeddings, model
  context, telemetry, cloud requests, or automatic export. The existing answer
  provider path remains separate and is not changed by diagnostic consent.
- Private Mode refuses arming and revokes a pending arm or active diagnostic.
  A privacy-blocked manifest contains no raw screenshot, OCR, app name, or
  bundle identifier; only aggregate stage/context evidence may remain.
- An over-budget or failed write publishes no incomplete bundle and returns a
  typed receipt saying whether the screenshot and OCR were saved. An explicit
  backend-owned reveal action opens only the fixed local diagnostics directory;
  the renderer cannot supply an arbitrary path.

## Consequences

- Users get the useful Clicky interaction within FNDR's bundle and privacy
  model, without three required cloud accounts or a second app.
- The initial implementation reuses existing services and adds no storage
  schema. Explicit diagnostics add only bounded local app-data files under the
  retention policy above.
- Explicit file questions can be answered without capturing the screen or
  granting Full Disk Access, but match completeness depends on the macOS
  metadata index and the user's Files and Folders permissions.
- A file answer is informational only. Opening, previewing, revealing, reading,
  or modifying the document remains outside Screen Guide's authority.
- The status item gives immediate micro-feedback without creating another
  capture-visible always-on window. It cannot guarantee a precise position
  relative to every Mac notch or menu-bar layout.
- Local model availability affects answers and semantic target selection; a
  grounded text fallback remains available. Icon-only targets have no point cue.
- Exact public-Clicky parity for modifier-only shortcuts and multi-monitor
  pointing requires platform-specific hardening beyond the primary-display
  vertical slice.
- Normal guide turns remain ephemeral. Any persistence beyond the explicit
  one-turn diagnostic bundle requires a separate product/privacy decision.

## Source and attribution

The behavior and selected motion/pointing ideas were informed by Clicky commit
`a80fa80721a8aebe51a170a7780705024ebc6e46`, licensed under MIT. FNDR keeps the
upstream notice in `THIRD_PARTY_NOTICES.md`; no Clicky executable, worker,
branding asset, analytics, or bundled media is shipped.
