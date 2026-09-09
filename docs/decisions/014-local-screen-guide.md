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
interaction coordinator.

## Decision

Implement the experience as an FNDR-owned **Screen Guide** domain.

- The feature is opt-in and read-only.
- Voice and typed questions share one transient interaction path.
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
- “Companion” remains reserved for FNDR's iPhone/Watch Companion API.

## Consequences

- Users get the useful Clicky interaction within FNDR's bundle and privacy
  model, without three required cloud accounts or a second app.
- The initial implementation can reuse existing services and add no storage
  schema or production dependency.
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
