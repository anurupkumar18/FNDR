import { useCallback, useEffect, useState } from "react";

type Theme = "dark" | "light";
import {
    CaptureStatus,
    MemoryRepairProgress,
    MemoryRepairSummary,
    McpServerStatus,
    deleteAllData,
    deleteOlderThan,
    getBlocklist,
    getMemoryRepairProgress,
    getMcpServerStatus,
    getRetentionDays,
    pauseCapture,
    resumeCapture,
    setBlocklist,
    setRetentionDays,
    startMcpServer,
    stopMcpServer,
    runMemoryRepairBackfill,
} from "../api/tauri";
import {
    ModelInfo,
    OnboardingState,
    getOnboardingState,
    deleteAiModel,
    downloadModel,
    listAvailableModels,
    refreshAiModels,
    saveOnboardingState,
} from "../api/onboarding";
import { useModelDownloadStatus } from "../hooks/useModelDownloadStatus";
import "./ControlPanel.css";

interface ControlPanelProps {
    status: CaptureStatus | null;
    compact?: boolean;
    /** Hide MCP and emphasize core privacy when true (VITE_EVAL_UI build). */
    evalUi?: boolean;
}

type Tab = "settings" | "model" | "privacy";

export function ControlPanel({ status, compact = false, evalUi = false }: ControlPanelProps) {
    const [isOpen, setIsOpen] = useState(false);
    const [activeTab, setActiveTab] = useState<Tab>("settings");
    const [blocklist, setBlocklistState] = useState<string[]>([]);
    const [newApp, setNewApp] = useState("");
    const [confirmDelete, setConfirmDelete] = useState(false);
    const [retentionDays, setRetentionDaysState] = useState<number>(7);
    const [retentionBusy, setRetentionBusy] = useState(false);
    const [mcpStatus, setMcpStatus] = useState<McpServerStatus | null>(null);
    const [mcpBusy, setMcpBusy] = useState(false);
    const [copiedMcpLink, setCopiedMcpLink] = useState(false);
    const [profileName, setProfileName] = useState("");
    const [profileDraft, setProfileDraft] = useState("");
    const [profileBusy, setProfileBusy] = useState(false);
    const [profileMsg, setProfileMsg] = useState<string | null>(null);
    const [repairBusy, setRepairBusy] = useState(false);
    const [repairSummary, setRepairSummary] = useState<MemoryRepairSummary | null>(null);
    const [repairError, setRepairError] = useState<string | null>(null);
    const [repairProgress, setRepairProgress] = useState<MemoryRepairProgress | null>(null);

    // Theme state
    const [theme, setTheme] = useState<Theme>(() => {
        return (localStorage.getItem("fndr-theme") as Theme) || "dark";
    });

    // Model tab state
    const [models, setModels] = useState<ModelInfo[]>([]);
    const [modelsLoading, setModelsLoading] = useState(false);
    const [downloadingId, setDownloadingId] = useState<string | null>(null);
    const [modelError, setModelError] = useState<string | null>(null);
    const [confirmDeleteModel, setConfirmDeleteModel] = useState<string | null>(null);
    const [isActivatingModel, setIsActivatingModel] = useState(false);
    const downloadStatus = useModelDownloadStatus();

    const loadData = useCallback(async () => {
        try {
            if (evalUi) {
                const [bl, ret, onboarding] = await Promise.all([
                    getBlocklist(),
                    getRetentionDays(),
                    getOnboardingState(),
                ]);
                setBlocklistState(bl);
                setRetentionDaysState(ret);
                const name = onboarding.display_name ?? "";
                setProfileName(name);
                setProfileDraft(name);
            } else {
                const [bl, ret, mcp, onboarding] = await Promise.all([
                    getBlocklist(),
                    getRetentionDays(),
                    getMcpServerStatus(),
                    getOnboardingState(),
                ]);
                setBlocklistState(bl);
                setRetentionDaysState(ret);
                setMcpStatus(mcp);
                const name = onboarding.display_name ?? "";
                setProfileName(name);
                setProfileDraft(name);
            }
        } catch (err) {
            console.error("Failed to load settings data:", err);
        }
    }, [evalUi]);

    useEffect(() => {
        if (isOpen) {
            void loadData();
        }
    }, [isOpen, loadData]);

    const loadModels = useCallback(async () => {
        setModelsLoading(true);
        try {
            const ms = await listAvailableModels();
            setModels(ms);
        } catch (e) {
            setModelError(String(e));
        } finally {
            setModelsLoading(false);
        }
    }, []);

    useEffect(() => {
        if (isOpen && activeTab === "model") {
            void loadModels();
        }
    }, [isOpen, activeTab, loadModels]);

    // Close on escape
    useEffect(() => {
        const handleEscape = (e: KeyboardEvent) => {
            if (e.key === "Escape") setIsOpen(false);
        };
        if (isOpen) {
            window.addEventListener("keydown", handleEscape);
            return () => window.removeEventListener("keydown", handleEscape);
        }
    }, [isOpen]);

    // Apply theme to document root
    useEffect(() => {
        document.documentElement.setAttribute("data-theme", theme);
        localStorage.setItem("fndr-theme", theme);
    }, [theme]);

    useEffect(() => {
        if (!downloadingId || downloadStatus.model_id !== downloadingId) {
            return;
        }

        if (downloadStatus.state === "failed" && downloadStatus.error) {
            setModelError(downloadStatus.error);
            setDownloadingId(null);
            void loadModels();
            return;
        }

        if (downloadStatus.state !== "completed" || downloadStatus.error) {
            return;
        }

        let cancelled = false;
        setDownloadingId(null);
        setIsActivatingModel(true);

        void (async () => {
            try {
                const runtime = await refreshAiModels();
                if (!runtime.ai_model_available && !cancelled) {
                    setModelError(
                        `Model download finished, but FNDR still cannot see Qwen at ${downloadStatus.destination_path ?? "disk"}.`,
                    );
                }
            } catch (refreshError) {
                if (!cancelled) {
                    setModelError(`Model downloaded, but FNDR failed to refresh the runtime: ${String(refreshError)}`);
                }
            } finally {
                if (!cancelled) {
                    setIsActivatingModel(false);
                    void loadModels();
                }
            }
        })();

        return () => {
            cancelled = true;
        };
    }, [downloadStatus.destination_path, downloadStatus.error, downloadStatus.model_id, downloadStatus.state, downloadingId, loadModels]);

    const handleDownloadModel = async (model: ModelInfo) => {
        if (downloadingId) return;
        setModelError(null);

        if (model.download_url === "already_downloaded") {
            setIsActivatingModel(true);
            try {
                const runtime = await refreshAiModels();
                if (!runtime.ai_model_available) {
                    setModelError("Qwen is supposed to be on disk, but FNDR could not find the local model files.");
                }
            } catch (e) {
                setModelError(String(e));
            } finally {
                setIsActivatingModel(false);
                await loadModels();
            }
            return;
        }

        setDownloadingId(model.id);
        try {
            await downloadModel(model.id, model.download_url, model.filename);
        } catch (e) {
            setModelError(String(e));
            setDownloadingId(null);
        }
    };

    const handleDeleteModel = async (model: ModelInfo) => {
        if (confirmDeleteModel !== model.id) {
            setConfirmDeleteModel(model.id);
            return;
        }
        setConfirmDeleteModel(null);
        setModelError(null);
        try {
            await deleteAiModel(model.filename);
            await loadModels();
        } catch (e) {
            setModelError(String(e));
        }
    };

    const handleRetentionChange = async (days: number) => {
        try {
            await setRetentionDays(days);
            setRetentionDaysState(days);
        } catch (e) {
            console.error("Failed to set retention:", e);
        }
    };

    const handleRunRetentionNow = async () => {
        if (retentionDays === 0) return;
        setRetentionBusy(true);
        try {
            await deleteOlderThan(retentionDays);
        } catch (e) {
            console.error("Failed to run retention:", e);
        } finally {
            setRetentionBusy(false);
        }
    };

    const handleToggleCapture = async () => {
        try {
            if (status?.is_paused) {
                await resumeCapture();
            } else {
                await pauseCapture();
            }
        } catch (e) {
            console.error("Failed to toggle capture:", e);
        }
    };

    const handleAddApp = async () => {
        if (!newApp.trim()) return;
        const updated = [...blocklist, newApp.trim()];
        try {
            await setBlocklist(updated);
            setBlocklistState(updated);
            setNewApp("");
        } catch (e) {
            console.error("Failed to update blocklist:", e);
        }
    };

    const handleRemoveApp = async (app: string) => {
        const updated = blocklist.filter((a) => a !== app);
        try {
            await setBlocklist(updated);
            setBlocklistState(updated);
        } catch (e) {
            console.error("Failed to update blocklist:", e);
        }
    };

    const handleDeleteAll = async () => {
        if (!confirmDelete) {
            setConfirmDelete(true);
            return;
        }
        try {
            await deleteAllData();
            setConfirmDelete(false);
        } catch (e) {
            console.error("Failed to delete data:", e);
        }
    };

    const handleRunRepairBackfill = async () => {
        setRepairBusy(true);
        setRepairError(null);
        setRepairSummary(null);
        try {
            const summary = await runMemoryRepairBackfill();
            setRepairSummary(summary);
        } catch (e) {
            setRepairError(String(e));
        } finally {
            setRepairBusy(false);
        }
    };

    useEffect(() => {
        if (!repairBusy) {
            return;
        }

        let mounted = true;
        const pollProgress = async () => {
            try {
                const progress = await getMemoryRepairProgress();
                if (mounted) {
                    setRepairProgress(progress);
                }
            } catch {
                // Ignore transient polling failures while repair is running.
            }
        };

        void pollProgress();
        const timer = window.setInterval(() => {
            void pollProgress();
        }, 1000);

        return () => {
            mounted = false;
            window.clearInterval(timer);
        };
    }, [repairBusy]);

    const handleToggleMcpServer = async () => {
        setMcpBusy(true);
        try {
            const updated = mcpStatus?.running ? await stopMcpServer() : await startMcpServer();
            setMcpStatus(updated);
        } catch (e) {
            console.error("Failed to toggle MCP server:", e);
        } finally {
            setMcpBusy(false);
        }
    };

    const handleCopyMcpLink = async () => {
        if (!mcpStatus?.endpoint) return;
        try {
            await navigator.clipboard.writeText(mcpStatus.endpoint);
            setCopiedMcpLink(true);
            setTimeout(() => setCopiedMcpLink(false), 1500);
        } catch (e) {
            console.error("Failed to copy MCP endpoint:", e);
        }
    };

    const handleSaveProfile = async () => {
        setProfileBusy(true);
        setProfileMsg(null);
        try {
            const onboarding: OnboardingState = await getOnboardingState();
            const normalized = profileDraft.trim();
            await saveOnboardingState({
                ...onboarding,
                display_name: normalized || null,
            });
            setProfileName(normalized);
            setProfileDraft(normalized);
            window.dispatchEvent(
                new CustomEvent("fndr-profile-updated", {
                    detail: { displayName: normalized || null },
                })
            );
            setProfileMsg("Saved");
        } catch (err) {
            setProfileMsg(`Failed to save: ${String(err)}`);
        } finally {
            setProfileBusy(false);
            window.setTimeout(() => setProfileMsg(null), 1400);
        }
    };

    function fmtBytes(b: number) {
        return b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${(b / 1e6).toFixed(0)} MB`;
    }

    return (
        <>
            <button
                className={`ui-action-btn settings-toggle ${compact ? "compact" : ""}`}
                onClick={() => setIsOpen(!isOpen)}
                aria-label="Open settings"
            >
                <svg className="settings-toggle-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                    <circle cx="12" cy="12" r="3" />
                    <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.86l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.7 1.7 0 0 0-1.86-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 0 1-4 0v-.09a1.7 1.7 0 0 0-1-1.55 1.7 1.7 0 0 0-1.86.34l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.86 1.7 1.7 0 0 0-1.55-1H3a2 2 0 0 1 0-4h.09a1.7 1.7 0 0 0 1.55-1 1.7 1.7 0 0 0-.34-1.86l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.7 1.7 0 0 0 1.86.34h0a1.7 1.7 0 0 0 1-1.55V3a2 2 0 0 1 4 0v.09a1.7 1.7 0 0 0 1 1.55h0a1.7 1.7 0 0 0 1.86-.34l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.86v0a1.7 1.7 0 0 0 1.55 1H21a2 2 0 0 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1Z" />
                </svg>
            </button>

            {isOpen && <div className="panel-backdrop" onClick={() => setIsOpen(false)} />}

            <aside className={`settings-panel ${isOpen ? "open" : ""}`}>
                <header className="panel-header">
                    <div>
                        <h2>FNDR Settings</h2>
                        <p className="panel-subtitle">Private, local, always in your control.</p>
                    </div>
                    <button className="ui-action-btn panel-close" onClick={() => setIsOpen(false)} aria-label="Close">
                        ✕
                    </button>
                </header>

                <nav className="panel-tabs">
                    <button
                        className={`ui-action-btn tab ${activeTab === "settings" ? "active" : ""}`}
                        onClick={() => setActiveTab("settings")}
                    >
                        Core
                    </button>
                    <button
                        className={`ui-action-btn tab ${activeTab === "model" ? "active" : ""}`}
                        onClick={() => setActiveTab("model")}
                    >
                        Model
                    </button>
                    <button
                        className={`ui-action-btn tab ${activeTab === "privacy" ? "active" : ""}`}
                        onClick={() => setActiveTab("privacy")}
                    >
                        Privacy
                    </button>
                </nav>

                <div className="panel-content">
                    {activeTab === "settings" && (
                        <>
                            <section className="panel-section">
                                <h3>Profile</h3>
                                <p className="section-hint">
                                    FNDR uses this name in your greeting.
                                </p>
                                <div className="profile-row">
                                    <input
                                        type="text"
                                        value={profileDraft}
                                        onChange={(event) => setProfileDraft(event.target.value)}
                                        placeholder="Your name"
                                        className="profile-input"
                                        onKeyDown={(event) => {
                                            if (event.key === "Enter") {
                                                void handleSaveProfile();
                                            }
                                        }}
                                    />
                                    <button
                                        className="ui-action-btn btn-secondary"
                                        onClick={() => void handleSaveProfile()}
                                        disabled={profileBusy || profileDraft.trim() === profileName.trim()}
                                    >
                                        {profileBusy ? "..." : "Save"}
                                    </button>
                                </div>
                                {profileMsg && <p className="profile-msg">{profileMsg}</p>}
                            </section>

                            <section className="panel-section">
                                <h3>Appearance</h3>
                                <p className="section-hint">Choose your interface theme.</p>
                                <div className="theme-choice-row" role="radiogroup" aria-label="Theme selection">
                                    <button
                                        className={`ui-action-btn theme-choice ${theme === "dark" ? "active" : ""}`}
                                        onClick={() => setTheme("dark")}
                                        aria-pressed={theme === "dark"}
                                    >
                                        <span className="theme-choice-icon" aria-hidden="true">🌙</span>
                                        Dark
                                    </button>
                                    <button
                                        className={`ui-action-btn theme-choice ${theme === "light" ? "active" : ""}`}
                                        onClick={() => setTheme("light")}
                                        aria-pressed={theme === "light"}
                                    >
                                        <span className="theme-choice-icon" aria-hidden="true">☀️</span>
                                        Light
                                    </button>
                                </div>
                            </section>

                            <section className="panel-section">
                                <h3>Capture Status</h3>
                                <button
                                    className={`ui-action-btn capture-toggle ${status?.is_paused ? "paused" : "active"}`}
                                    onClick={handleToggleCapture}
                                >
                                    {status?.is_paused ? "Resume capture" : "Pause capture"}
                                </button>
                                <div className="capture-stats">
                                    <span>Frames: {status?.frames_captured ?? 0}</span>
                                    <span>Dropped: {status?.frames_dropped ?? 0}</span>
                                </div>
                            </section>

                            <section className="panel-section">
                                <h3>Indexing</h3>
                                <p className="section-hint">
                                    Keep a compact rolling memory window.
                                </p>
                                <div className="retention-controls">
                                    <select
                                        value={retentionDays}
                                        onChange={(e) => void handleRetentionChange(Number(e.target.value))}
                                        className="retention-select"
                                    >
                                        <option value={7}>7 days</option>
                                        <option value={30}>30 days</option>
                                        <option value={90}>90 days</option>
                                        <option value={0}>Forever</option>
                                    </select>
                                    {retentionDays > 0 && (
                                        <button
                                            className="ui-action-btn btn-secondary"
                                            onClick={() => void handleRunRetentionNow()}
                                            disabled={retentionBusy}
                                        >
                                            {retentionBusy ? "..." : "Run now"}
                                        </button>
                                    )}
                                </div>
                            </section>

                            {!evalUi && (
                                <section className="panel-section">
                                    <h3>MCP Server</h3>
                                    <p className="section-hint">
                                        Connect FNDR to external tools via Model Context Protocol.
                                    </p>
                                    <div className="mcp-status-row">
                                        <span className={`mcp-pill ${mcpStatus?.running ? "running" : "stopped"}`}>
                                            {mcpStatus?.running ? "Running" : "Stopped"}
                                        </span>
                                        <button
                                            className="ui-action-btn btn-secondary"
                                            onClick={() => void handleToggleMcpServer()}
                                            disabled={mcpBusy}
                                        >
                                            {mcpBusy ? "..." : mcpStatus?.running ? "Stop" : "Start"}
                                        </button>
                                    </div>
                                    <div className="mcp-link-row">
                                        <input
                                            className="mcp-link-input"
                                            value={mcpStatus?.endpoint ?? "http://127.0.0.1:8799/mcp"}
                                            readOnly
                                        />
                                        <button className="ui-action-btn btn-primary" onClick={() => void handleCopyMcpLink()}>
                                            {copiedMcpLink ? "Copied" : "Copy link"}
                                        </button>
                                    </div>
                                    {mcpStatus?.last_error && <p className="mcp-error">{mcpStatus.last_error}</p>}
                                </section>
                            )}
                        </>
                    )}

                    {activeTab === "model" && (
                        <section className="panel-section">
                            <h3>AI Model</h3>
                            <p className="section-hint">
                                Qwen3-VL is FNDR&apos;s required local model for summaries, Q&amp;A, and smarter indexing.
                                {status?.ai_model_available
                                    ? status?.ai_model_loaded
                                        ? " It is currently loaded in memory."
                                        : " It is downloaded and will load automatically when needed."
                                    : " It is not downloaded yet."}
                            </p>
                            <p className="section-hint">
                                Search embeddings: {status
                                    ? status.embedding_degraded
                                        ? `degraded (${status.embedding_backend})`
                                        : status.embedding_backend
                                    : "unknown"}.
                                {status?.embedding_detail ? ` ${status.embedding_detail}` : ""}
                            </p>

                            {modelError && <div className="model-error">{modelError}</div>}

                            {modelsLoading && <p className="section-hint">Loading…</p>}

                            {!modelsLoading && models.map((model) => {
                                const isDownloaded = model.download_url === "already_downloaded";
                                const isDownloading = downloadingId === model.id;
                                const confirmingDelete = confirmDeleteModel === model.id;
                                const shouldShowActivate = isDownloaded && !status?.ai_model_loaded;

                                return (
                                    <div key={model.id} className={`model-row ${isDownloaded ? "downloaded" : ""}`}>
                                        <div className="model-row-info">
                                            <div className="model-row-name">
                                                {model.name}
                                                {isDownloaded && <span className="model-badge-downloaded">Downloaded</span>}
                                                {model.recommended && !isDownloaded && <span className="model-badge-recommended">Recommended</span>}
                                            </div>
                                            <div className="model-row-meta">{model.size_label} · {model.speed_label} · ~{model.ram_gb} GB RAM</div>
                                            <div className="model-row-desc">{model.description}</div>
                                        </div>

                                        {isDownloading ? (
                                            <div className="model-dl-progress">
                                                {downloadStatus.state === "downloading" ? (
                                                    <>
                                                        <div className="model-dl-bar-wrap">
                                                            <div className="model-dl-bar-fill" style={{ width: `${downloadStatus.percent.toFixed(1)}%` }} />
                                                        </div>
                                                        <span className="model-dl-pct">
                                                            {fmtBytes(downloadStatus.bytes_downloaded)} / {fmtBytes(downloadStatus.total_bytes)} ({downloadStatus.percent.toFixed(0)}%)
                                                        </span>
                                                    </>
                                                ) : (
                                                    <span className="model-dl-pct">
                                                        {isActivatingModel
                                                            ? "Loading model…"
                                                            : downloadStatus.state === "finalizing"
                                                                ? "Finalizing…"
                                                                : "Connecting…"}
                                                    </span>
                                                )}
                                            </div>
                                        ) : shouldShowActivate ? (
                                            <button
                                                className="btn-liquid-glass"
                                                onClick={() => void handleDownloadModel(model)}
                                                disabled={isActivatingModel}
                                            >
                                                {isActivatingModel ? "..." : "Load Now"}
                                            </button>
                                        ) : isDownloaded ? (
                                            <button
                                                className={`btn-danger-sm ${confirmingDelete ? "confirm" : ""}`}
                                                onClick={() => void handleDeleteModel(model)}
                                            >
                                                {confirmingDelete ? "Confirm delete" : "Delete"}
                                            </button>
                                        ) : (
                                            <button
                                                className="btn-primary-sm"
                                                onClick={() => void handleDownloadModel(model)}
                                                disabled={!!downloadingId}
                                            >
                                                Download
                                            </button>
                                        )}
                                    </div>
                                );
                            })}

                            {(downloadingId || isActivatingModel) && (
                                <div style={{
                                    marginTop: 16,
                                    background: "rgba(255,255,255,0.04)",
                                    border: "1px solid rgba(255,255,255,0.08)",
                                    borderRadius: 10,
                                    padding: 12,
                                    fontFamily: "inherit",
                                    fontSize: 11,
                                    color: "rgba(255,255,255,0.75)",
                                    maxHeight: 140,
                                    overflowY: "auto"
                                }}>
                                    <div style={{ color: "rgba(255,255,255,0.95)", marginBottom: 8 }}>
                                        Stage: {isActivatingModel ? "activating" : downloadStatus.state}
                                    </div>
                                    {downloadStatus.destination_path && (
                                        <div style={{ marginBottom: 8 }}>{downloadStatus.destination_path}</div>
                                    )}
                                    {downloadStatus.logs.map((line, index) => (
                                        <div key={index} style={{ marginBottom: 4 }}>{line}</div>
                                    ))}
                                </div>
                            )}
                        </section>
                    )}

                    {activeTab === "privacy" && (
                        <>
                            <section className="panel-section">
                                <h3>Blocked Apps</h3>
                                <p className="section-hint">These apps will not be captured.</p>
                                <div className="blocklist">
                                    {blocklist.length === 0 ? (
                                        <p className="blocklist-empty">No apps blocked</p>
                                    ) : (
                                        blocklist.map((app) => (
                                            <div key={app} className="blocklist-item">
                                                <span>{app}</span>
                                                <button onClick={() => void handleRemoveApp(app)}>✕</button>
                                            </div>
                                        ))
                                    )}
                                </div>
                                <div className="add-app-row">
                                    <input
                                        type="text"
                                        placeholder="Add app name..."
                                        value={newApp}
                                        onChange={(e) => setNewApp(e.target.value)}
                                        onKeyDown={(e) => e.key === "Enter" && void handleAddApp()}
                                        className="add-app-input"
                                    />
                                    <button onClick={() => void handleAddApp()} className="ui-action-btn btn-primary">Add</button>
                                </div>
                            </section>

                            <section className="panel-section danger-section">
                                <h3>Danger Zone</h3>
                                <p className="section-hint">
                                    One-time repair can merge historical duplicate memories into continuity cards.
                                </p>
                                <button
                                    className="ui-action-btn"
                                    onClick={() => void handleRunRepairBackfill()}
                                    disabled={repairBusy}
                                >
                                    {repairBusy ? "Repairing..." : "Run memory continuity repair (one-time)"}
                                </button>
                                {repairBusy && repairProgress && (
                                    <p className="section-hint" style={{ marginTop: 8 }}>
                                        Progress: {repairProgress.processed.toLocaleString()} / {repairProgress.total.toLocaleString()} ·
                                        phase {repairProgress.phase} · merged {repairProgress.merged_count.toLocaleString()} ·
                                        anchor merges {repairProgress.anchor_merges.toLocaleString()}
                                    </p>
                                )}
                                {repairSummary && (
                                    <p className="section-hint" style={{ marginTop: 8 }}>
                                        Merged {repairSummary.merged_count} duplicates ({repairSummary.total_before} → {repairSummary.total_after} cards),
                                        updated {repairSummary.task_reference_updates} task references.
                                        Spotify {repairSummary.spotify_merges}, YouTube {repairSummary.youtube_merges},
                                        Codex {repairSummary.codex_merges}, Discord {repairSummary.discord_merges},
                                        GitLab {repairSummary.gitlab_merges}, Antigravity {repairSummary.antigravity_merges}.
                                    </p>
                                )}
                                {repairError && <p className="section-hint" style={{ marginTop: 8 }}>{repairError}</p>}
                                <button
                                    className={`ui-action-btn btn-danger ${confirmDelete ? "confirm" : ""}`}
                                    onClick={() => void handleDeleteAll()}
                                >
                                    {confirmDelete ? "Click again to confirm" : "Delete all data"}
                                </button>
                            </section>
                        </>
                    )}
                </div>
            </aside>
        </>
    );
}
