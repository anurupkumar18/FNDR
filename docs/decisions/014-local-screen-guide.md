# ADR 014: Local-first Screen Guide with ephemeral visual context

## Status

Accepted

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
- Display pixels, OCR, questions, answers, and the in-memory conversation ring
  are not persisted by Screen Guide. The existing local Whisper boundary may
  create a bounded temporary audio file and removes it after transcription.
- Screen pixels are passed only to existing local inference. There is no silent
  cloud fallback and no imported Clicky Worker.
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
  the target application.
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

## Consequences

- Users get the useful Clicky interaction within FNDR's bundle and privacy
  model, without three required cloud accounts or a second app.
- The initial implementation can reuse existing services and add no storage
  schema or production dependency.
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
- Persisting a guide turn or enabling any cloud provider requires a separate
  product/privacy decision.

## Source and attribution

The behavior and selected motion/pointing ideas were informed by Clicky commit
`a80fa80721a8aebe51a170a7780705024ebc6e46`, licensed under MIT. FNDR keeps the
upstream notice in `THIRD_PARTY_NOTICES.md`; no Clicky executable, worker,
branding asset, analytics, or bundled media is shipped.
