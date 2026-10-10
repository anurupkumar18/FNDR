import { useCallback, useEffect, useId, useState } from "react";
import {
    getVoiceOutputSettings,
    setVoiceOutputSettings,
    ttsInstall,
    ttsRemove,
    ttsStatus,
    ttsVoices,
    type TtsStatus,
    type TtsVoices,
    type VoiceOutputSettings,
} from "@/shared/ipc/tauri";
import { createLocalNeuralProvider } from "./localNeural";
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

/** What the on-device neural voice row needs; tests pass fakes. */
export interface NeuralVoiceControls {
    status(): Promise<TtsStatus>;
    voices(): Promise<TtsVoices>;
    install(consent: boolean): Promise<TtsStatus>;
    remove(): Promise<TtsStatus>;
    preview(voiceId: string, rate: number): Promise<void>;
}

const NEURAL_PREVIEW_LINE = "Hi, this is how FNDR sounds when it speaks on this Mac.";
const POLL_MS = 500;

let previewProvider: ReturnType<typeof createLocalNeuralProvider> | null = null;

const defaultNeural: NeuralVoiceControls = {
    status: ttsStatus,
    voices: ttsVoices,
    install: ttsInstall,
    remove: ttsRemove,
    preview(voiceId, rate) {
        previewProvider ??= createLocalNeuralProvider();
        previewProvider.cancel();
        return previewProvider.speak(NEURAL_PREVIEW_LINE, { voice: voiceId, rate });
    },
};

function megabytes(bytes: number): string {
    return `${Math.round(bytes / 1_000_000)} MB`;
}

const ACCENT_NAMES = { us: "American", gb: "British" } as const;

function NeuralVoiceRow({
    neural,
    voice,
    rate,
    onVoice,
    onInstalled,
}: {
    neural: NeuralVoiceControls;
    voice: string;
    rate: number;
    onVoice: (voice: string) => void;
    onInstalled: () => void;
}) {
    const ids = useId();
    const [status, setStatus] = useState<TtsStatus | null>(null);
    const [voices, setVoices] = useState<TtsVoices | null>(null);
    const [asking, setAsking] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        let live = true;
        void neural
            .status()
            .then((next) => live && setStatus(next))
            .catch((reason) => live && setError(errorText(reason)));
        void neural
            .voices()
            .then((next) => live && setVoices(next))
            .catch(() => undefined);
        return () => {
            live = false;
        };
    }, [neural]);

    const downloading = status?.state === "downloading";
    useEffect(() => {
        if (!downloading) return;
        const timer = setInterval(() => {
            void neural
                .status()
                .then((next) => {
                    setStatus(next);
                    if (next.state === "ready") onInstalled();
                })
                .catch((reason) => setError(errorText(reason)));
        }, POLL_MS);
        return () => clearInterval(timer);
    }, [downloading, neural, onInstalled]);

    const run = (work: Promise<TtsStatus>) => {
        setError(null);
        void work.then(setStatus).catch((reason) => setError(errorText(reason)));
    };

    const size = status ? megabytes(status.totalBytes) : "";
    const chosen = voice || voices?.defaultVoice || "";
    let body: JSX.Element | null = null;
    if (!status) {
        body = <small>Checking…</small>;
    } else if (status.state === "ready") {
        body = (
            <span className="fndr-voice-output__controls">
                <select
                    id={`${ids}-neural`}
                    className="fndr-voice-output__select"
                    aria-label="On-device voice"
                    value={chosen}
                    disabled={!voices}
                    onChange={(event) => onVoice(event.target.value)}
                >
                    {voices?.voices.map((option) => (
                        <option key={option.id} value={option.id}>
                            {`${option.name}, ${ACCENT_NAMES[option.accent]} ${option.gender}`}
                        </option>
                    ))}
                </select>
                <button
                    type="button"
                    className="ui-action-btn"
                    disabled={!chosen}
                    onClick={() => void neural.preview(chosen, rate).catch((reason) => setError(errorText(reason)))}
                >
                    Preview
                </button>
                <button type="button" className="ui-action-btn" onClick={() => run(neural.remove())}>
                    Remove ({size})
                </button>
            </span>
        );
    } else if (downloading) {
        const percent = status.totalBytes ? Math.floor((status.downloadedBytes / status.totalBytes) * 100) : 0;
        body = (
            <span className="fndr-voice-output__controls">
                <progress max={100} value={percent} aria-label="On-device voice download" />
                <small aria-live="polite">{`${percent}% of ${size}`}</small>
            </span>
        );
    } else if (asking) {
        body = (
            <span className="fndr-voice-output__controls">
                <button
                    type="button"
                    className="ui-action-btn"
                    onClick={() => {
                        setAsking(false);
                        run(neural.install(true));
                    }}
                >
                    Download {size}
                </button>
                <button type="button" className="ui-action-btn" onClick={() => setAsking(false)}>
                    Not now
                </button>
            </span>
        );
    } else {
        body = (
            <span className="fndr-voice-output__controls">
                <button type="button" className="ui-action-btn" onClick={() => setAsking(true)}>
                    {status.state === "error" ? "Try again" : `Install (${size})`}
                </button>
            </span>
        );
    }

    return (
        <>
            <div className="sg-setting-row">
                <span>
                    <strong>
                        <label htmlFor={`${ids}-neural`}>On-device neural voice</label>
                    </strong>
                    <small>Kokoro, a natural voice that runs fully on this Mac. Nothing it says leaves the Mac.</small>
                    {asking && (
                        <small role="note">
                            {`FNDR downloads the Kokoro model and its voices (${size}) from Hugging Face, and pronunciation dictionaries from GitHub, once. Each file is checked against a pinned checksum. After that no network is used to speak.`}
                        </small>
                    )}
                    {status?.state === "error" && status.detail && <small>{status.detail}</small>}
                </span>
                {body}
            </div>
            {error && (
                <p className="fndr-voice-output__error" role="alert">
                    {error}
                </p>
            )}
        </>
    );
}

export interface VoiceOutputSectionProps {
    registry?: SpeechRegistry;
    listVoices?: () => Promise<RankedVoice[]>;
    preview?: (voiceId: string, rate: number) => Promise<void>;
    neural?: NeuralVoiceControls;
}

/** Settings > Voice: which voice FNDR speaks with, a preview, and the speed. */
export function VoiceOutputSection({
    registry = speechRegistry,
    listVoices = () => createSystemEnhancedProvider().voices(),
    preview = (voiceId, rate) => previewVoice(voiceId, { rate }),
    neural = defaultNeural,
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
            {registered.includes("local_neural") && (
                <NeuralVoiceRow
                    neural={neural}
                    voice={settings?.neural_voice ?? ""}
                    rate={settings?.rate ?? 1}
                    onVoice={(neural_voice) => save({ neural_voice })}
                    onInstalled={refreshStatus}
                />
            )}
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
