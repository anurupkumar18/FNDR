import { useCallback, useEffect, useId, useState } from "react";
import { getVoiceOutputSettings, setVoiceOutputSettings, type VoiceOutputSettings } from "@/shared/ipc/tauri";
import type { SpeechProviderId } from "./speechProvider";
import { installOfflineVoices, speechRegistry, type SpeechRegistry } from "./speechRegistry";
import { createSystemEnhancedProvider, PREMIUM_VOICE_GUIDE, previewVoice, type RankedVoice, type VoiceQuality } from "./systemEnhanced";
import "./voice.css";

const PROVIDER_NAMES: Record<SpeechProviderId, string> = {
    codex_realtime: "ChatGPT plan voice",
    local_neural: "On-device neural voice",
    system_enhanced: "Mac voice",
    webview_basic: "Basic system voice",
};

const QUALITY_NAMES: Record<VoiceQuality, string> = {
    siri: "Siri",
    premium: "Premium",
    enhanced: "Enhanced",
    compact: "Compact",
};

const DEFAULT_SETTINGS: VoiceOutputSettings = { provider: "auto", system_voice: "", rate: 1 };

function errorText(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
}

async function describeVoice(registry: SpeechRegistry): Promise<string> {
    let codexReason: string | null = null;
    for (const provider of registry.providers()) {
        const result = await provider.available().catch((reason) => ({ ok: false, reason: errorText(reason) }));
        if (provider.id === "codex_realtime") {
            if (result.ok) return "Using your ChatGPT plan";
            codexReason = result.reason ?? "unavailable";
            continue;
        }
        if (result.ok) {
            if (codexReason) return `Using an on-device voice (${codexReason})`;
            return registry.settings().preferred === provider.id ? "Using an on-device voice (your choice)" : "Using an on-device voice";
        }
    }
    return "No voice is available in this window.";
}

export interface VoiceOutputSectionProps {
    registry?: SpeechRegistry;
    listVoices?: () => Promise<RankedVoice[]>;
    preview?: (voiceId: string, rate: number) => Promise<void>;
}

/** Settings > Voice: which voice FNDR speaks with, a preview, and the speed. */
export function VoiceOutputSection({
    registry = speechRegistry,
    listVoices = () => createSystemEnhancedProvider().voices(),
    preview = (voiceId, rate) => previewVoice(voiceId, { rate }),
}: VoiceOutputSectionProps) {
    const ids = useId();
    if (registry === speechRegistry) installOfflineVoices(registry);
    const [settings, setSettings] = useState<VoiceOutputSettings | null>(null);
    const [voices, setVoices] = useState<RankedVoice[]>([]);
    const [status, setStatus] = useState("Checking voices…");
    const [error, setError] = useState<string | null>(null);

    const apply = useCallback(
        (next: VoiceOutputSettings) => {
            registry.configure({
                preferred: next.provider === "auto" ? "auto" : (next.provider as SpeechProviderId),
                voice: next.system_voice || undefined,
                rate: next.rate,
            });
        },
        [registry],
    );

    const refreshStatus = useCallback(() => {
        void describeVoice(registry).then(setStatus);
    }, [registry]);

    useEffect(() => {
        let live = true;
        void getVoiceOutputSettings()
            .then((saved) => {
                if (!live) return;
                setSettings(saved);
                apply(saved);
                refreshStatus();
            })
            .catch((reason) => {
                if (!live) return;
                setSettings(DEFAULT_SETTINGS);
                setError(errorText(reason));
                refreshStatus();
            });
        void listVoices()
            .then((list) => live && setVoices(list))
            .catch(() => undefined);
        const stop = registry.onProviderChange(() => refreshStatus());
        return () => {
            live = false;
            stop();
        };
        // Load once per mount; the registry and loaders are fixed for a mount.
    }, []);

    const save = (patch: Partial<VoiceOutputSettings>) => {
        if (!settings) return;
        const next = { ...settings, ...patch };
        setSettings(next);
        apply(next);
        setError(null);
        void setVoiceOutputSettings(next)
            .then(() => refreshStatus())
            .catch((reason) => setError(errorText(reason)));
    };

    const registered = registry.order();
    const hasCodex = registered.includes("codex_realtime");
    const onlyCompact = voices.length > 0 && voices[0].quality === "compact";
    const chosenVoice = settings?.system_voice || voices[0]?.id || "";

    return (
        <section className="sg-settings-card fndr-voice-output" aria-labelledby={`${ids}-title`}>
            <div className="sg-setting-row">
                <span>
                    <strong id={`${ids}-title`}>Voice</strong>
                    <small aria-live="polite">{status}</small>
                    {!hasCodex && <small>The ChatGPT plan voice is not available in this version, so FNDR speaks with a voice on this Mac.</small>}
                </span>
                <select
                    className="fndr-voice-output__select"
                    aria-label="Speak with"
                    value={settings?.provider ?? "auto"}
                    disabled={!settings}
                    onChange={(event) => save({ provider: event.target.value })}
                >
                    <option value="auto">Best available</option>
                    {registered.map((id) => (
                        <option key={id} value={id}>
                            {PROVIDER_NAMES[id]}
                        </option>
                    ))}
                </select>
            </div>
            <div className="sg-setting-row">
                <span>
                    <strong>
                        <label htmlFor={`${ids}-voice`}>Mac voice</label>
                    </strong>
                    <small>{onlyCompact ? PREMIUM_VOICE_GUIDE : "Used whenever FNDR speaks with a voice on this Mac."}</small>
                </span>
                <span className="fndr-voice-output__controls">
                    <select
                        id={`${ids}-voice`}
                        className="fndr-voice-output__select"
                        value={settings?.system_voice ?? ""}
                        disabled={!settings || voices.length === 0}
                        onChange={(event) => save({ system_voice: event.target.value })}
                    >
                        <option value="">{voices[0] ? `Best installed (${voices[0].name})` : "Best installed"}</option>
                        {voices.map((voice) => (
                            <option key={voice.id} value={voice.id}>
                                {`${voice.name}, ${QUALITY_NAMES[voice.quality]}`}
                            </option>
                        ))}
                    </select>
                    <button
                        type="button"
                        className="ui-action-btn"
                        disabled={!chosenVoice}
                        onClick={() => void preview(chosenVoice, settings?.rate ?? 1).catch((reason) => setError(errorText(reason)))}
                    >
                        Preview voice
                    </button>
                </span>
            </div>
            <div className="sg-setting-row">
                <span>
                    <strong>
                        <label htmlFor={`${ids}-rate`}>Speed</label>
                    </strong>
                    <small>1.0 is the voice's natural pace.</small>
                </span>
                <span className="fndr-voice-output__controls">
                    <input
                        id={`${ids}-rate`}
                        type="range"
                        min={0.5}
                        max={2}
                        step={0.1}
                        value={settings?.rate ?? 1}
                        aria-valuetext={`${(settings?.rate ?? 1).toFixed(1)} times`}
                        disabled={!settings}
                        onChange={(event) => save({ rate: Number(event.target.value) })}
                    />
                    <span className="fndr-voice-output__rate" aria-hidden="true">
                        {(settings?.rate ?? 1).toFixed(1)}×
                    </span>
                </span>
            </div>
            {error && (
                <p className="fndr-voice-output__error" role="alert">
                    {error}
                </p>
            )}
        </section>
    );
}
