import { useState, useEffect, useCallback } from "react";
import {
    CaptureStatus,
    McpServerStatus,
    pauseCapture,
    resumeCapture,
    getBlocklist,
    setBlocklist,
    deleteAllData,
    getStats,
    getRetentionDays,
    setRetentionDays,
    deleteOlderThan,
    getMcpServerStatus,
    startMcpServer,
    stopMcpServer,
    Stats,
} from "../api/tauri";
import {
    ModelInfo,
    listAvailableModels,
    downloadModel,
    deleteAiModel,
    refreshAiModels,
} from "../api/onboarding";
import { useModelDownloadStatus } from "../hooks/useModelDownloadStatus";
import "./ControlPanel.css";

interface ControlPanelProps {
    status: CaptureStatus | null;
}

type Tab = "settings" | "model" | "stats" | "privacy";

export function ControlPanel({ status }: ControlPanelProps) {
    const [isOpen, setIsOpen] = useState(false);
    const [activeTab, setActiveTab] = useState<Tab>("settings");
    const [blocklist, setBlocklistState] = useState<string[]>([]);
    const [newApp, setNewApp] = useState("");
    const [stats, setStats] = useState<Stats | null>(null);
    const [confirmDelete, setConfirmDelete] = useState(false);
    const [retentionDays, setRetentionDaysState] = useState<number>(7);
    const [retentionBusy, setRetentionBusy] = useState(false);
    const [mcpStatus, setMcpStatus] = useState<McpServerStatus | null>(null);
    const [mcpBusy, setMcpBusy] = useState(false);
    const [copiedMcpLink, setCopiedMcpLink] = useState(false);

    // Model tab state
    const [models, setModels] = useState<ModelInfo[]>([]);
    const [modelsLoading, setModelsLoading] = useState(false);
    const [downloadingId, setDownloadingId] = useState<string | null>(null);
    const [modelError, setModelError] = useState<string | null>(null);
    const [confirmDeleteModel, setConfirmDeleteModel] = useState<string | null>(null);
    const [isActivatingModel, setIsActivatingModel] = useState(false);
    const downloadStatus = useModelDownloadStatus();

    useEffect(() => {
        if (isOpen) {
            loadData();
        }
    }, [isOpen]);

    useEffect(() => {
        if (isOpen && activeTab === "model") {
            loadModels();
        }
    }, [isOpen, activeTab]);

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

    const loadData = async () => {
        try {
            const [bl, st, ret, mcp] = await Promise.all([
                getBlocklist(),
                getStats(),
                getRetentionDays(),
                getMcpServerStatus(),
            ]);
            setBlocklistState(bl);
            setStats(st);
            setRetentionDaysState(ret);
            setMcpStatus(mcp);
        } catch (e) {
            console.error("Failed to load data:", e);
        }
    };

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
            const deleted = await deleteOlderThan(retentionDays);
            if (deleted > 0) await loadData();
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
            loadData();
        } catch (e) {
            console.error("Failed to delete data:", e);
        }
    };

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

    function fmtBytes(b: number) {
        return b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${(b / 1e6).toFixed(0)} MB`;
    }

    return (
        <>
            <button
                className="settings-toggle"
                onClick={() => setIsOpen(!isOpen)}
                aria-label="Open settings"
            >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <circle cx="12" cy="12" r="3" />
                    <path d="M12 1v2m0 18v2M4.22 4.22l1.42 1.42m12.72 12.72l1.42 1.42M1 12h2m18 0h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
                </svg>
            </button>

            {isOpen && <div className="panel-backdrop" onClick={() => setIsOpen(false)} />}

            <aside className={`settings-panel ${isOpen ? "open" : ""}`}>
                <header className="panel-header">
                    <h2>Settings</h2>
                    <button className="panel-close" onClick={() => setIsOpen(false)} aria-label="Close">✕</button>
                </header>

                <nav className="panel-tabs">
                    {(["settings", "model", "stats", "privacy"] as Tab[]).map((t) => (
                        <button
                            key={t}
                            className={`tab ${activeTab === t ? "active" : ""}`}
                            onClick={() => setActiveTab(t)}
                        >
                            {t === "settings" ? "⚙️ General" : t === "model" ? "🧠 Model" : t === "stats" ? "📊 Stats" : "🔒 Privacy"}
                        </button>
                    ))}
                </nav>

                <div className="panel-content">
                    {/* General Tab */}
                    {activeTab === "settings" && (
                        <>
                            <section className="panel-section">
                                <h3>Capture Control</h3>
                                <button
                                    className={`capture-toggle ${status?.is_paused ? "paused" : "active"}`}
                                    onClick={handleToggleCapture}
                                >
                                    {status?.is_paused ? "▶ Resume Capture" : "⏸ Pause Capture"}
                                </button>
                                <div className="capture-stats">
                                    <span>Frames: {status?.frames_captured ?? 0}</span>
                                    <span>Dropped: {status?.frames_dropped ?? 0}</span>
                                </div>
                            </section>

                            <section className="panel-section">
                                <h3>Data Retention</h3>
                                <p className="section-hint">Automatically remove old memories to save space.</p>
                                <div className="retention-controls">
                                    <select
                                        value={retentionDays}
                                        onChange={(e) => handleRetentionChange(Number(e.target.value))}
                                        className="retention-select"
                                    >
                                        <option value={7}>7 days</option>
                                        <option value={30}>30 days</option>
                                        <option value={90}>90 days</option>
                                        <option value={0}>Forever</option>
                                    </select>
                                    {retentionDays > 0 && (
                                        <button className="btn-secondary" onClick={handleRunRetentionNow} disabled={retentionBusy}>
                                            {retentionBusy ? "..." : "Run Now"}
                                        </button>
                                    )}
                                </div>
                            </section>

                            <section className="panel-section">
                                <h3>MCP Server</h3>
                                <p className="section-hint">Connect FNDR to external tools via Model Context Protocol.</p>
                                <div className="mcp-status-row">
                                    <span className={`mcp-pill ${mcpStatus?.running ? "running" : "stopped"}`}>
                                        {mcpStatus?.running ? "Running" : "Stopped"}
                                    </span>
                                    <button className="btn-secondary" onClick={handleToggleMcpServer} disabled={mcpBusy}>
                                        {mcpBusy ? "..." : mcpStatus?.running ? "Stop" : "Start"}
                                    </button>
                                </div>
                                <div className="mcp-link-row">
                                    <input className="mcp-link-input" value={mcpStatus?.endpoint ?? "http://127.0.0.1:8799/mcp"} readOnly />
                                    <button className="btn-primary" onClick={handleCopyMcpLink}>
                                        {copiedMcpLink ? "Copied" : "Copy Link"}
                                    </button>
                                </div>
                                {mcpStatus?.last_error && <p className="mcp-error">{mcpStatus.last_error}</p>}
                            </section>
                        </>
                    )}

                    {/* Model Tab */}
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
                                                className="btn-primary-sm"
                                                onClick={() => handleDownloadModel(model)}
                                            >
                                                Load Now
                                            </button>
                                        ) : isDownloaded ? (
                                            <button
                                                className={`btn-danger-sm ${confirmingDelete ? "confirm" : ""}`}
                                                onClick={() => handleDeleteModel(model)}
                                            >
                                                {confirmingDelete ? "Confirm delete" : "Delete"}
                                            </button>
                                        ) : (
                                            <button
                                                className="btn-primary-sm"
                                                onClick={() => handleDownloadModel(model)}
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
                                    fontFamily: "monospace",
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

                    {/* Stats Tab */}
                    {activeTab === "stats" && stats && (
                        <section className="panel-section">
                            <h3>Statistics</h3>
                            <div className="stats-grid">
                                <div className="stat-card">
                                    <span className="stat-value">{stats.total_records.toLocaleString()}</span>
                                    <span className="stat-label">Total Memories</span>
                                </div>
                                <div className="stat-card">
                                    <span className="stat-value">{stats.today_count.toLocaleString()}</span>
                                    <span className="stat-label">Today</span>
                                </div>
                                <div className="stat-card">
                                    <span className="stat-value">{stats.total_days}</span>
                                    <span className="stat-label">Days Active</span>
                                </div>
                            </div>
                            <div className="profile-section">
                                <h3>Profile</h3>
                                <div className="profile-card">
                                    <div className="profile-avatar">👤</div>
                                    <div className="profile-info">
                                        <span className="profile-name">Local User</span>
                                        <span className="profile-detail">All data stored locally on your Mac</span>
                                    </div>
                                </div>
                            </div>
                        </section>
                    )}

                    {/* Privacy Tab */}
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
                                                <button onClick={() => handleRemoveApp(app)}>✕</button>
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
                                        onKeyDown={(e) => e.key === "Enter" && handleAddApp()}
                                        className="add-app-input"
                                    />
                                    <button onClick={handleAddApp} className="btn-primary">Add</button>
                                </div>
                            </section>

                            <section className="panel-section danger-section">
                                <h3>Danger Zone</h3>
                                <button
                                    className={`btn-danger ${confirmDelete ? "confirm" : ""}`}
                                    onClick={handleDeleteAll}
                                >
                                    {confirmDelete ? "Click again to confirm" : "🗑️ Delete All Data"}
                                </button>
                            </section>

                            <section className="panel-section about-section">
                                <h3>About Privacy</h3>
                                <p className="about-text">
                                    FNDR runs 100% on your Mac. No screenshots or data are ever
                                    sent to the cloud. Screen content is converted to text and vectors
                                    locally—raw pixels are discarded immediately.
                                </p>
                            </section>
                        </>
                    )}
                </div>
            </aside>
        </>
    );
}
