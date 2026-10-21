/**
 * The last resort: the webview's default system voice, one utterance capped at
 * 300 characters, exactly what the notch speaker (`notchSpeech.ts`) does today.
 */

import type { SpeakOptions, SpeechProvider } from "./speechProvider";
import { createUtterancePlayer, type MakeUtterance, type SynthLike } from "./systemEnhanced";

const MAX_SPOKEN_CHARS = 300;

export function createWebviewBasicProvider(
    deps: { synth?: SynthLike; Utterance?: MakeUtterance } = {
        synth: typeof window === "undefined" || !window.speechSynthesis ? undefined : (window.speechSynthesis as unknown as SynthLike),
        Utterance: typeof SpeechSynthesisUtterance === "undefined" ? undefined : (SpeechSynthesisUtterance as unknown as MakeUtterance),
    },
): SpeechProvider {
    const player = deps.synth && deps.Utterance ? createUtterancePlayer(deps.synth, deps.Utterance) : null;
    return {
        id: "webview_basic",
        label: "Basic system voice",
        async available() {
            return player ? { ok: true } : { ok: false, reason: "This window has no speech synthesis." };
        },
        speak(text: string, opts: SpeakOptions = {}) {
            if (!player) return Promise.reject(new Error("This window has no speech synthesis."));
            const line = text.trim().slice(0, MAX_SPOKEN_CHARS);
            return player.play(line ? [line] : [], (u) => {
                if (opts.rate !== undefined) u.rate = opts.rate;
            });
        },
        cancel() {
            player?.cancel();
        },
        speaking() {
            return player?.speaking() ?? false;
        },
    };
}
