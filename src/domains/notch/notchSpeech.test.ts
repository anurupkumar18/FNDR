import { describe, expect, it, vi } from "vitest";
import { createSpeaker, readMuted, writeMuted } from "./notchSpeech";

class FakeUtterance {
    onend: (() => void) | null = null;
    onerror: (() => void) | null = null;
    constructor(public text: string) {}
}

function fakeSynth() {
    const spoken: FakeUtterance[] = [];
    let current: FakeUtterance | null = null;
    return {
        spoken,
        get speaking() {
            return current !== null;
        },
        speak: vi.fn((u: FakeUtterance) => {
            spoken.push(u);
            current = u;
        }),
        cancel: vi.fn(() => {
            const u = current;
            current = null;
            u?.onerror?.();
        }),
        finish() {
            const u = current;
            current = null;
            u?.onend?.();
        },
    };
}

function speaker(synth = fakeSynth(), now = () => 0) {
    return { synth, speaker: createSpeaker(synth as never, FakeUtterance as never, now)! };
}

describe("createSpeaker", () => {
    it("speaks a line and queues only the newest non-urgent line behind it", () => {
        const { synth, speaker: s } = speaker();
        s.say({ text: "one", urgent: false });
        s.say({ text: "two", urgent: false });
        s.say({ text: "three", urgent: false });
        expect(synth.spoken.map((u) => u.text)).toEqual(["one"]);
        synth.finish();
        expect(synth.spoken.map((u) => u.text)).toEqual(["one", "three"]);
    });

    it("lets an urgent line cut in and drops the queue", () => {
        const { synth, speaker: s } = speaker();
        s.say({ text: "one", urgent: false });
        s.say({ text: "two", urgent: false });
        s.say({ text: "approve?", urgent: true });
        expect(synth.cancel).toHaveBeenCalled();
        expect(synth.spoken.map((u) => u.text)).toEqual(["one", "approve?"]);
        synth.finish();
        expect(synth.spoken).toHaveLength(2);
    });

    it("cancel silences at once and nothing queued starts afterwards", () => {
        const { synth, speaker: s } = speaker();
        s.say({ text: "one", urgent: false });
        s.say({ text: "two", urgent: false });
        s.cancel();
        expect(synth.speaking).toBe(false);
        expect(synth.spoken).toHaveLength(1);
        expect(s.echoText()).toBe("");
    });

    it("keeps what was said as the echo window only while speaking and just after", () => {
        let t = 0;
        const { synth, speaker: s } = speaker(fakeSynth(), () => t);
        s.say({ text: "Understood: open Spotify.", urgent: false });
        expect(s.echoText()).toContain("open Spotify");
        t = 1000;
        synth.finish();
        t = 2000;
        expect(s.echoText()).toContain("open Spotify");
        t = 5000;
        expect(s.echoText()).toBe("");
    });

    it("shortens very long lines", () => {
        const { synth, speaker: s } = speaker();
        s.say({ text: "word ".repeat(200), urgent: false });
        expect(synth.spoken[0].text.length).toBeLessThanOrEqual(300);
    });

    it("is absent when the webview has no speech", () => {
        expect(createSpeaker(undefined, undefined)).toBeNull();
    });
});

describe("mute setting", () => {
    it("is on by default and remembers a mute", () => {
        window.localStorage.removeItem("fndr.notch.do.muted");
        expect(readMuted()).toBe(false);
        writeMuted(true);
        expect(readMuted()).toBe(true);
        writeMuted(false);
        expect(readMuted()).toBe(false);
    });
});
