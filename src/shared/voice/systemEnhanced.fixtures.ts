import { vi } from "vitest";
import type { VoiceLike } from "./systemEnhanced";

export function voice(name: string, voiceURI: string, lang = "en-US", localService = true, isDefault = false): VoiceLike {
    return { name, voiceURI, lang, localService, default: isDefault };
}

/** What WKWebView in this Tauri build listed on the owner's Mac on 2026-10-09 (no Premium voices installed). */
export const OWNER_MAC = [
    voice("Samantha", "com.apple.voice.compact.en-US.Samantha", "en-US", true, true),
    voice("Samantha", "com.apple.voice.super-compact.en-US.Samantha"),
    voice("Albert", "com.apple.speech.synthesis.voice.Albert"),
    voice("Bad News", "com.apple.speech.synthesis.voice.BadNews"),
    voice("Zarvox", "com.apple.speech.synthesis.voice.Zarvox"),
    voice("Fred", "com.apple.speech.synthesis.voice.Fred"),
    voice("Daniel", "com.apple.voice.super-compact.en-GB.Daniel", "en-GB"),
    voice("Karen", "com.apple.voice.super-compact.en-AU.Karen", "en-AU"),
    voice("Thomas", "com.apple.voice.compact.fr-FR.Thomas", "fr-FR"),
];

export class FakeUtterance {
    text: string;
    voice: VoiceLike | null = null;
    rate = 1;
    pitch = 1;
    lang = "";
    onstart: (() => void) | null = null;
    onend: (() => void) | null = null;
    onerror: ((e: { error: string }) => void) | null = null;
    constructor(text: string) {
        this.text = text;
    }
}

export function fakeSynth(voices: VoiceLike[]) {
    const queue: FakeUtterance[] = [];
    const synth = {
        spoken: [] as FakeUtterance[],
        speaking: false,
        getVoices: () => voices,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        speak(u: FakeUtterance) {
            synth.spoken.push(u);
            queue.push(u);
            synth.speaking = true;
            if (queue.length === 1) u.onstart?.();
        },
        cancel: vi.fn(() => {
            const pending = queue.splice(0);
            synth.speaking = false;
            pending.forEach((u) => u.onerror?.({ error: "canceled" }));
        }),
        /** Finishes the utterance at the head of the queue. */
        end() {
            const u = queue.shift();
            if (queue.length === 0) synth.speaking = false;
            u?.onend?.();
            queue[0]?.onstart?.();
        },
        fail(error: string) {
            const u = queue.shift();
            synth.speaking = queue.length > 0;
            u?.onerror?.({ error });
        },
    };
    return synth;
}
