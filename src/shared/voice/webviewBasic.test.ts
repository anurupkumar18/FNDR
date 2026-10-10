import { describe, expect, it } from "vitest";
import { FakeUtterance, fakeSynth, OWNER_MAC } from "./systemEnhanced.fixtures";
import { createWebviewBasicProvider } from "./webviewBasic";

describe("webview_basic provider", () => {
    it("speaks with the system default voice, capped like the notch speaker", async () => {
        const synth = fakeSynth(OWNER_MAC);
        const provider = createWebviewBasicProvider({ synth, Utterance: FakeUtterance });
        expect(await provider.available()).toEqual({ ok: true });
        const speech = provider.speak("x".repeat(500), { voice: "com.apple.voice.premium.en-US.Zoe" });
        expect(synth.spoken).toHaveLength(1);
        expect(synth.spoken[0].text).toHaveLength(300);
        expect(synth.spoken[0].voice).toBeNull();
        synth.end();
        await speech;
    });

    it("is unavailable without speech synthesis", async () => {
        const provider = createWebviewBasicProvider({ synth: undefined, Utterance: undefined });
        expect((await provider.available()).ok).toBe(false);
    });
});
