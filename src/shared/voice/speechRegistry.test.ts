import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SpeakOptions, SpeechProvider, SpeechProviderId } from "./speechProvider";
import { createSpeechRegistry, DEFAULT_ORDER, installOfflineVoices, type SpeechRegistry } from "./speechRegistry";

interface FakeProvider extends SpeechProvider {
    spoken: { text: string; opts?: SpeakOptions }[];
    finish(): void;
    cancelled: number;
}

type Behavior = "speaks" | "unavailable" | "rejects" | "stalls" | "hangs_available";

function fake(id: SpeechProviderId, behavior: Behavior = "speaks"): FakeProvider {
    let active = false;
    let done: (() => void) | null = null;
    const provider: FakeProvider = {
        id,
        label: id,
        spoken: [],
        cancelled: 0,
        available: () => {
            if (behavior === "unavailable") return Promise.resolve({ ok: false, reason: `${id} is off` });
            if (behavior === "hangs_available") return new Promise(() => {});
            return Promise.resolve({ ok: true });
        },
        speak(text, opts) {
            provider.spoken.push({ text, opts });
            if (behavior === "rejects") return Promise.reject(new Error(`${id} broke`));
            if (behavior === "stalls") return new Promise<void>((resolve) => (done = resolve));
            active = true;
            return new Promise<void>((resolve) => {
                done = () => {
                    active = false;
                    resolve();
                };
            });
        },
        cancel() {
            provider.cancelled += 1;
            active = false;
            const end = done;
            done = null;
            end?.();
        },
        speaking: () => active,
        finish() {
            const end = done;
            done = null;
            end?.();
        },
    };
    return provider;
}

function memoryStorage(initial: Record<string, string> = {}) {
    const data = { ...initial };
    return {
        data,
        getItem: (k: string) => (k in data ? data[k] : null),
        setItem: (k: string, v: string) => {
            data[k] = v;
        },
    };
}

async function flush() {
    for (let i = 0; i < 50; i += 1) await Promise.resolve();
}

describe("speech registry", () => {
    let storage: ReturnType<typeof memoryStorage>;
    let registry: SpeechRegistry;

    beforeEach(() => {
        vi.useFakeTimers();
        storage = memoryStorage();
        registry = createSpeechRegistry({ storage, startTimeoutMs: 1500, cooldownMs: 60_000 });
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it("prefers the ChatGPT plan voice, then the neural, system and webview voices", () => {
        expect(DEFAULT_ORDER).toEqual(["codex_realtime", "local_neural", "system_enhanced", "webview_basic"]);
        registry.registerProvider(fake("webview_basic"));
        registry.registerProvider(fake("system_enhanced"));
        registry.registerProvider(fake("codex_realtime"));
        expect(registry.order()).toEqual(["codex_realtime", "system_enhanced", "webview_basic"]);
    });

    it("speaks with the first available provider", async () => {
        const codex = fake("codex_realtime");
        const system = fake("system_enhanced");
        registry.registerProvider(system);
        registry.registerProvider(codex);
        const done = registry.speak("Opening Safari");
        await flush();
        expect(codex.spoken.map((s) => s.text)).toEqual(["Opening Safari"]);
        expect(system.spoken).toEqual([]);
        codex.finish();
        await expect(done).resolves.toEqual({ spoken: true, provider: "codex_realtime" });
    });

    it("falls back to the next provider when one is unavailable or fails", async () => {
        const codex = fake("codex_realtime", "unavailable");
        const neural = fake("local_neural", "rejects");
        const system = fake("system_enhanced");
        [codex, neural, system].forEach((p) => registry.registerProvider(p));
        const changes: [SpeechProviderId, string][] = [];
        registry.onProviderChange((id, reason) => changes.push([id, reason]));
        const done = registry.speak("Step one");
        await flush();
        expect(system.spoken.map((s) => s.text)).toEqual(["Step one"]);
        system.finish();
        await expect(done).resolves.toEqual({ spoken: true, provider: "system_enhanced" });
        expect(changes).toHaveLength(1);
        expect(changes[0][0]).toBe("system_enhanced");
        expect(changes[0][1]).toContain("local_neural broke");
    });

    it("moves on when the first choice has not started within the start timeout", async () => {
        const codex = fake("codex_realtime", "stalls");
        const system = fake("system_enhanced");
        registry.registerProvider(codex);
        registry.registerProvider(system);
        const done = registry.speak("Hello");
        await flush();
        expect(system.spoken).toEqual([]);
        await vi.advanceTimersByTimeAsync(1500);
        expect(codex.cancelled).toBe(1);
        expect(system.spoken.map((s) => s.text)).toEqual(["Hello"]);
        system.finish();
        await expect(done).resolves.toMatchObject({ spoken: true, provider: "system_enhanced" });
    });

    it("treats an availability check that never answers as a stall", async () => {
        const codex = fake("codex_realtime", "hangs_available");
        const system = fake("system_enhanced");
        registry.registerProvider(codex);
        registry.registerProvider(system);
        registry.speak("Hello");
        await vi.advanceTimersByTimeAsync(1500);
        expect(system.spoken).toHaveLength(1);
    });

    it("skips a provider that failed until its cooldown has passed", async () => {
        const codex = fake("codex_realtime", "rejects");
        const system = fake("system_enhanced");
        registry.registerProvider(codex);
        registry.registerProvider(system);
        registry.speak("one");
        await flush();
        system.finish();
        await flush();
        registry.speak("two");
        await flush();
        expect(codex.spoken).toHaveLength(1);
        system.finish();
        await flush();
        await vi.advanceTimersByTimeAsync(60_000);
        registry.speak("three");
        await flush();
        expect(codex.spoken.map((s) => s.text)).toEqual(["one", "three"]);
    });

    it("reports nothing spoken when every provider fails, without throwing", async () => {
        registry.registerProvider(fake("system_enhanced", "unavailable"));
        registry.registerProvider(fake("webview_basic", "rejects"));
        await expect(registry.speak("hi")).resolves.toMatchObject({ spoken: false });
    });

    it("plays one utterance at a time and queues the rest", async () => {
        const system = fake("system_enhanced");
        registry.registerProvider(system);
        const first = registry.speak("first");
        const second = registry.speak("second");
        await flush();
        expect(system.spoken.map((s) => s.text)).toEqual(["first"]);
        expect(registry.speaking()).toBe(true);
        system.finish();
        await expect(first).resolves.toMatchObject({ spoken: true });
        await flush();
        expect(system.spoken.map((s) => s.text)).toEqual(["first", "second"]);
        system.finish();
        await expect(second).resolves.toMatchObject({ spoken: true });
    });

    it("keeps the queue small by dropping the oldest waiting line", async () => {
        const system = fake("system_enhanced");
        registry.registerProvider(system);
        registry.speak("now");
        const dropped = registry.speak("a");
        registry.speak("b");
        registry.speak("c");
        registry.speak("d");
        await expect(dropped).resolves.toEqual({ spoken: false, reason: "dropped" });
    });

    it("lets an urgent line cut in, cancelling what plays and what waits", async () => {
        const system = fake("system_enhanced");
        registry.registerProvider(system);
        const playing = registry.speak("long status");
        const waiting = registry.speak("later");
        await flush();
        const urgent = registry.speak("Stopped", { urgent: true });
        await expect(playing).resolves.toMatchObject({ spoken: false, reason: "cancelled" });
        await expect(waiting).resolves.toMatchObject({ spoken: false, reason: "cancelled" });
        await flush();
        expect(system.spoken.map((s) => s.text)).toEqual(["long status", "Stopped"]);
        system.finish();
        await expect(urgent).resolves.toMatchObject({ spoken: true });
    });

    it("cancel stops the speaking provider and forgets the queue", async () => {
        const system = fake("system_enhanced");
        registry.registerProvider(system);
        const playing = registry.speak("one");
        const waiting = registry.speak("two");
        await flush();
        registry.cancel();
        expect(system.cancelled).toBe(1);
        await expect(playing).resolves.toMatchObject({ spoken: false, reason: "cancelled" });
        await expect(waiting).resolves.toMatchObject({ spoken: false, reason: "cancelled" });
        expect(registry.speaking()).toBe(false);
        await flush();
        expect(system.spoken).toHaveLength(1);
    });

    it("cancel during a stalled attempt does not fall through to the next provider", async () => {
        const codex = fake("codex_realtime", "stalls");
        const system = fake("system_enhanced");
        registry.registerProvider(codex);
        registry.registerProvider(system);
        const done = registry.speak("hi");
        await flush();
        registry.cancel();
        await vi.advanceTimersByTimeAsync(2000);
        await expect(done).resolves.toMatchObject({ spoken: false, reason: "cancelled" });
        expect(system.spoken).toEqual([]);
    });

    it("fills in the saved voice and rate unless the caller passes its own", async () => {
        const system = fake("system_enhanced");
        registry.registerProvider(system);
        registry.configure({ voice: "com.apple.voice.premium.en-US.Zoe", rate: 1.1 });
        registry.speak("a");
        await flush();
        system.finish();
        await flush();
        registry.speak("b", { rate: 0.9 });
        await flush();
        expect(system.spoken[0].opts).toMatchObject({ voice: "com.apple.voice.premium.en-US.Zoe", rate: 1.1 });
        expect(system.spoken[1].opts).toMatchObject({ voice: "com.apple.voice.premium.en-US.Zoe", rate: 0.9 });
    });

    it("puts a chosen provider first and remembers the choice", () => {
        registry.registerProvider(fake("codex_realtime"));
        registry.registerProvider(fake("system_enhanced"));
        registry.registerProvider(fake("webview_basic"));
        registry.configure({ preferred: "system_enhanced" });
        expect(registry.order()).toEqual(["system_enhanced", "codex_realtime", "webview_basic"]);
        const reopened = createSpeechRegistry({ storage });
        reopened.registerProvider(fake("codex_realtime"));
        reopened.registerProvider(fake("system_enhanced"));
        expect(reopened.settings().preferred).toBe("system_enhanced");
        expect(reopened.order()).toEqual(["system_enhanced", "codex_realtime"]);
    });

    it("ignores unreadable saved settings", () => {
        const broken = createSpeechRegistry({ storage: memoryStorage({ "fndr.voice.output": "{nope" }) });
        expect(broken.settings()).toEqual({ preferred: "auto" });
    });

    it("tells listeners when the voice in use changes, and only then", async () => {
        const codex = fake("codex_realtime");
        const system = fake("system_enhanced");
        registry.registerProvider(codex);
        registry.registerProvider(system);
        const changes: [SpeechProviderId, string][] = [];
        const stop = registry.onProviderChange((id, reason) => changes.push([id, reason]));
        registry.speak("a");
        await flush();
        codex.finish();
        await flush();
        registry.speak("b");
        await flush();
        codex.finish();
        await flush();
        expect(changes).toEqual([["codex_realtime", "preferred"]]);
        expect(registry.activeProvider()).toBe("codex_realtime");
        stop();
        registry.configure({ preferred: "system_enhanced" });
        registry.speak("c");
        await flush();
        expect(changes).toHaveLength(1);
    });

    it("installs the Mac and basic voices once, leaving room for the ChatGPT plan voice", () => {
        const codex = fake("codex_realtime");
        registry.registerProvider(codex);
        installOfflineVoices(registry);
        installOfflineVoices(registry);
        expect(registry.order()).toEqual(["codex_realtime", "system_enhanced", "webview_basic"]);
        expect(registry.providers()[0]).toBe(codex);
    });
});
