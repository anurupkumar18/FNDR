/**
 * Picks who speaks. Providers are tried in preference order; one that is
 * unavailable, fails, or has not started speaking within the start timeout is
 * skipped and the next one speaks instead, so a slow first choice never leaves
 * dead air. A provider that failed is skipped for a cooldown. One utterance
 * plays at a time with a small queue; an urgent line cuts in.
 */

import type { SpeakOptions, SpeechProvider, SpeechProviderId } from "./speechProvider";

export const DEFAULT_ORDER: readonly SpeechProviderId[] = ["codex_realtime", "local_neural", "system_enhanced", "webview_basic"];

const STORAGE_KEY = "fndr.voice.output";
const MAX_QUEUED = 3;

export interface SpeechSettings {
    preferred: SpeechProviderId | "auto";
    voice?: string;
    rate?: number;
}

export type SpeakOutcome =
    | { spoken: true; provider: SpeechProviderId }
    | { spoken: false; reason: "cancelled" | "dropped" | "failed" | "no_provider"; detail?: string };

export type ProviderChangeListener = (id: SpeechProviderId, reason: string) => void;

export interface SpeechRegistry {
    registerProvider(provider: SpeechProvider): void;
    /** Never rejects: the outcome says whether and by whom the line was spoken. */
    speak(text: string, opts?: SpeakOptions): Promise<SpeakOutcome>;
    cancel(): void;
    speaking(): boolean;
    onProviderChange(listener: ProviderChangeListener): () => void;
    /** Registered providers in the order they will be tried. */
    order(): SpeechProviderId[];
    providers(): SpeechProvider[];
    activeProvider(): SpeechProviderId | null;
    settings(): SpeechSettings;
    configure(patch: Partial<SpeechSettings>): void;
}

interface KeyValueStore {
    getItem(key: string): string | null;
    setItem(key: string, value: string): void;
}

export interface SpeechRegistryDeps {
    storage?: KeyValueStore | null;
    startTimeoutMs?: number;
    cooldownMs?: number;
    now?: () => number;
}

interface Pending {
    text: string;
    opts: SpeakOptions;
    settle(outcome: SpeakOutcome): void;
}

type Attempt = "spoken" | "cancelled" | { failed: string; cool: boolean };

const STALLED = Symbol("stalled");

function browserStorage(): KeyValueStore | null {
    try {
        return typeof window === "undefined" ? null : window.localStorage;
    } catch {
        return null;
    }
}

function isProviderId(value: unknown): value is SpeechProviderId {
    return typeof value === "string" && (DEFAULT_ORDER as readonly string[]).includes(value);
}

function readSettings(storage: KeyValueStore | null): SpeechSettings {
    try {
        const raw = storage?.getItem(STORAGE_KEY);
        if (!raw) return { preferred: "auto" };
        const parsed = JSON.parse(raw) as Record<string, unknown>;
        const settings: SpeechSettings = { preferred: isProviderId(parsed.preferred) ? parsed.preferred : "auto" };
        if (typeof parsed.voice === "string" && parsed.voice) settings.voice = parsed.voice;
        if (typeof parsed.rate === "number" && Number.isFinite(parsed.rate)) settings.rate = parsed.rate;
        return settings;
    } catch {
        return { preferred: "auto" };
    }
}

function withDeadline<T>(work: Promise<T>, ms: number): Promise<T | typeof STALLED> {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const deadline = new Promise<typeof STALLED>((resolve) => {
        timer = setTimeout(() => resolve(STALLED), Math.max(0, ms));
    });
    return Promise.race([work, deadline]).finally(() => clearTimeout(timer));
}

function message(err: unknown): string {
    return err instanceof Error ? err.message : String(err);
}

export function createSpeechRegistry(deps: SpeechRegistryDeps = {}): SpeechRegistry {
    const storage = deps.storage === undefined ? browserStorage() : deps.storage;
    const startTimeoutMs = deps.startTimeoutMs ?? 1500;
    const cooldownMs = deps.cooldownMs ?? 60_000;
    const now = deps.now ?? (() => Date.now());

    const registered = new Map<SpeechProviderId, SpeechProvider>();
    const coolUntil = new Map<SpeechProviderId, number>();
    const listeners = new Set<ProviderChangeListener>();
    let settings = readSettings(storage);
    let queue: Pending[] = [];
    let current: { item: Pending; provider: SpeechProvider | null } | null = null;
    let generation = 0;
    let lastProvider: SpeechProviderId | null = null;

    const order = (): SpeechProviderId[] => {
        const ids = DEFAULT_ORDER.filter((id) => registered.has(id));
        const preferred = settings.preferred;
        if (preferred === "auto" || !registered.has(preferred)) return ids;
        return [preferred, ...ids.filter((id) => id !== preferred)];
    };

    const announce = (id: SpeechProviderId, reason: string) => {
        if (id === lastProvider) return;
        lastProvider = id;
        listeners.forEach((listener) => listener(id, reason));
    };

    const tryProvider = async (provider: SpeechProvider, item: Pending, mine: number): Promise<Attempt> => {
        const startedAt = now();
        const remaining = () => startTimeoutMs - (now() - startedAt);
        let availability: { ok: boolean; reason?: string } | typeof STALLED;
        try {
            availability = await withDeadline(provider.available(), remaining());
        } catch (err) {
            return { failed: message(err), cool: true };
        }
        if (mine !== generation) return "cancelled";
        if (availability === STALLED) return { failed: "did not answer in time", cool: true };
        if (!availability.ok) return { failed: availability.reason ?? "unavailable", cool: false };

        if (current) current.provider = provider;
        const speech = provider.speak(item.text, item.opts);
        let early: void | typeof STALLED;
        try {
            early = await withDeadline(speech, remaining());
        } catch (err) {
            return mine === generation ? { failed: message(err), cool: true } : "cancelled";
        }
        if (mine !== generation) return "cancelled";
        if (early !== STALLED) return "spoken";
        if (!provider.speaking()) {
            provider.cancel();
            speech.catch(() => undefined);
            return { failed: "did not start speaking in time", cool: true };
        }
        try {
            await speech;
        } catch (err) {
            if (mine === generation) coolUntil.set(provider.id, now() + cooldownMs);
            return mine === generation ? { failed: message(err), cool: false } : "cancelled";
        }
        return mine === generation ? "spoken" : "cancelled";
    };

    const run = async (item: Pending, mine: number) => {
        const skipped: string[] = [];
        const ids = order();
        for (const [index, id] of ids.entries()) {
            const provider = registered.get(id)!;
            const cooling = coolUntil.get(id);
            if (cooling !== undefined && cooling > now()) {
                skipped.push(`${provider.label}: cooling down after a failure`);
                continue;
            }
            const attempt = await tryProvider(provider, item, mine);
            if (mine !== generation || attempt === "cancelled") return;
            if (attempt === "spoken") {
                coolUntil.delete(id);
                announce(id, index === 0 ? "preferred" : skipped.join("; "));
                finish(item, { spoken: true, provider: id });
                return;
            }
            if (attempt.cool) coolUntil.set(id, now() + cooldownMs);
            skipped.push(`${provider.label}: ${attempt.failed}`);
        }
        finish(item, ids.length === 0 ? { spoken: false, reason: "no_provider" } : { spoken: false, reason: "failed", detail: skipped.join("; ") });
    };

    const finish = (item: Pending, outcome: SpeakOutcome) => {
        item.settle(outcome);
        if (current?.item === item) current = null;
        pump();
    };

    const pump = () => {
        if (current) return;
        const item = queue.shift();
        if (!item) return;
        current = { item, provider: null };
        void run(item, generation);
    };

    const cancelAll = () => {
        generation += 1;
        const active = current;
        const waiting = queue;
        current = null;
        queue = [];
        active?.provider?.cancel();
        active?.item.settle({ spoken: false, reason: "cancelled" });
        waiting.forEach((item) => item.settle({ spoken: false, reason: "cancelled" }));
    };

    return {
        registerProvider(provider) {
            registered.set(provider.id, provider);
        },
        speak(text, opts = {}) {
            return new Promise<SpeakOutcome>((resolve) => {
                let settled = false;
                const item: Pending = {
                    text,
                    opts: { voice: settings.voice, rate: settings.rate, ...opts },
                    settle(outcome) {
                        if (settled) return;
                        settled = true;
                        resolve(outcome);
                    },
                };
                if (opts.urgent) cancelAll();
                queue.push(item);
                while (queue.length > MAX_QUEUED) queue.shift()!.settle({ spoken: false, reason: "dropped" });
                pump();
            });
        },
        cancel: cancelAll,
        speaking() {
            return current !== null;
        },
        onProviderChange(listener) {
            listeners.add(listener);
            return () => listeners.delete(listener);
        },
        order,
        providers() {
            return order().map((id) => registered.get(id)!);
        },
        activeProvider() {
            return lastProvider;
        },
        settings() {
            return { ...settings };
        },
        configure(patch) {
            settings = { ...settings, ...patch };
            try {
                storage?.setItem(STORAGE_KEY, JSON.stringify(settings));
            } catch {
                // The choice still holds for this window.
            }
        },
    };
}

/** The app's one registry; every window that speaks registers into its own copy. */
export const speechRegistry = createSpeechRegistry();
