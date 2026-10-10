/**
 * FNDR's on-device neural voice (ADR 030): Kokoro-82M run by the Rust side
 * through `tts_synthesize`, fully on this Mac. A line is split into
 * sentences; the next sentence is synthesized while the current one plays,
 * and clips are scheduled back to back on one AudioContext so there is no
 * gap between them. Cancel stops every scheduled clip at once.
 */

import { ttsCancel, ttsStatus, ttsSynthesize, type TtsClip, type TtsStatus } from "../ipc/tauri";
import type { SpeakOptions, SpeechProvider } from "./speechProvider";

/** The Kokoro voices FNDR ships (`src-tauri/src/tts/install.rs`). */
export const NEURAL_VOICE_IDS: readonly string[] = [
    "af_heart",
    "af_bella",
    "af_nicole",
    "af_aoede",
    "af_kore",
    "af_sarah",
    "am_fenrir",
    "am_michael",
    "am_puck",
    "bf_emma",
    "bf_isabella",
    "bm_fable",
    "bm_george",
];

/** A sentence longer than this is split at a comma, so the first audio comes sooner. */
const MAX_PART_CHARS = 220;
const STATUS_TTL_MS = 3_000;
const ABBREVIATIONS = /(?:\b(?:Dr|Mr|Mrs|Ms|Jr|Sr|Prof|St|vs|etc|approx|No)|\b[A-Z]|\be\.g|\bi\.e)\.$/;

export interface LocalNeuralIpc {
    status(): Promise<TtsStatus>;
    synthesize(text: string, voice?: string, speed?: number): Promise<TtsClip>;
    cancel(): Promise<void>;
}

/** Plays WAV clips back to back. */
export interface ClipPlayer {
    /** Resolves when the clip has finished playing, or at once after `stop`. */
    enqueue(wav: ArrayBuffer): Promise<void>;
    stop(): void;
}

export interface LocalNeuralDeps {
    ipc?: LocalNeuralIpc;
    createPlayer?: () => ClipPlayer | null;
    now?: () => number;
}

const defaultIpc: LocalNeuralIpc = { status: ttsStatus, synthesize: ttsSynthesize, cancel: ttsCancel };

/** Splits a line into sentences, keeping "Dr. Smith" and "e.g. this" whole. */
export function splitSentences(text: string): string[] {
    const words = text.replace(/\s+/g, " ").trim().split(" ").filter(Boolean);
    const sentences: string[] = [];
    let current: string[] = [];
    for (const [index, word] of words.entries()) {
        current.push(word);
        const next = words[index + 1];
        const ends = /[.!?…]["”)]*$/.test(word) && !ABBREVIATIONS.test(word.replace(/["”)]+$/, ""));
        if (ends && (next === undefined || /^["“(]?[A-Z0-9]/.test(next))) {
            sentences.push(current.join(" "));
            current = [];
        }
    }
    if (current.length) sentences.push(current.join(" "));
    return sentences.flatMap(splitLong);
}

function splitLong(sentence: string): string[] {
    if (sentence.length <= MAX_PART_CHARS) return [sentence];
    const head = sentence.slice(0, MAX_PART_CHARS);
    const cut = Math.max(head.lastIndexOf(", "), head.lastIndexOf("; "), head.lastIndexOf(": "));
    const at = cut > 40 ? cut + 1 : head.lastIndexOf(" ") > 40 ? head.lastIndexOf(" ") : MAX_PART_CHARS;
    return [sentence.slice(0, at).trim(), ...splitLong(sentence.slice(at).trim())].filter(Boolean);
}

/** One AudioContext; each clip starts when the previous one ends. */
export function createAudioContextPlayer(): ClipPlayer | null {
    if (typeof AudioContext === "undefined") return null;
    let context: AudioContext | null = null;
    let nextStart = 0;
    const playing = new Set<{ source: AudioBufferSourceNode; done: () => void }>();
    return {
        async enqueue(wav) {
            context ??= new AudioContext();
            if (context.state === "suspended") await context.resume();
            const buffer = await context.decodeAudioData(wav.slice(0));
            const source = context.createBufferSource();
            source.buffer = buffer;
            source.connect(context.destination);
            const startAt = Math.max(context.currentTime, nextStart);
            nextStart = startAt + buffer.duration;
            return new Promise<void>((resolve) => {
                const entry = {
                    source,
                    done: () => {
                        playing.delete(entry);
                        resolve();
                    },
                };
                playing.add(entry);
                source.onended = entry.done;
                source.start(startAt);
            });
        },
        stop() {
            nextStart = 0;
            for (const entry of [...playing]) {
                entry.source.onended = null;
                try {
                    entry.source.stop();
                } catch {
                    // Already stopped.
                }
                entry.done();
            }
        },
    };
}

export function isNeuralVoice(voice: string | undefined): voice is string {
    return voice !== undefined && NEURAL_VOICE_IDS.includes(voice);
}

function describe(status: TtsStatus): string {
    switch (status.state) {
        case "not_installed":
            return "The on-device voice is not installed.";
        case "downloading": {
            const percent = status.totalBytes ? Math.floor((status.downloadedBytes / status.totalBytes) * 100) : 0;
            return `The on-device voice is downloading (${percent}%).`;
        }
        case "error":
            return `The on-device voice could not be installed: ${status.detail ?? "unknown error"}`;
        default:
            return "";
    }
}

export function createLocalNeuralProvider(deps: LocalNeuralDeps = {}): SpeechProvider {
    const ipc = deps.ipc ?? defaultIpc;
    const now = deps.now ?? (() => Date.now());
    const player = deps.createPlayer ? deps.createPlayer() : createAudioContextPlayer();
    let cached: { at: number; status: TtsStatus } | null = null;
    let generation = 0;
    let audible = false;
    const stops = new Set<() => void>();

    const cancel = () => {
        generation += 1;
        audible = false;
        player?.stop();
        stops.forEach((stop) => stop());
        stops.clear();
        void ipc.cancel().catch(() => undefined);
    };

    return {
        id: "local_neural",
        label: "On-device neural voice",
        async available() {
            if (!player) return { ok: false, reason: "This window cannot play audio." };
            if (!cached || now() - cached.at > STATUS_TTL_MS) {
                cached = { at: now(), status: await ipc.status() };
            }
            const status = cached.status;
            return status.state === "ready" ? { ok: true } : { ok: false, reason: describe(status) };
        },
        async speak(text: string, opts: SpeakOptions = {}) {
            if (!player) throw new Error("This window cannot play audio.");
            if (opts.urgent || audible) cancel();
            const mine = ++generation;
            const parts = splitSentences(text);
            if (!parts.length) return;
            const voice = isNeuralVoice(opts.voice) ? opts.voice : undefined;
            const speed = Math.min(2, Math.max(0.5, opts.rate ?? 1));
            let stopped!: () => void;
            const stopSignal = new Promise<"stopped">((resolve) => {
                stopped = () => resolve("stopped");
            });
            stops.add(stopped);
            const synth = (part: string) => {
                const clip = ipc.synthesize(part, voice, speed);
                clip.catch(() => undefined);
                return clip;
            };
            try {
                let pending: Promise<TtsClip> | null = synth(parts[0]);
                let previousEnd: Promise<void> = Promise.resolve();
                let lastEnd: Promise<void> = Promise.resolve();
                for (let index = 0; index < parts.length && pending; index += 1) {
                    const clip = await Promise.race([pending, stopSignal]);
                    if (clip === "stopped" || mine !== generation) return;
                    const ended = player.enqueue(clip.wav);
                    audible = true;
                    pending = null;
                    if (index + 1 < parts.length) {
                        // At most one clip waits behind the one playing.
                        if ((await Promise.race([previousEnd, stopSignal])) === "stopped" || mine !== generation) return;
                        pending = synth(parts[index + 1]);
                    }
                    previousEnd = ended;
                    lastEnd = ended;
                }
                await Promise.race([lastEnd, stopSignal]);
            } finally {
                stops.delete(stopped);
                if (mine === generation) audible = false;
            }
        },
        cancel,
        speaking() {
            return audible;
        },
    };
}
