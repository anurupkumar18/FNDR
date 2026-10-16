/**
 * FNDR's natural voice from the ChatGPT plan (ADR 028): a WebRTC peer
 * connection to the Codex realtime voice, set up through the `voice_out_*`
 * commands. Only reply text FNDR already decided to say leaves the Mac, through
 * `voiceOutSpeak`. The microphone is never opened: realtime v3 does not speak
 * without an incoming audio track, so the connection sends a silent one made
 * in an AudioContext.
 *
 * Speech ends on the data channel's `turn.done` (seen live on 2026-10-09).
 * Cancel mutes at once and settles the pending line; a turn still being made
 * is discarded by ending the call, and the next line reconnects.
 */

import {
    voiceOutCancel,
    voiceOutSpeak,
    voiceOutStart,
    voiceOutStatus,
    voiceOutStop,
    voiceOutVoices,
    type VoiceOutStatus,
} from "../ipc/tauri";
import type { SpeakOptions, SpeechProvider } from "./speechProvider";

/** After a failure the provider reports itself unavailable this long, so the registry falls back. */
const FAILURE_COOLDOWN_MS = 30_000;
const CONNECT_TIMEOUT_MS = 10_000;
/** A line that never reports its end still ends: a floor plus time per character. */
const FINISH_FLOOR_MS = 4_000;
const FINISH_PER_CHAR_MS = 90;

export interface VoiceOutIpc {
    status(): Promise<VoiceOutStatus>;
    start(sdpOffer: string, voice?: string): Promise<string>;
    speak(text: string): Promise<void>;
    cancel(): Promise<void>;
    stop(): Promise<void>;
}

/** The parts of an RTCPeerConnection this provider uses. */
export interface PeerLike {
    connectionState: string;
    onconnectionstatechange: (() => void) | null;
    ontrack: ((event: { streams: readonly MediaStream[]; track: MediaStreamTrack }) => void) | null;
    addTrack(track: MediaStreamTrack, stream: MediaStream): unknown;
    createDataChannel(label: string): ChannelLike;
    createOffer(): Promise<{ sdp?: string }>;
    setLocalDescription(description: { sdp?: string; type?: string }): Promise<void>;
    setRemoteDescription(description: { type: "answer"; sdp: string }): Promise<void>;
    close(): void;
}

export interface ChannelLike {
    readyState: string;
    onmessage: ((event: { data: unknown }) => void) | null;
    onopen: (() => void) | null;
    close(): void;
}

export interface AudioOutLike {
    srcObject: MediaStream | null;
    muted: boolean;
    play(): Promise<void>;
    pause(): void;
}

export interface SilentSource {
    track: MediaStreamTrack;
    stream: MediaStream;
    stop(): void;
}

export interface RealtimeOutDeps {
    ipc?: VoiceOutIpc;
    createPeer?: () => PeerLike;
    createSilentSource?: () => SilentSource;
    createAudio?: () => AudioOutLike;
    /** Called with true while FNDR's voice is audible, false when it stops. */
    onDuck?: (ducking: boolean) => void;
    now?: () => number;
}

export interface RealtimeOutProvider extends SpeechProvider {
    voices(): Promise<string[]>;
    /** Ends the call and the Codex session behind it. */
    close(): Promise<void>;
}

const defaultIpc: VoiceOutIpc = {
    status: voiceOutStatus,
    start: voiceOutStart,
    speak: voiceOutSpeak,
    cancel: voiceOutCancel,
    stop: voiceOutStop,
};

/** A track of digital silence. Never the microphone. */
function silentSource(): SilentSource {
    const context = new AudioContext();
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    gain.gain.value = 0;
    const destination = context.createMediaStreamDestination();
    oscillator.connect(gain);
    gain.connect(destination);
    oscillator.start();
    const stream = destination.stream;
    return {
        track: stream.getAudioTracks()[0],
        stream,
        stop() {
            oscillator.stop();
            void context.close();
        },
    };
}

function webrtcAvailable(): boolean {
    return typeof RTCPeerConnection !== "undefined" && typeof AudioContext !== "undefined";
}

const UNAVAILABLE: Record<Exclude<VoiceOutStatus["state"], "ready">, string> = {
    not_installed: "Codex is not installed",
    broken: "Codex would not start",
    signed_out: "not signed in with ChatGPT",
    unsupported: "this Codex has no realtime voice",
    near_limit: "the ChatGPT plan is near its usage limit",
    over_limit: "the ChatGPT plan is over its usage limit",
    private_mode: "Private Mode is on",
};

interface Line {
    settle(error?: Error): void;
}

export function createRealtimeOut(deps: RealtimeOutDeps = {}): RealtimeOutProvider {
    const ipc = deps.ipc ?? defaultIpc;
    const createPeer = deps.createPeer ?? (() => new RTCPeerConnection() as unknown as PeerLike);
    const createSilent = deps.createSilentSource ?? silentSource;
    const createAudio = deps.createAudio ?? (() => new Audio() as AudioOutLike);
    const now = deps.now ?? Date.now;
    const duck = (on: boolean) => deps.onDuck?.(on);

    let peer: PeerLike | null = null;
    let channel: ChannelLike | null = null;
    let silent: SilentSource | null = null;
    let audio: AudioOutLike | null = null;
    let connecting: Promise<void> | null = null;
    let line: Line | null = null;
    let audible = false;
    /** A cancelled turn the voice is still making, muted. */
    let discarding = false;
    let failedAt = 0;
    let failure = "";

    const fail = (reason: string) => {
        failedAt = now();
        failure = reason;
    };

    const teardown = () => {
        channel?.close();
        peer?.close();
        silent?.stop();
        if (audio) {
            audio.pause();
            audio.srcObject = null;
        }
        peer = null;
        channel = null;
        silent = null;
        connecting = null;
        discarding = false;
        if (audible) duck(false);
        audible = false;
    };

    const onEvent = (raw: unknown) => {
        let type = "";
        try {
            type = (JSON.parse(String(raw)) as { type?: string }).type ?? "";
        } catch {
            return;
        }
        if (type === "turn.created" || type === "output_transcript.added") {
            if (!discarding && !audible) {
                audible = true;
                duck(true);
            }
        } else if (type === "turn.done") {
            if (discarding) {
                discarding = false;
                return;
            }
            if (audible) duck(false);
            audible = false;
            line?.settle();
        } else if (type === "error" && line) {
            line.settle(new Error("The ChatGPT voice reported an error."));
        }
    };

    const connect = async (voice?: string) => {
        const pc = createPeer();
        peer = pc;
        silent = createSilent();
        pc.addTrack(silent.track, silent.stream);
        const dc = pc.createDataChannel("oai-events");
        channel = dc;
        dc.onmessage = (event) => onEvent(event.data);
        audio = audio ?? createAudio();
        const out = audio;
        pc.ontrack = (event) => {
            out.srcObject = event.streams[0] ?? new MediaStream([event.track]);
            void out.play().catch(() => undefined);
        };
        pc.onconnectionstatechange = () => {
            const state = pc.connectionState;
            if (peer === pc && (state === "failed" || state === "closed" || state === "disconnected")) {
                fail("network");
                teardown();
                line?.settle(new Error("The ChatGPT voice connection dropped."));
            }
        };
        const offer = await pc.createOffer();
        await pc.setLocalDescription(offer);
        const answer = await ipc.start(offer.sdp ?? "", voice);
        if (peer !== pc) throw new Error("The ChatGPT voice was closed while connecting.");
        await pc.setRemoteDescription({ type: "answer", sdp: answer });
        await new Promise<void>((resolve, reject) => {
            const timer = setTimeout(() => reject(new Error("The ChatGPT voice did not connect in time.")), CONNECT_TIMEOUT_MS);
            const ready = () => {
                if (dc.readyState === "open") {
                    clearTimeout(timer);
                    resolve();
                }
            };
            dc.onopen = ready;
            ready();
        });
    };

    const ensureConnected = async (voice?: string) => {
        if (discarding) {
            // The cancelled turn is still being made: end it rather than wait.
            await ipc.cancel().catch(() => undefined);
            teardown();
        }
        if (peer && channel?.readyState === "open") return;
        if (!connecting) {
            connecting = connect(voice).catch((error: unknown) => {
                teardown();
                throw error;
            });
        }
        await connecting;
    };

    const provider: RealtimeOutProvider = {
        id: "codex_realtime",
        label: "ChatGPT voice",

        async available() {
            if (!webrtcAvailable() && !deps.createPeer) return { ok: false, reason: "WebRTC is not available" };
            if (failedAt && now() - failedAt < FAILURE_COOLDOWN_MS) return { ok: false, reason: failure };
            let status: VoiceOutStatus;
            try {
                status = await ipc.status();
            } catch {
                return { ok: false, reason: "Codex did not answer" };
            }
            if (status.state === "ready") return { ok: true };
            return { ok: false, reason: UNAVAILABLE[status.state] ?? status.state };
        },

        async speak(text: string, opts?: SpeakOptions) {
            line?.settle();
            const finish = new Promise<void>((resolve, reject) => {
                let timer: ReturnType<typeof setTimeout> | undefined;
                const mine: Line = {
                    settle(error) {
                        if (line !== mine) return;
                        line = null;
                        if (timer) clearTimeout(timer);
                        if (error) reject(error);
                        else resolve();
                    },
                };
                line = mine;
                timer = setTimeout(() => {
                    if (audible) duck(false);
                    audible = false;
                    mine.settle();
                }, FINISH_FLOOR_MS + text.length * FINISH_PER_CHAR_MS);
            });
            const mine = line;
            try {
                await ensureConnected(opts?.voice);
                if (line !== mine) return finish;
                if (audio) audio.muted = false;
                try {
                    await ipc.speak(text);
                } catch {
                    // The session may have dropped since it connected: once more, fresh.
                    teardown();
                    await ensureConnected(opts?.voice);
                    if (line !== mine) return finish;
                    if (audio) audio.muted = false;
                    await ipc.speak(text);
                }
            } catch (error) {
                fail(error instanceof Error ? error.message : "network");
                mine?.settle(error instanceof Error ? error : new Error(String(error)));
            }
            return finish;
        },

        cancel() {
            if (audio) audio.muted = true;
            if (audible) {
                discarding = true;
                duck(false);
            }
            audible = false;
            line?.settle();
        },

        speaking() {
            return audible;
        },

        async voices() {
            try {
                return (await voiceOutVoices()).voices;
            } catch {
                return [];
            }
        },

        async close() {
            provider.cancel();
            teardown();
            await ipc.stop().catch(() => undefined);
        },
    };
    return provider;
}
