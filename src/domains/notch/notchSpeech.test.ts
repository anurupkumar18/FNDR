import { describe, expect, it, vi } from "vitest";
import type { SpeechProvider, SpeechProviderId } from "@/shared/voice/speechProvider";
import { createSpeechRegistry } from "@/shared/voice/speechRegistry";
import { createNotchVoice, installNotchVoices, loadNotchMuted, providerLine, splitLine, usageWarning } from "./notchSpeech";

interface FakeProvider extends SpeechProvider {
    spoken: string[];
    finish(): void;
}

function fake(id: SpeechProviderId, available = true): FakeProvider {
    let done: (() => void) | null = null;
    let active = false;
    const provider: FakeProvider = {
        id,
        label: id,
        spoken: [],
        available: () => Promise.resolve(available ? { ok: true } : { ok: false, reason: `${id} is off` }),
        speak(text) {
            provider.spoken.push(text);
            active = true;
            return new Promise<void>((resolve) => {
                done = () => {
                    active = false;
                    resolve();
                };
            });
        },
        cancel() {
            const end = done;
            done = null;
            active = false;
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

const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function setup(realtimeAvailable = false) {
    const registry = createSpeechRegistry({ storage: null, startTimeoutMs: 200 });
    const realtime = fake("codex_realtime", realtimeAvailable);
    const mac = fake("system_enhanced");
    registry.registerProvider(realtime);
    registry.registerProvider(mac);
    let clock = 0;
    const voice = createNotchVoice(registry, () => clock);
    return { registry, realtime, mac, voice, tick: (ms: number) => (clock += ms) };
}

describe("splitLine", () => {
    it("keeps a short line whole and splits a long one at sentence ends, each at most 140 characters", () => {
        expect(splitLine("Step one of two: Open Spotify.")).toEqual(["Step one of two: Open Spotify."]);
        const long = `${"Done. ".repeat(10)}${"word ".repeat(60)}`;
        const parts = splitLine(long);
        expect(parts.length).toBeGreaterThan(1);
        for (const part of parts) expect(part.length).toBeLessThanOrEqual(140);
        expect(parts.join(" ").replace(/\s+/g, " ")).toBe(long.trim().replace(/\s+/g, " "));
    });
});

describe("createNotchVoice", () => {
    it("speaks with the ChatGPT voice when it is ready, and falls back to the Mac voice when it is not", async () => {
        const ready = setup(true);
        void ready.voice.say({ text: "Step one of two: Open Spotify.", urgent: false });
        await vi.waitFor(() => expect(ready.realtime.spoken).toEqual(["Step one of two: Open Spotify."]));

        const off = setup(false);
        void off.voice.say({ text: "Step one of two: Open Spotify.", urgent: false });
        await vi.waitFor(() => expect(off.mac.spoken).toEqual(["Step one of two: Open Spotify."]));
        expect(off.realtime.spoken).toEqual([]);
    });

    it("says only the speakable form of a line", async () => {
        const { mac, voice } = setup();
        void voice.say({ text: "Opened https://canvas.utah.edu/courses/1 and pressed Stop.", urgent: false });
        await vi.waitFor(() => expect(mac.spoken).toEqual(["Opened a link on canvas.utah.edu and pressed End."]));
    });

    it("lets a newer progress line replace a waiting one, and an urgent line cut in", async () => {
        const { mac, voice } = setup();
        const first = voice.say({ text: "Step one of three: Open Notes.", urgent: false });
        await vi.waitFor(() => expect(mac.spoken).toHaveLength(1));
        void voice.say({ text: "Step two of three: Type.", urgent: false });
        void voice.say({ text: "Step three of three: Save.", urgent: false });
        mac.finish();
        await first;
        await vi.waitFor(() => expect(mac.spoken).toEqual(["Step one of three: Open Notes.", "Step three of three: Save."]));

        void voice.say({ text: "I need your okay to click Buy. Tap Allow.", urgent: true });
        await vi.waitFor(() => expect(mac.spoken[mac.spoken.length - 1]).toBe("I need your okay to click Buy. Tap Allow."));
    });

    it("goes quiet at once on cancel and remembers what it said for the echo guard", async () => {
        const { mac, voice, tick } = setup();
        const changes: boolean[] = [];
        voice.onSpeaking((on) => changes.push(on));
        const line = voice.say({ text: "Understood: open Spotify. two steps. Starting.", urgent: false });
        await vi.waitFor(() => expect(mac.spoken).toHaveLength(1));
        expect(voice.speaking()).toBe(true);
        expect(voice.echoText()).toContain("open Spotify");
        voice.cancel();
        await line;
        expect(voice.speaking()).toBe(false);
        expect(mac.speaking()).toBe(false);
        expect(changes).toEqual([true, false]);
        expect(voice.echoText()).toBe("");

        const again = voice.say({ text: "Step one.", urgent: false });
        await vi.waitFor(() => expect(mac.spoken).toHaveLength(2));
        mac.finish();
        await again;
        await flush();
        tick(1000);
        expect(voice.echoText()).toBe("Step one.");
        tick(1500);
        expect(voice.echoText()).toBe("");
    });
});

describe("installNotchVoices", () => {
    it("registers the plan voice once, ahead of the offline voices", () => {
        const registry = createSpeechRegistry({ storage: null });
        const realtime = { ...fake("codex_realtime"), voices: async () => [], close: vi.fn(async () => undefined) };
        const create = vi.fn(() => realtime);
        expect(installNotchVoices(registry, create)).toBe(realtime);
        expect(installNotchVoices(registry, create)).toBe(realtime);
        expect(create).toHaveBeenCalledTimes(1);
        expect(registry.order()[0]).toBe("codex_realtime");
        expect(registry.has("webview_basic")).toBe(true);
    });
});

describe("provider status", () => {
    it("names who is speaking", () => {
        expect(providerLine("codex_realtime", "preferred")).toBe("Speaking with your ChatGPT plan");
        expect(providerLine("system_enhanced", "ChatGPT voice: not signed in with ChatGPT")).toBe(
            "On-device voice (ChatGPT voice: not signed in with ChatGPT)",
        );
        expect(providerLine("webview_basic", "preferred")).toBe("On-device voice");
    });

    it("warns as the plan nears its limit and says when FNDR has moved to the Mac voice", () => {
        const status = (state: "ready" | "near_limit" | "over_limit", usedPercent: number | null) => ({
            state,
            detail: null,
            usedPercent,
            fallbackAbovePercent: 95,
            connected: false,
        });
        expect(usageWarning(status("ready", 40))).toBeNull();
        expect(usageWarning(status("ready", 85))).toBe("85% of your ChatGPT plan's limit is used");
        expect(usageWarning(status("near_limit", 96))).toBe("ChatGPT plan nearly used up: speaking on this Mac");
        expect(usageWarning(status("over_limit", 100))).toBe("ChatGPT plan used up: speaking on this Mac");
        expect(usageWarning(status("ready", null))).toBeNull();
    });
});

describe("loadNotchMuted", () => {
    function storage(initial: Record<string, string> = {}) {
        const values = new Map(Object.entries(initial));
        return {
            getItem: (key: string) => values.get(key) ?? null,
            removeItem: (key: string) => void values.delete(key),
            values,
        };
    }

    it("reads the persisted mute and drops the old per-window flag", async () => {
        const store = storage({ "fndr.notch.do.muted": "1" });
        const set = vi.fn();
        await expect(loadNotchMuted({ get: async () => false, set, storage: store })).resolves.toBe(false);
        expect(set).not.toHaveBeenCalled();
        expect(store.values.has("fndr.notch.do.muted")).toBe(false);
    });

    it("migrates the old flag once when nothing is persisted yet", async () => {
        const store = storage({ "fndr.notch.do.muted": "1" });
        const set = vi.fn(async (muted: boolean) => muted);
        await expect(loadNotchMuted({ get: async () => null, set, storage: store })).resolves.toBe(true);
        expect(set).toHaveBeenCalledWith(true);
        expect(store.values.has("fndr.notch.do.muted")).toBe(false);
        await expect(loadNotchMuted({ get: async () => null, set, storage: storage() })).resolves.toBe(false);
    });

    it("falls back to the old flag when the setting cannot be read", async () => {
        const store = storage({ "fndr.notch.do.muted": "1" });
        await expect(loadNotchMuted({ get: () => Promise.reject(new Error("no backend")), set: vi.fn(), storage: store })).resolves.toBe(true);
    });
});
