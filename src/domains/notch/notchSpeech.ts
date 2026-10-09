/**
 * Notch Do's voice: the webview's own speech synthesis, which speaks with the
 * macOS system voices on this Mac and sends nothing anywhere. One utterance at
 * a time, in this process, so cancelling is immediate and nothing is left
 * running after the notch closes (ADR-020 amendment, 2026-10-08).
 */

import type { Narration } from "./doNarration";

const MAX_SPOKEN_CHARS = 300;
/** FNDR's voice can still be in the microphone this long after it stops. */
const ECHO_TAIL_MS = 2000;
const MUTE_KEY = "fndr.notch.do.muted";

type Synth = Pick<SpeechSynthesis, "speak" | "cancel" | "speaking">;
type MakeUtterance = new (text: string) => SpeechSynthesisUtterance;

export interface NotchSpeaker {
    say(line: Narration): void;
    /** Stops speaking now and forgets anything queued. */
    cancel(): void;
    /** What was just said, while it could still be heard back; else empty. */
    echoText(): string;
}

export function createSpeaker(
    synth: Synth | undefined = typeof window === "undefined" ? undefined : window.speechSynthesis,
    Utterance: MakeUtterance | undefined = typeof SpeechSynthesisUtterance === "undefined" ? undefined : SpeechSynthesisUtterance,
    now: () => number = Date.now,
): NotchSpeaker | null {
    if (!synth || !Utterance) return null;
    let pending: string | null = null;
    let recent: string[] = [];
    let lastEndedAt = 0;
    let turn = 0;

    const start = (text: string) => {
        const mine = turn;
        const utterance = new Utterance(text.length > MAX_SPOKEN_CHARS ? text.slice(0, MAX_SPOKEN_CHARS) : text);
        const ended = () => {
            if (mine !== turn) return;
            lastEndedAt = now();
            const next = pending;
            pending = null;
            if (next) start(next);
        };
        utterance.onend = ended;
        utterance.onerror = ended;
        recent = [...recent, text].slice(-3);
        lastEndedAt = 0;
        synth.speak(utterance);
    };

    return {
        say(line) {
            if (line.urgent) {
                turn += 1;
                pending = null;
                synth.cancel();
                start(line.text);
            } else if (synth.speaking) {
                pending = line.text;
            } else {
                start(line.text);
            }
        },
        cancel() {
            turn += 1;
            pending = null;
            recent = [];
            synth.cancel();
        },
        echoText() {
            if (recent.length === 0) return "";
            const live = synth.speaking || now() - lastEndedAt < ECHO_TAIL_MS;
            return live ? recent.join(" ") : "";
        },
    };
}

/** Speech is on unless the person muted it in the notch. */
export function readMuted(): boolean {
    try {
        return window.localStorage.getItem(MUTE_KEY) === "1";
    } catch {
        return false;
    }
}

export function writeMuted(muted: boolean): void {
    try {
        if (muted) window.localStorage.setItem(MUTE_KEY, "1");
        else window.localStorage.removeItem(MUTE_KEY);
    } catch {
        // The mute still holds for this session through component state.
    }
}
