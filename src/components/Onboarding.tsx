import { useState, useEffect, useCallback } from "react";
import {
    OnboardingState,
    OnboardingStep,
    ModelInfo,
    DownloadProgress,
    getOnboardingState,
    saveOnboardingState,
    requestBiometricAuth,
    checkPermissions,
    openSystemSettings,
    listAvailableModels,
    downloadModel,
    onDownloadProgress,
} from "../api/onboarding";
import { getStatus } from "../api/tauri";
import "./Onboarding.css";

// ── Helper: step index for progress dots ─────────────────────────────────
const STEPS: OnboardingStep[] = [
    "welcome",
    "biometrics",
    "privacy_promise",
    "permissions",
    "model_download",
    "indexing_started",
];

function stepIndex(s: OnboardingStep) {
    return STEPS.indexOf(s);
}

// ── StepDots ──────────────────────────────────────────────────────────────
function StepDots({ current }: { current: OnboardingStep }) {
    const ci = stepIndex(current);
    return (
        <div className="ob-step-dots">
            {STEPS.map((s, i) => (
                <div
                    key={s}
                    className={`ob-step-dot ${i === ci ? "active" : i < ci ? "done" : ""}`}
                />
            ))}
        </div>
    );
}

// ── Step 1: Welcome ───────────────────────────────────────────────────────
function StepWelcome({ onNext }: { onNext: () => void }) {
    return (
        <>
            <span className="ob-icon">⌘</span>
            <h1 className="ob-title">Your memory, on your Mac.</h1>
            <p className="ob-subtitle">
                FNDR remembers what you've worked on so you don't have to.
                Search across apps, documents, and conversations — instantly.
                <br /><br />
                Everything runs on your computer. Nothing leaves it. Ever.
            </p>
            <button id="ob-get-started" className="ob-btn-primary" onClick={onNext}>
                Get Started
            </button>
        </>
    );
}

// ── Step 2: Biometrics ────────────────────────────────────────────────────
function StepBiometrics({ state, onSave }: { state: OnboardingState; onSave: (s: OnboardingState) => void }) {
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    async function handleEnable() {
        setLoading(true);
        setError(null);
        try {
            const ok = await requestBiometricAuth("Unlock FNDR — your private screen history");
            if (ok) {
                const next = { ...state, step: "privacy_promise" as OnboardingStep, biometric_enabled: true };
                onSave(next);
            } else {
                setError("Authentication failed. Please try again.");
            }
        } catch {
            setError("Touch ID is not available. We'll use your Mac login password.");
        }
        setLoading(false);
    }

    function handleSkip() {
        onSave({ ...state, step: "privacy_promise", biometric_enabled: false });
    }

    return (
        <>
            <span className="ob-icon">🔐</span>
            <h1 className="ob-title">Lock FNDR with Touch ID</h1>
            <p className="ob-subtitle">
                FNDR stores everything you see on screen.
                Before we start, let's make sure only you can open it.
            </p>
            {error && <div className="ob-error-box">{error}</div>}
            <button id="ob-enable-touchid" className="ob-btn-primary" onClick={handleEnable} disabled={loading}>
                {loading ? "Authenticating…" : "Enable Touch ID Lock"}
            </button>
            <button className="ob-btn-ghost" onClick={handleSkip}>
                Use Mac password instead
            </button>
        </>
    );
}

// ── Step 3: Privacy Promise ───────────────────────────────────────────────
function StepPrivacyPromise({ state, onSave }: { state: OnboardingState; onSave: (s: OnboardingState) => void }) {
    return (
        <>
            <span className="ob-icon">🔒</span>
            <h1 className="ob-title">What FNDR sees (and doesn't share)</h1>
            <div className="ob-privacy-list">
                {[
                    {
                        icon: "✅",
                        title: "What FNDR stores",
                        body: "Text and a thumbnail of your active screen, every few seconds. This lives in a private folder on your Mac.",
                    },
                    {
                        icon: "🌐",
                        title: "Nothing leaves your Mac",
                        body: "No servers. No cloud. No company can read your memories — ever.",
                    },
                    {
                        icon: "🎭",
                        title: "Automatic privacy",
                        body: "Password managers and banking apps are automatically skipped.",
                    },
                    {
                        icon: "🗑",
                        title: "You're in control",
                        body: "Delete any memory anytime — or wipe everything in one tap.",
                    },
                ].map(({ icon, title, body }) => (
                    <div className="ob-privacy-item" key={title}>
                        <span className="ob-privacy-icon">{icon}</span>
                        <div className="ob-privacy-text">
                            <strong>{title}</strong>
                            <span>{body}</span>
                        </div>
                    </div>
                ))}
            </div>
            <button
                id="ob-accept-privacy"
                className="ob-btn-primary"
                onClick={() => onSave({ ...state, step: "permissions" })}
            >
                I'm in — Continue
            </button>
        </>
    );
}

// ── Step 4: Permissions ───────────────────────────────────────────────────
function StepPermissions({ state, onSave }: { state: OnboardingState; onSave: (s: OnboardingState) => void }) {
    const [perms, setPerms] = useState({ screen_recording: false, accessibility: false, microphone: false });

    const refresh = useCallback(async () => {
        try {
            const p = await checkPermissions();
            setPerms(p);
        } catch {/* ignore */}
    }, []);

    useEffect(() => {
        refresh();
        const id = setInterval(refresh, 2500);
        return () => clearInterval(id);
    }, [refresh]);

    async function openSettings(pane: Parameters<typeof openSystemSettings>[0]) {
        await openSystemSettings(pane);
    }

    function handleContinue() {
        onSave({
            ...state,
            step: "model_download",
            screen_permission: perms.screen_recording,
            accessibility_permission: perms.accessibility,
        });
    }

    const canContinue = perms.screen_recording;

    return (
        <>
            <span className="ob-icon">🛡️</span>
            <h1 className="ob-title">Grant a few permissions</h1>
            <p className="ob-subtitle">FNDR needs permission to see your screen. Everything stays local.</p>

            {[
                {
                    key: "screen_recording" as const,
                    icon: "🖥",
                    label: "Screen Recording",
                    desc: "Required — captures snapshots locally",
                    pane: "screen-recording" as const,
                },
                {
                    key: "accessibility" as const,
                    icon: "🔡",
                    label: "Accessibility",
                    desc: "Optional — reads window titles for better search",
                    pane: "accessibility" as const,
                },
                {
                    key: "microphone" as const,
                    icon: "🎙",
                    label: "Microphone",
                    desc: "Optional — for meeting transcription",
                    pane: "microphone" as const,
                },
            ].map(({ key, icon, label, desc, pane }) => (
                <div className={`ob-permission-row ${perms[key] ? "granted" : ""}`} key={key}>
                    <div className="ob-permission-left">
                        <span className="ob-permission-icon">{icon}</span>
                        <div>
                            <div className="ob-permission-label">{label}</div>
                            <div className="ob-permission-desc">{desc}</div>
                        </div>
                    </div>
                    {perms[key] ? (
                        <span className="ob-permission-badge">✅</span>
                    ) : (
                        <button
                            id={`ob-perm-${pane}`}
                            className="ob-permission-btn"
                            onClick={() => openSettings(pane)}
                        >
                            Grant
                        </button>
                    )}
                </div>
            ))}

            <button
                id="ob-continue-permissions"
                className="ob-btn-primary"
                style={{ marginTop: 20 }}
                onClick={handleContinue}
                disabled={!canContinue}
                title={canContinue ? undefined : "Screen Recording is required to continue"}
            >
                {canContinue ? "Continue" : "Grant Screen Recording to continue"}
            </button>
        </>
    );
}

// ── Step 5: Model Download ────────────────────────────────────────────────
function StepModelDownload({ state, onSave }: { state: OnboardingState; onSave: (s: OnboardingState) => void }) {
    const [models, setModels] = useState<ModelInfo[]>([]);
    const [selected, setSelected] = useState<ModelInfo | null>(null);
    const [progress, setProgress] = useState<DownloadProgress | null>(null);
    const [isDownloading, setIsDownloading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        listAvailableModels().then((ms) => {
            setModels(ms);
            const preferred = ms.find((m) => m.recommended) ?? ms[0];
            setSelected(preferred ?? null);
        });
    }, []);

    useEffect(() => {
        let unlisten: (() => void) | null = null;
        onDownloadProgress((p) => {
            setProgress(p);
            if (p.done && !p.error) {
                setIsDownloading(false);
                if (selected) {
                    onSave({ ...state, step: "indexing_started", model_downloaded: true, model_id: selected.id });
                }
            }
            if (p.error) {
                setError(p.error);
                setIsDownloading(false);
            }
        }).then((u) => { unlisten = u; });
        return () => { unlisten?.(); };
    }, [selected, state, onSave]);

    async function handleDownload() {
        if (!selected) return;
        if (selected.download_url === "already_downloaded") {
            onSave({ ...state, step: "indexing_started", model_downloaded: true, model_id: selected.id });
            return;
        }
        setError(null);
        setIsDownloading(true);
        try {
            await downloadModel(selected.id, selected.download_url, selected.filename);
        } catch (e: unknown) {
            setError(String(e));
            setIsDownloading(false);
        }
    }

    function handleSkip() {
        onSave({ ...state, step: "indexing_started", model_downloaded: false });
    }

    const alreadyDownloaded = selected?.download_url === "already_downloaded";

    function fmtBytes(b: number) {
        return b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${(b / 1e6).toFixed(0)} MB`;
    }

    return (
        <>
            <span className="ob-icon">🧠</span>
            <h1 className="ob-title">Choose your on-device AI</h1>
            <p className="ob-subtitle">Runs entirely on your Mac. No internet after download.</p>

            {!isDownloading && (
                <div className="ob-model-cards">
                    {models.map((m) => (
                        <button
                            key={m.id}
                            id={`ob-model-${m.id}`}
                            className={`ob-model-card ${selected?.id === m.id ? "selected" : ""} ${m.download_url === "already_downloaded" ? "already-downloaded" : ""}`}
                            onClick={() => setSelected(m)}
                        >
                            {m.recommended && <span className="ob-model-badge">Recommended</span>}
                            {m.download_url === "already_downloaded" && (
                                <span className="ob-model-badge downloaded">Downloaded</span>
                            )}
                            <div className="ob-model-name">{m.name}</div>
                            <div className="ob-model-desc">{m.description}</div>
                            <div className="ob-model-meta">
                                <span>💾 {m.size_label}</span>
                                <span>⚡ {m.speed_label}</span>
                                <span>🧠 ~{m.ram_gb} GB RAM</span>
                            </div>
                        </button>
                    ))}
                </div>
            )}

            {isDownloading && progress && (
                <div style={{ marginBottom: 24 }}>
                    <div className="ob-download-info">
                        <div className="ob-download-title">Downloading {selected?.name}…</div>
                        <div className="ob-download-subtitle">
                            {fmtBytes(progress.bytes_downloaded)} / {fmtBytes(progress.total_bytes)}
                        </div>
                    </div>
                    <div className="ob-progress-bar-wrap">
                        <div
                            className="ob-progress-bar-fill"
                            style={{ width: `${progress.percent.toFixed(1)}%` }}
                        />
                    </div>
                    <div className="ob-progress-label">{progress.percent.toFixed(0)}%</div>
                </div>
            )}

            {error && <div className="ob-error-box">{error}</div>}

            {!isDownloading && (
                <>
                    <button
                        id="ob-download-model"
                        className="ob-btn-primary"
                        onClick={handleDownload}
                        disabled={!selected}
                    >
                        {alreadyDownloaded
                            ? `Use ${selected?.name}`
                            : `Download ${selected?.name ?? ""} · ${selected?.size_label ?? ""}`}
                    </button>
                    <button className="ob-btn-ghost" onClick={handleSkip}>
                        Skip for now — start with search only
                    </button>
                </>
            )}
        </>
    );
}

// ── Step 6: Indexing Started ──────────────────────────────────────────────
function StepIndexingStarted({ state, onSave }: { state: OnboardingState; onSave: (s: OnboardingState) => void }) {
    const [memories, setMemories] = useState(0);
    const [apps] = useState(0);
    const [elapsed, setElapsed] = useState(0);

    useEffect(() => {
        const id = setInterval(async () => {
            setElapsed((e) => e + 1);
            try {
                const s = await getStatus();
                setMemories(s.frames_captured);
            } catch {/* ignore */}
        }, 1000);
        return () => clearInterval(id);
    }, []);

    function formatElapsed(secs: number) {
        if (secs < 60) return `${secs}s`;
        return `${Math.floor(secs / 60)}m ${secs % 60}s`;
    }

    return (
        <>
            <span className="ob-icon">✨</span>
            <h1 className="ob-title">FNDR is learning your screen</h1>
            <p className="ob-subtitle">
                <span className="ob-pulse-dot" />
                Keep using your Mac like normal. FNDR works quietly in the background.
            </p>

            <div className="ob-live-stats">
                <div className="ob-live-stat">
                    <span className="ob-live-stat-num">{memories}</span>
                    <span className="ob-live-stat-label">Memories</span>
                </div>
                <div className="ob-live-stat">
                    <span className="ob-live-stat-num">{apps}</span>
                    <span className="ob-live-stat-label">Apps seen</span>
                </div>
                <div className="ob-live-stat">
                    <span className="ob-live-stat-num">{formatElapsed(elapsed)}</span>
                    <span className="ob-live-stat-label">Tracking</span>
                </div>
            </div>

            <div className="ob-search-teaser">
                Try searching: "the article I was reading earlier" or "that Figma file"
            </div>

            <button
                id="ob-open-fndr"
                className="ob-btn-primary"
                onClick={() => onSave({ ...state, step: "complete" })}
            >
                Open FNDR →
            </button>
        </>
    );
}

// ── Root Onboarding Component ─────────────────────────────────────────────
interface OnboardingProps {
    onComplete: () => void;
}

export function Onboarding({ onComplete }: OnboardingProps) {
    const [state, setState] = useState<OnboardingState | null>(null);

    useEffect(() => {
        getOnboardingState().then(setState);
    }, []);

    const save = useCallback(
        async (next: OnboardingState) => {
            setState(next);
            await saveOnboardingState(next);
            if (next.step === "complete") {
                onComplete();
            }
        },
        [onComplete]
    );

    if (!state) return null;

    return (
        <div className="onboarding-overlay">
            <div className="ob-card">
                {state.step !== "welcome" && state.step !== "complete" && (
                    <StepDots current={state.step} />
                )}

                {state.step === "welcome" && <StepWelcome onNext={() => save({ ...state, step: "biometrics" })} />}
                {state.step === "biometrics" && <StepBiometrics state={state} onSave={save} />}
                {state.step === "privacy_promise" && <StepPrivacyPromise state={state} onSave={save} />}
                {state.step === "permissions" && <StepPermissions state={state} onSave={save} />}
                {state.step === "model_download" && <StepModelDownload state={state} onSave={save} />}
                {state.step === "indexing_started" && <StepIndexingStarted state={state} onSave={save} />}
            </div>
        </div>
    );
}
