import { useState, useEffect } from "react";
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
import "./ControlPanel.css";

interface ControlPanelProps {
    status: CaptureStatus | null;
}

type Tab = "settings" | "stats" | "privacy";

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

    useEffect(() => {
        if (isOpen) {
            loadData();
        }
    }, [isOpen]);

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

    return (
        <>
            {/* Settings Toggle Button */}
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

            {/* Backdrop */}
            {isOpen && <div className="panel-backdrop" onClick={() => setIsOpen(false)} />}

            {/* Panel */}
            <aside className={`settings-panel ${isOpen ? "open" : ""}`}>
                <header className="panel-header">
                    <h2>Settings</h2>
                    <button className="panel-close" onClick={() => setIsOpen(false)} aria-label="Close">
                        ✕
                    </button>
                </header>

                {/* Tabs */}
                <nav className="panel-tabs">
                    <button
                        className={`tab ${activeTab === "settings" ? "active" : ""}`}
                        onClick={() => setActiveTab("settings")}
                    >
                        ⚙️ General
                    </button>
                    <button
                        className={`tab ${activeTab === "stats" ? "active" : ""}`}
                        onClick={() => setActiveTab("stats")}
                    >
                        📊 Stats
                    </button>
                    <button
                        className={`tab ${activeTab === "privacy" ? "active" : ""}`}
                        onClick={() => setActiveTab("privacy")}
                    >
                        🔒 Privacy
                    </button>
                </nav>

                <div className="panel-content">
                    {/* Settings Tab */}
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
                                <p className="section-hint">
                                    Automatically remove old memories to save space.
                                </p>
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
                                        <button
                                            className="btn-secondary"
                                            onClick={handleRunRetentionNow}
                                            disabled={retentionBusy}
                                        >
                                            {retentionBusy ? "..." : "Run Now"}
                                        </button>
                                    )}
                                </div>
                            </section>

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
                                        className="btn-secondary"
                                        onClick={handleToggleMcpServer}
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
                                    <button className="btn-primary" onClick={handleCopyMcpLink}>
                                        {copiedMcpLink ? "Copied" : "Copy Link"}
                                    </button>
                                </div>
                                {mcpStatus?.last_error && (
                                    <p className="mcp-error">{mcpStatus.last_error}</p>
                                )}
                            </section>
                        </>
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
                                    <div className="profile-avatar">
                                        👤
                                    </div>
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
                                <p className="section-hint">
                                    These apps will not be captured.
                                </p>
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
