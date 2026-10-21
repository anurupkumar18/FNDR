import { describe, expect, it, vi } from "vitest";
import type { VoiceOutStatus } from "../ipc/tauri";
import { createRealtimeOut, type AudioOutLike, type ChannelLike, type PeerLike, type VoiceOutIpc } from "./realtimeOut";

const ready: VoiceOutStatus = { state: "ready", detail: null, usedPercent: 12, fallbackAbovePercent: 95, connected: false };

class FakeChannel implements ChannelLike {
    readyState = "connecting";
    onmessage: ((event: { data: unknown }) => void) | null = null;
    onopen: (() => void) | null = null;
    open() {
        this.readyState = "open";
        this.onopen?.();
    }
    emit(type: string) {
        this.onmessage?.({ data: JSON.stringify({ type }) });
    }
    close() {
        this.readyState = "closed";
    }
}

class FakePeer implements PeerLike {
    connectionState = "new";
    onconnectionstatechange: (() => void) | null = null;
    ontrack: PeerLike["ontrack"] = null;
    tracks: MediaStreamTrack[] = [];
    channel = new FakeChannel();
    remote = "";
    closed = false;
    addTrack(track: MediaStreamTrack) {
        this.tracks.push(track);
    }
    createDataChannel() {
        return this.channel;
    }
    async createOffer() {
        return { sdp: "v=0 offer" };
    }
    async setLocalDescription() {}
    async setRemoteDescription(description: { sdp: string }) {
        this.remote = description.sdp;
        queueMicrotask(() => this.channel.open());
    }
    close() {
        this.closed = true;
    }
    drop() {
        this.connectionState = "failed";
        this.onconnectionstatechange?.();
    }
}

function harness(overrides: Partial<VoiceOutIpc> = {}) {
    const peers: FakePeer[] = [];
    const audio: AudioOutLike = { srcObject: null, muted: false, play: async () => {}, pause: () => {} };
    const ducks: boolean[] = [];
    const ipc: VoiceOutIpc = {
        status: vi.fn(async () => ready),
        start: vi.fn(async () => "v=0 answer"),
        speak: vi.fn(async () => {}),
        cancel: vi.fn(async () => {}),
        stop: vi.fn(async () => {}),
        ...overrides,
    };
    const silentTrack = { kind: "audio", label: "silence" } as unknown as MediaStreamTrack;
    const provider = createRealtimeOut({
        ipc,
        createPeer: () => {
            const peer = new FakePeer();
            peers.push(peer);
            return peer;
        },
        createSilentSource: () => ({ track: silentTrack, stream: {} as MediaStream, stop: () => {} }),
        createAudio: () => audio,
        onDuck: (on) => ducks.push(on),
    });
    return { provider, ipc, peers, audio, ducks, silentTrack };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("ChatGPT realtime voice provider", () => {
    it("speaks a line through the broker and finishes on turn.done", async () => {
        const { provider, ipc, peers, ducks } = harness();
        const done = provider.speak("Your briefing is ready.", { voice: "marin" });
        await tick();
        await tick();
        expect(ipc.start).toHaveBeenCalledWith("v=0 offer", "marin");
        expect(peers[0].remote).toBe("v=0 answer");
        expect(ipc.speak).toHaveBeenCalledWith("Your briefing is ready.");

        peers[0].channel.emit("turn.created");
        expect(provider.speaking()).toBe(true);
        peers[0].channel.emit("turn.done");
        await expect(done).resolves.toBeUndefined();
        expect(provider.speaking()).toBe(false);
        expect(ducks).toEqual([true, false]);
    });

    it("never opens the microphone: the only track sent is silence", async () => {
        const getUserMedia = vi.fn();
        Object.defineProperty(navigator, "mediaDevices", { value: { getUserMedia }, configurable: true });
        const { provider, peers, silentTrack } = harness();
        void provider.speak("Hello.");
        await tick();
        await tick();
        expect(peers[0].tracks).toEqual([silentTrack]);
        expect(getUserMedia).not.toHaveBeenCalled();
    });

    it("cancel mutes and settles the line well within 150 ms", async () => {
        const { provider, peers, audio } = harness();
        const done = provider.speak("A long line that will be cut off.");
        await tick();
        await tick();
        peers[0].channel.emit("turn.created");

        const started = performance.now();
        provider.cancel();
        await done;
        expect(performance.now() - started).toBeLessThan(150);
        expect(audio.muted).toBe(true);
        expect(provider.speaking()).toBe(false);
    });

    it("ends a cancelled turn still being made before the next line, then reconnects", async () => {
        const { provider, ipc, peers, audio } = harness();
        void provider.speak("First.");
        await tick();
        await tick();
        peers[0].channel.emit("turn.created");
        provider.cancel();

        void provider.speak("Second.");
        await tick();
        await tick();
        await tick();
        expect(ipc.cancel).toHaveBeenCalledTimes(1);
        expect(peers).toHaveLength(2);
        expect(peers[0].closed).toBe(true);
        expect(ipc.speak).toHaveBeenLastCalledWith("Second.");
        expect(audio.muted).toBe(false);
    });

    it("reuses the connection when the cancelled turn already ended", async () => {
        const { provider, ipc, peers } = harness();
        void provider.speak("First.");
        await tick();
        await tick();
        peers[0].channel.emit("turn.created");
        provider.cancel();
        peers[0].channel.emit("turn.done");

        void provider.speak("Second.");
        await tick();
        expect(ipc.cancel).not.toHaveBeenCalled();
        expect(peers).toHaveLength(1);
    });

    it("says why it is unavailable, from the broker's status", async () => {
        for (const [state, reason] of [
            ["signed_out", "not signed in with ChatGPT"],
            ["not_installed", "Codex is not installed"],
            ["near_limit", "the ChatGPT plan is near its usage limit"],
            ["unsupported", "this Codex has no realtime voice"],
            ["private_mode", "Private Mode is on"],
        ] as const) {
            const { provider } = harness({ status: async () => ({ ...ready, state }) });
            await expect(provider.available()).resolves.toEqual({ ok: false, reason });
        }
        const { provider } = harness();
        await expect(provider.available()).resolves.toEqual({ ok: true });
    });

    it("a dropped connection rejects the line and reports unavailable so the registry falls back", async () => {
        const { provider, peers } = harness();
        const done = provider.speak("Hello.");
        await tick();
        await tick();
        peers[0].drop();
        await expect(done).rejects.toThrow("dropped");
        await expect(provider.available()).resolves.toEqual({ ok: false, reason: "network" });
    });

    it("a failed connection rejects the line and makes the provider unavailable", async () => {
        const { provider } = harness({ start: async () => Promise.reject(new Error("Sign in with ChatGPT to use its voice.")) });
        await expect(provider.speak("Hello.")).rejects.toThrow("Sign in");
        const availability = await provider.available();
        expect(availability.ok).toBe(false);
    });

    it("retries once on a fresh connection when the broker lost the session", async () => {
        let calls = 0;
        const { provider, ipc, peers } = harness({
            speak: vi.fn(async () => {
                calls += 1;
                if (calls === 1) throw new Error("The ChatGPT voice is not connected.");
            }),
        });
        void provider.speak("Hello.");
        for (let i = 0; i < 6; i += 1) await tick();
        expect(ipc.speak).toHaveBeenCalledTimes(2);
        expect(peers).toHaveLength(2);
    });
});
