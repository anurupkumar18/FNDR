/**
 * Notch Do's voice, on the shared speech registry: the ChatGPT plan voice
 * when it is ready (ADR 028), else the best Mac voice, else the webview's
 * basic voice. This file decides only what Notch Do needs on top: one line at
 * a time cut into utterances of at most 140 characters, a newer progress line
 * replacing a waiting one, urgent lines cutting in, nothing said that
 * `speakable` would not say, and the recent lines kept for the echo guard.
 *
 * The mute lives in `config.toml` (`notch_do_muted`, ADR 020 amendment
 * 2026-10-09); the older per-window flag is migrated once.
 */

import { getNotchDoMuted, setNotchDoMuted, type VoiceOutStatus } from "@/shared/ipc/tauri";
import { createRealtimeOut, type RealtimeOutProvider } from "@/shared/voice/realtimeOut";
import type { SpeechProviderId } from "@/shared/voice/speechProvider";
import { installOfflineVoices, type SpeechRegistry } from "@/shared/voice/speechRegistry";
import { speakable, type Narration } from "./doNarration";

export const MAX_UTTERANCE_CHARS = 140;
/** FNDR's voice can still be in the microphone this long after it stops. */
const ECHO_TAIL_MS = 2000;
const LEGACY_MUTE_KEY = "fndr.notch.do.muted";
/** The usage share at which the notch starts saying how much of the plan is used. */
const USAGE_WARNING_PERCENT = 80;

export interface NotchVoice {
    /** Resolves when the line has been said, replaced, cancelled or has failed. */
    say(line: Narration): Promise<void>;
    /** Stops speaking now and forgets anything waiting. */
    cancel(): void;
    speaking(): boolean;
    /** What was just said, while it could still be heard back; else empty. */
    echoText(): string;
    onSpeaking(listener: (speaking: boolean) => void): () => void;
}

function cut(sentence: string): string[] {
    const parts: string[] = [];
    let current = "";
    for (const word of sentence.split(" ")) {
        if (current && current.length + 1 + word.length > MAX_UTTERANCE_CHARS) {
            parts.push(current);
            current = word.slice(0, MAX_UTTERANCE_CHARS);
        } else {
            current = current ? `${current} ${word}` : word.slice(0, MAX_UTTERANCE_CHARS);
        }
    }
    if (current) parts.push(current);
    return parts;
}

/** One utterance per sentence group of at most 140 characters, cut at sentence ends. */
export function splitLine(text: string): string[] {
    const clean = text.replace(/\s+/g, " ").trim();
    if (!clean) return [];
    // A sentence ends at punctuation followed by a space, so "canvas.utah.edu" stays whole.
    const sentences = clean.split(/(?<=[.!?]["')\]]*)\s+/).filter(Boolean);
    const out: string[] = [];
    let current = "";
    for (const sentence of sentences.flatMap(cut)) {
        if (current && current.length + 1 + sentence.length <= MAX_UTTERANCE_CHARS) current = `${current} ${sentence}`;
        else {
            if (current) out.push(current);
            current = sentence;
        }
    }
    if (current) out.push(current);
    return out;
}

interface Waiting {
    parts: string[];
    urgent: boolean;
    done: () => void;
}

export function createNotchVoice(registry: SpeechRegistry, now: () => number = Date.now): NotchVoice {
    let generation = 0;
    let active: Waiting | null = null;
    let waiting: Waiting | null = null;
    let recent: string[] = [];
    let endedAt = 0;
    const listeners = new Set<(speaking: boolean) => void>();
    const notify = (on: boolean) => listeners.forEach((listener) => listener(on));

    const play = async (line: Waiting) => {
        const mine = ++generation;
        const wasActive = active !== null;
        active = line;
        if (!wasActive) notify(true);
        recent = [...recent, line.parts.join(" ")].slice(-3);
        endedAt = 0;
        for (const [index, part] of line.parts.entries()) {
            const outcome = await registry.speak(part, { urgent: line.urgent && index === 0 });
            if (mine !== generation) {
                line.done();
                return;
            }
            if (!outcome.spoken && outcome.reason === "cancelled") break;
        }
        line.done();
        const next = waiting;
        waiting = null;
        if (next) {
            void play(next);
            return;
        }
        active = null;
        endedAt = now();
        notify(false);
    };

    return {
        say(line) {
            const parts = splitLine(speakable(line.text));
            if (parts.length === 0) return Promise.resolve();
            return new Promise<void>((resolve) => {
                const item: Waiting = { parts, urgent: line.urgent, done: resolve };
                if (line.urgent) {
                    waiting?.done();
                    waiting = null;
                    active?.done();
                    void play(item);
                } else if (active) {
                    waiting?.done();
                    waiting = item;
                } else {
                    void play(item);
                }
            });
        },
        cancel() {
            generation += 1;
            waiting?.done();
            waiting = null;
            const was = active;
            active = null;
            recent = [];
            registry.cancel();
            was?.done();
            if (was) notify(false);
        },
        speaking() {
            return active !== null;
        },
        echoText() {
            if (recent.length === 0) return "";
            const live = active !== null || now() - endedAt < ECHO_TAIL_MS;
            return live ? recent.join(" ") : "";
        },
        onSpeaking(listener) {
            listeners.add(listener);
            return () => listeners.delete(listener);
        },
    };
}

let realtime: RealtimeOutProvider | null = null;

/**
 * Registers the voices Notch Do speaks with: the ChatGPT plan voice (its call
 * starts lazily at the first line, sends a silent track and never the
 * microphone) and the offline voices it falls back to.
 */
export function installNotchVoices(
    registry: SpeechRegistry,
    create: () => RealtimeOutProvider = createRealtimeOut,
): RealtimeOutProvider {
    installOfflineVoices(registry);
    if (!realtime || !registry.has("codex_realtime")) {
        realtime = create();
        registry.registerProvider(realtime);
    }
    return realtime;
}

/** The status line under the notch's caption: who is speaking. */
export function providerLine(id: SpeechProviderId, reason: string): string {
    if (id === "codex_realtime") return "Speaking with your ChatGPT plan";
    return reason && reason !== "preferred" ? `On-device voice (${reason})` : "On-device voice";
}

/** A warning near the plan's usage limit, or null. */
export function usageWarning(status: VoiceOutStatus): string | null {
    if (status.state === "over_limit") return "ChatGPT plan used up: speaking on this Mac";
    if (status.state === "near_limit") return "ChatGPT plan nearly used up: speaking on this Mac";
    if (status.state !== "ready" || status.usedPercent === null) return null;
    return status.usedPercent >= USAGE_WARNING_PERCENT ? `${Math.round(status.usedPercent)}% of your ChatGPT plan's limit is used` : null;
}

interface MuteDeps {
    get(): Promise<boolean | null>;
    set(muted: boolean): Promise<unknown>;
    storage: { getItem(key: string): string | null; removeItem(key: string): void } | null;
}

function browserStorage(): MuteDeps["storage"] {
    try {
        return typeof window === "undefined" ? null : window.localStorage;
    } catch {
        return null;
    }
}

/** The persisted mute. The first time, the older per-window flag is copied
 *  into `config.toml` and removed. */
export async function loadNotchMuted(
    deps: MuteDeps = { get: getNotchDoMuted, set: setNotchDoMuted, storage: browserStorage() },
): Promise<boolean> {
    let legacy = false;
    try {
        legacy = deps.storage?.getItem(LEGACY_MUTE_KEY) === "1";
    } catch {
        legacy = false;
    }
    try {
        const saved = await deps.get();
        if (saved === null) await deps.set(legacy);
        deps.storage?.removeItem(LEGACY_MUTE_KEY);
        return saved ?? legacy;
    } catch {
        return legacy;
    }
}

export function saveNotchMuted(muted: boolean): Promise<unknown> {
    return setNotchDoMuted(muted).catch(() => undefined);
}
