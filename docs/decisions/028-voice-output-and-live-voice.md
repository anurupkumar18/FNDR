# ADR 028: Natural voice output from the ChatGPT plan, and live voice later

## Status

Accepted 2026-10-09 under the delegation rule in `AGENTS.md` (the owner asked for FNDR to talk back in a natural voice drawn from the connected ChatGPT or Codex plan, and delegated the privacy decision). Built: `src-tauri/src/speech_out/`, `src/shared/voice/realtimeOut.ts`.

## Context

The system voices FNDR speaks with today sound robotic on this Mac, which has only compact voices installed. A person who signed in with ChatGPT for Screen Guide or Notch Do already has a plan whose Codex app-server exposes a realtime voice. OpenAI's API text-to-speech (`gpt-4o-mini-tts`) is billed per token to an API key and is not the plan, so it is out.

What the installed Codex actually offers, checked on 2026-10-09 against Codex 0.162.0-alpha.2 (the ChatGPT app's bundled `codex-cli/bin/codex`) and 0.153.4, with `codex app-server generate-json-schema --experimental` and a live session:

- The realtime methods are experimental: `thread/realtime/start`, `appendSpeech`, `appendText`, `appendAudio`, `listVoices`, `stop`. The server answers them only on a connection whose `initialize` sets `capabilities.experimentalApi`. Codex 0.153 has the `realtime_conversation` feature off by default; 0.162 has it on.
- `thread/realtime/start` takes a thread id, `outputModality`, a `version`, and a WebRTC transport carrying the browser's SDP offer. Its response is empty; the SDP answer arrives as a `thread/realtime/sdp` notification, and failures as `thread/realtime/error`.
- With a ChatGPT sign-in, `v2` is refused ("AVAS realtime calls require realtime v1 or v3") and `v1` asks for an alpha header the app-server does not send. Only `v3` connected.
- On `v3`, `appendSpeech` with a receive-only peer connection was accepted (`session.context.appended`) but nothing was spoken. With a silent outgoing audio track the same text was spoken word for word: the data channel reported `turn.created`, transcript deltas, then `turn.done` with the exact sentence, and the browser received about 45 KB of audio with rising audio energy. So v3 needs an incoming audio track to speak at all.
- `includeStartupContext` defaults to on. Codex's startup context can carry workspace details, so FNDR must turn it off.

## Decision

**Hybrid, text out only.**

1. Speech to text stays on the Mac (the existing Whisper and native helper path). Nothing about listening changes.
2. Only the reply text FNDR already decided to say goes to the Codex realtime session, through `thread/realtime/appendSpeech`. That text is FNDR-authored narration: fixed lines composed in code (`doNarration.ts`) and the agent's own answer. Never raw memory text, OCR, window titles or transcripts. The broker cannot see where text came from, so this is the caller's contract; the broker enforces what it can: at most 600 characters, nothing empty, and no other append method is ever called.
3. The microphone is never attached to the peer connection. Because v3 will not speak without an incoming track, the webview sends a track of digital silence made in an AudioContext (`realtimeOut.ts`, `silentSource`). It never calls `getUserMedia`. This is stated plainly in the code, the tests and here.
4. The session starts with `includeStartupContext: false`, `clientManagedHandoffs: true`, fixed read-aloud instructions and no other context, on an ephemeral, read-only thread in an empty directory, with Codex's acting features and every MCP server disabled (the same arguments as Screen Guide).
5. Private Mode (`is_incognito`) refuses start and speak in the broker, and status reports `private_mode`, so nothing goes out. The on/off switch stays where it already is: the speech registry's provider choice (`[voice_output]`), Screen Guide's spoken-answers setting, and Notch Do's mute. Muting means no call to `speak`, so nothing is sent.
6. Every send is in Privacy Activity as `voice_out` to `chatgpt.com`: the session's fixed instructions at start, and each line's bytes. No content.
7. A full live conversation, where microphone audio streams to OpenAI and the model answers by voice, is out of scope. It is a future opt-in Labs mode with its own consent screen naming the audio as the payload.

### Plan limits

Speaking draws on the person's plan. The broker reads both usage windows (`account/rateLimits/read`) and takes the higher one. Above 95 percent used (`FALLBACK_ABOVE_USED_PERCENT`) the status is `near_limit` and the provider reports itself unavailable, so the registry speaks with a Mac voice instead and the person keeps the rest of their limit for their own work. At 100 percent, or when Codex reports a reached limit, the status is `over_limit`. The UI can show `usedPercent` and `fallbackAbovePercent` from `voice_out_status`. Whether realtime voice counts against the same windows as Codex turns is not known.

### Shape

- `src-tauri/src/speech_out/mod.rs`: one broker for the app, one session at a time. A task owns the `codex app-server` for the session's life, so a closed call or an exited Codex is noticed at once. Start waits at most 25 s for the SDP answer; a start that fails or hangs kills that session. Ending the task drops the app-server, whose child is killed on drop. Commands: `voice_out_status`, `voice_out_voices`, `voice_out_start(sdpOffer, voice?)`, `voice_out_speak(text)`, `voice_out_cancel`, `voice_out_stop`.
- `src/shared/voice/realtimeOut.ts`: the `codex_realtime` `SpeechProvider`. A line ends on the data channel's `turn.done`, or after a length-based timeout. `cancel()` mutes and settles the line at once (well under 150 ms); a turn still being made is ended through `voice_out_cancel` only when the next line comes, which reconnects. A dropped connection rejects the line and makes `available()` report `network` for 30 s so the registry falls back.

## Options considered

| Option | Why not |
|---|---|
| A. Full live voice: microphone audio streams to OpenAI, the realtime model listens and answers | Sends the room's audio to a cloud service on every use. Recorded as a future Labs opt-in with its own consent. |
| B. Hybrid, text out only (chosen) | Natural voice from the plan; the only content that leaves is a line FNDR already wrote. Costs a WebRTC call per session and a silent outgoing track. |
| C. OpenAI API text-to-speech | Per-token billing to an API key, not the plan the person connected. |
| D. Keep only Mac voices | No cloud at all, but the voices on a Mac without Premium voices are what prompted this. Stays the fallback. |
| E. Receive-only peer connection | Tried live: v3 accepts the text and stays silent. |
| F. `appendText` with a user role | Would make the model answer the text rather than read it, and could pull in Codex handoffs. |

## Consequences

- A WebRTC offer carries the Mac's ICE candidates, which can include local and public IP addresses, to OpenAI's media servers. That is inherent to WebRTC and true of any realtime call.
- The read-aloud instructions live in `speech_out/mod.rs`. AGENTS.md wants model-facing strings in `inference/prompts.rs` with a catalog row; moving it there is a follow-up in that lane.
- Privacy Activity shows `voice_out` with its raw feature name until `PrivacyProof.tsx` gets a label.
- Registering the provider with the speech registry belongs to the registry's lane.

## Verification

- Unit: usage classification and the 95 percent rule, start parameters, SDP and error notification handling.
- Fake app-server (`tests/fixtures/operator/fake_codex_app_server.py`): status, signed out, near and over the limit, a Codex without realtime, no SDP answer (timeout and kill), a dropped session and reconnect, cancel keeping the thread, text-only egress, Private Mode refusal, ledger rows.
- Webview, fake RTCPeerConnection: speak and `turn.done`, silence as the only track and no `getUserMedia`, cancel latency, reconnect after a cancelled turn, a dropped connection and a failed start rejecting and turning `available()` off.
- Real Codex 0.162 and a ChatGPT sign-in (`live_voice_out_speaks_one_sentence`, ignored by default): status ready with usage, the voice list, a start answered with an SDP answer, one sentence accepted. A browser peer on the same protocol (a one-off probe, 2026-10-09) heard the sentence spoken word for word.

## Unverified

- Audio through FNDR's own webview (WKWebView): WebRTC, AudioContext and audio autoplay inside the packaged app have not been run.
- Which voices v3 accepts: `listVoices` returns v1 and v2 lists only; no voice was passed in the live runs.
- Whether realtime voice draws on the same usage windows as Codex turns, and how fast.
- Server-side interruption: no v3 event to stop a turn mid-sentence was found, so cancel mutes and ends the call instead.
- That `codex app-server` exits when FNDR quits without the exit handler stopping it (it should on stdin end; not checked for this session).
- The realtime methods are experimental and may change shape in any Codex release.
