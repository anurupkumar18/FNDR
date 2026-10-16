import { describe, expect, it } from "vitest";
import { chunkForSpeech, createSystemEnhancedProvider, previewVoice, rankVoices } from "./systemEnhanced";
import { FakeUtterance, fakeSynth, OWNER_MAC, voice } from "./systemEnhanced.fixtures";

/** A Mac with Premium and Enhanced voices downloaded from Spoken Content. */
const DOWNLOADED = [
    ...OWNER_MAC,
    voice("Zoe (Premium)", "com.apple.voice.premium.en-US.Zoe"),
    voice("Ava (Enhanced)", "com.apple.voice.enhanced.en-US.Ava"),
    voice("Serena", "com.apple.voice.premium.en-GB.Serena", "en-GB"),
    voice("Google US English", "Google US English", "en-US", false),
];

describe("rankVoices", () => {
    it("puts Premium, then Enhanced, then compact voices first, in the person's language", () => {
        const ranked = rankVoices(DOWNLOADED, "en-US");
        expect(ranked.slice(0, 4).map((v) => [v.name, v.quality])).toEqual([
            ["Zoe", "premium"],
            ["Serena", "premium"],
            ["Ava", "enhanced"],
            ["Samantha", "compact"],
        ]);
    });

    it("prefers the exact region when quality is equal", () => {
        const ranked = rankVoices(OWNER_MAC, "en-GB");
        expect(ranked[0].name).toBe("Daniel");
    });

    it("leaves out novelty voices, network voices, other languages and duplicates", () => {
        const names = rankVoices(DOWNLOADED, "en-US").map((v) => v.name);
        expect(names).not.toContain("Zarvox");
        expect(names).not.toContain("Bad News");
        expect(names).not.toContain("Albert");
        expect(names).not.toContain("Google US English");
        expect(names).not.toContain("Thomas");
        expect(names.filter((n) => n === "Samantha")).toHaveLength(1);
    });

    it("falls back to every local voice when none speaks the language", () => {
        const ranked = rankVoices(OWNER_MAC, "ja-JP");
        expect(ranked.length).toBeGreaterThan(0);
        expect(ranked.map((v) => v.name)).not.toContain("Zarvox");
    });

    it("keeps a Siri voice at the top when the webview offers one", () => {
        const ranked = rankVoices([...OWNER_MAC, voice("Siri Voice 4", "com.apple.speech.voice.siri.en-US.4")], "en-US");
        expect(ranked[0]).toMatchObject({ quality: "siri" });
    });
});

describe("chunkForSpeech", () => {
    it("speaks a short line in one piece", () => {
        expect(chunkForSpeech("Opening Safari.")).toEqual(["Opening Safari."]);
    });

    it("splits long text at sentence ends and keeps each piece short", () => {
        const text = Array.from({ length: 8 }, (_, i) => `Step ${i + 1} opened the calendar and found the Friday meeting.`).join(" ");
        const chunks = chunkForSpeech(text, 160);
        expect(chunks.length).toBeGreaterThan(2);
        chunks.forEach((c) => expect(c.length).toBeLessThanOrEqual(160));
        expect(chunks.join(" ")).toBe(text);
    });

    it("breaks a sentence with no stop at a comma or space", () => {
        const text = "word ".repeat(80).trim();
        const chunks = chunkForSpeech(text, 100);
        chunks.forEach((c) => expect(c.length).toBeLessThanOrEqual(100));
        expect(chunks.join(" ")).toBe(text);
    });

    it("drops empty input", () => {
        expect(chunkForSpeech("   ")).toEqual([]);
    });
});

describe("system_enhanced provider", () => {
    it("is available when a usable voice is installed and says when only compact voices exist", async () => {
        const provider = createSystemEnhancedProvider({ synth: fakeSynth(OWNER_MAC), Utterance: FakeUtterance, lang: "en-US" });
        const result = await provider.available();
        expect(result.ok).toBe(true);
        expect(result.reason).toMatch(/Premium/);
        const best = createSystemEnhancedProvider({ synth: fakeSynth(DOWNLOADED), Utterance: FakeUtterance, lang: "en-US" });
        expect(await best.available()).toEqual({ ok: true });
    });

    it("is unavailable without speech synthesis or voices", async () => {
        const none = createSystemEnhancedProvider({ synth: undefined, Utterance: FakeUtterance });
        expect((await none.available()).ok).toBe(false);
        const empty = createSystemEnhancedProvider({ synth: fakeSynth([]), Utterance: FakeUtterance, voiceWaitMs: 0 });
        expect((await empty.available()).ok).toBe(false);
    });

    it("speaks with the best voice in sentence chunks and resolves after the last one", async () => {
        const synth = fakeSynth(DOWNLOADED);
        const provider = createSystemEnhancedProvider({ synth, Utterance: FakeUtterance, lang: "en-US", maxChunk: 40 });
        let done = false;
        const speech = provider.speak("The calendar is open. The Friday meeting is at noon.").then(() => (done = true));
        expect(synth.spoken.map((u) => u.text)).toEqual(["The calendar is open.", "The Friday meeting is at noon."]);
        expect(synth.spoken[0].voice?.voiceURI).toBe("com.apple.voice.premium.en-US.Zoe");
        expect(provider.speaking()).toBe(true);
        synth.end();
        await Promise.resolve();
        expect(done).toBe(false);
        synth.end();
        await speech;
        expect(provider.speaking()).toBe(false);
    });

    it("uses the chosen voice and rate", async () => {
        const synth = fakeSynth(DOWNLOADED);
        const provider = createSystemEnhancedProvider({ synth, Utterance: FakeUtterance, lang: "en-US" });
        const speech = provider.speak("Hi", { voice: "com.apple.voice.enhanced.en-US.Ava", rate: 1.2 });
        expect(synth.spoken[0].voice?.name).toBe("Ava (Enhanced)");
        expect(synth.spoken[0].rate).toBeCloseTo(1.2);
        synth.end();
        await speech;
    });

    it("resolves on cancel and rejects on a real synthesis error", async () => {
        const synth = fakeSynth(DOWNLOADED);
        const provider = createSystemEnhancedProvider({ synth, Utterance: FakeUtterance, lang: "en-US" });
        const cancelled = provider.speak("One. Two.");
        provider.cancel();
        await expect(cancelled).resolves.toBeUndefined();
        expect(provider.speaking()).toBe(false);
        const broken = provider.speak("Three.");
        synth.fail("synthesis-failed");
        await expect(broken).rejects.toThrow(/synthesis-failed/);
    });

    it("previews a voice with a sample line", async () => {
        const synth = fakeSynth(DOWNLOADED);
        const preview = previewVoice("com.apple.voice.premium.en-US.Zoe", { synth, Utterance: FakeUtterance });
        expect(synth.spoken[0].voice?.name).toBe("Zoe (Premium)");
        expect(synth.spoken[0].text.length).toBeGreaterThan(10);
        synth.end();
        await preview;
    });
});
