import { describe, expect, it, vi } from "vitest";
import type { TtsClip, TtsStatus } from "../ipc/tauri";
import { createLocalNeuralProvider, splitSentences, type ClipPlayer, type LocalNeuralIpc } from "./localNeural";
import type { SpeechProvider } from "./speechProvider";
import { createSpeechRegistry } from "./speechRegistry";

const ready: TtsStatus = { state: "ready", detail: null, downloadedBytes: 1, totalBytes: 1, loaded: false };

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((r) => {
        resolve = r;
    });
    return { promise, resolve };
}

function clip(text: string): TtsClip {
    return { wav: new TextEncoder().encode(text).buffer as ArrayBuffer, durationMs: 1000 };
}

/** A player whose clips end when the test says so. */
class FakePlayer implements ClipPlayer {
    queued: string[] = [];
    ends: Array<() => void> = [];
    stopped = 0;
    enqueue(wav: ArrayBuffer) {
        this.queued.push(new TextDecoder().decode(wav));
        const done = deferred<void>();
        this.ends.push(() => done.resolve());
        return done.promise;
    }
    stop() {
        this.stopped += 1;
        this.ends.forEach((end) => end());
    }
}

function harness(status: TtsStatus = ready) {
    const player = new FakePlayer();
    const requests: Array<{ text: string; voice?: string; speed?: number }> = [];
    const ipc: LocalNeuralIpc = {
        status: vi.fn(async () => status),
        synthesize: vi.fn(async (text: string, voice?: string, speed?: number) => {
            requests.push({ text, voice, speed });
            return clip(text);
        }),
        cancel: vi.fn(async () => undefined),
    };
    let time = 0;
    const provider = createLocalNeuralProvider({ ipc, createPlayer: () => player, now: () => time });
    return { provider, player, ipc, requests, advance: (ms: number) => (time += ms) };
}

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("splitSentences", () => {
    it("splits at sentence ends but not after titles and e.g.", () => {
        expect(splitSentences("Dr. Smith is here. The meeting starts at 3 p.m. today! Ready? e.g. this one.")).toEqual([
            "Dr. Smith is here.",
            "The meeting starts at 3 p.m. today!",
            "Ready? e.g. this one.",
        ]);
    });

    it("splits a very long sentence at a comma", () => {
        const long = `${"word ".repeat(30)}and more, ${"text ".repeat(30)}end.`;
        const parts = splitSentences(long);
        expect(parts.length).toBeGreaterThan(1);
        expect(parts.every((part) => part.length <= 220)).toBe(true);
        expect(parts.join(" ").replace(/\s+/g, " ")).toBe(long.replace(/\s+/g, " ").trim());
    });
});

describe("local neural provider", () => {
    it("says why it cannot speak until the voice is installed", async () => {
        expect(await harness({ ...ready, state: "not_installed" }).provider.available()).toEqual({
            ok: false,
            reason: "The on-device voice is not installed.",
        });
        expect((await harness({ ...ready, state: "downloading", downloadedBytes: 42, totalBytes: 100 }).provider.available()).reason).toBe(
            "The on-device voice is downloading (42%).",
        );
        expect((await harness({ ...ready, state: "error", detail: "disk full" }).provider.available()).reason).toContain("disk full");
        expect(await harness().provider.available()).toEqual({ ok: true });
    });

    it("caches the status for a few seconds", async () => {
        const { provider, ipc, advance } = harness();
        await provider.available();
        await provider.available();
        expect(ipc.status).toHaveBeenCalledTimes(1);
        advance(5_000);
        await provider.available();
        expect(ipc.status).toHaveBeenCalledTimes(2);
    });

    it("synthesizes the next sentence while the current one plays and plays them in order", async () => {
        const { provider, player, requests } = harness();
        let finished = false;
        const speech = provider.speak("One here. Two here. Three here.").then(() => {
            finished = true;
        });
        await flush();
        expect(player.queued).toEqual(["One here.", "Two here."]);
        expect(requests.map((r) => r.text)).toEqual(["One here.", "Two here."]);
        expect(provider.speaking()).toBe(true);
        player.ends[0]();
        await flush();
        expect(requests.map((r) => r.text)).toEqual(["One here.", "Two here.", "Three here."]);
        expect(player.queued).toHaveLength(3);
        expect(finished).toBe(false);
        player.ends[1]();
        player.ends[2]();
        await speech;
        expect(finished).toBe(true);
        expect(provider.speaking()).toBe(false);
    });

    it("passes only Kokoro voice ids and clamps the speed", async () => {
        const first = harness();
        const a = first.provider.speak("Hi.", { voice: "com.apple.voice.premium.en-US.Zoe", rate: 9 });
        await flush();
        first.player.ends[0]();
        await a;
        expect(first.requests[0]).toEqual({ text: "Hi.", voice: undefined, speed: 2 });
        const second = harness();
        const b = second.provider.speak("Hi.", { voice: "bf_emma" });
        await flush();
        second.player.ends[0]();
        await b;
        expect(second.requests[0].voice).toBe("bf_emma");
    });

    it("cancel stops the audio at once, settles the line and asks Rust to stop", async () => {
        const { provider, player, ipc, requests } = harness();
        const speech = provider.speak("One here. Two here. Three here.");
        await flush();
        const started = performance.now();
        provider.cancel();
        await speech;
        expect(performance.now() - started).toBeLessThan(150);
        expect(player.stopped).toBe(1);
        expect(ipc.cancel).toHaveBeenCalled();
        expect(provider.speaking()).toBe(false);
        await flush();
        expect(requests.length).toBeLessThanOrEqual(2);
    });

    it("cancel while the first sentence is still being made settles without playing", async () => {
        const gate = deferred<TtsClip>();
        const { provider, player, ipc } = harness();
        vi.mocked(ipc.synthesize).mockImplementationOnce(() => gate.promise);
        const speech = provider.speak("Slow one.");
        provider.cancel();
        await speech;
        gate.resolve(clip("Slow one."));
        await flush();
        expect(player.queued).toEqual([]);
    });

    it("a failed synthesis rejects so the registry can fall back", async () => {
        const { provider, ipc } = harness();
        vi.mocked(ipc.synthesize).mockRejectedValueOnce(new Error("model missing"));
        await expect(provider.speak("Hello.")).rejects.toThrow("model missing");
    });
});

describe("registry order", () => {
    function fake(id: SpeechProvider["id"], spoken: string[]): SpeechProvider {
        return {
            id,
            label: id,
            available: async () => ({ ok: true }),
            speak: async (text) => {
                spoken.push(`${id}:${text}`);
            },
            cancel: () => undefined,
            speaking: () => false,
        };
    }

    it("falls back to the Mac voice while the neural voice is not installed", async () => {
        const spoken: string[] = [];
        const registry = createSpeechRegistry({ storage: null });
        registry.registerProvider(harness({ ...ready, state: "not_installed" }).provider);
        registry.registerProvider(fake("system_enhanced", spoken));
        expect(registry.order()).toEqual(["local_neural", "system_enhanced"]);
        expect(await registry.speak("Hello.")).toEqual({ spoken: true, provider: "system_enhanced" });
        expect(spoken).toEqual(["system_enhanced:Hello."]);
    });

    it("sits between the ChatGPT voice and the Mac voice", () => {
        const registry = createSpeechRegistry({ storage: null });
        registry.registerProvider(fake("system_enhanced", []));
        registry.registerProvider(harness().provider);
        registry.registerProvider(fake("codex_realtime", []));
        expect(registry.order()).toEqual(["local_neural", "codex_realtime", "system_enhanced"]);
    });
});
