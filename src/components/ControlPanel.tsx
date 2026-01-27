import { useState, useEffect } from "react";
import {
    CaptureStatus,
    pauseCapture,
    resumeCapture,
    getBlocklist,
    setBlocklist,
    deleteAllData,
    getStats,
    Stats,
} from "../api/tauri";
import "./ControlPanel.css";

interface ControlPanelProps {
    status: CaptureStatus | null;
}

export function ControlPanel({ status }: ControlPanelProps) {
    const [isOpen, setIsOpen] = useState(false);
    const [blocklist, setBlocklistState] = useState<string[]>([]);
    const [newApp, setNewApp] = useState("");
    const [stats, setStats] = useState<Stats | null>(null);
    const [confirmDelete, setConfirmDelete] = useState(false);

    useEffect(() => {
        if (isOpen) {
            loadData();
        }
    }, [isOpen]);

    const loadData = async () => {
        try {
            const [bl, st] = await Promise.all([getBlocklist(), getStats()]);
            setBlocklistState(bl);
            setStats(st);
        } catch (e) {
            console.error("Failed to load data:", e);
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

    return (
        <>
            <button className="control-toggle" onClick={() => setIsOpen(!isOpen)}>
                ⚙️
            </button>

            {isOpen && (
                <div className="control-panel">
                    <div className="panel-header">
                        <h2>Settings</h2>
                        <button className="close-btn" onClick={() => setIsOpen(false)}>
                            ×
                        </button>
                    </div>

                    <div className="panel-section">
                        <h3>Capture Control</h3>
                        <button
                            className={`toggle-btn ${status?.is_paused ? "paused" : "active"}`}
                            onClick={handleToggleCapture}
                        >
                            {status?.is_paused ? "▶ Resume Capture" : "⏸ Pause Capture"}
                        </button>
                        <div className="stats-row">
                            <span>Frames captured: {status?.frames_captured ?? 0}</span>
                            <span>Frames dropped: {status?.frames_dropped ?? 0}</span>
                        </div>
                    </div>

                    <div className="panel-section">
                        <h3>Statistics</h3>
                        {stats && (
                            <div className="stats-grid">
                                <div className="stat">
                                    <span className="stat-value">{stats.total_records}</span>
                                    <span className="stat-label">Total Records</span>
                                </div>
                                <div className="stat">
                                    <span className="stat-value">{stats.today_count}</span>
                                    <span className="stat-label">Today</span>
                                </div>
                                <div className="stat">
                                    <span className="stat-value">{stats.total_days}</span>
                                    <span className="stat-label">Days</span>
                                </div>
                            </div>
                        )}
                    </div>

                    <div className="panel-section">
                        <h3>Blocked Apps</h3>
                        <div className="blocklist">
                            {blocklist.map((app) => (
                                <div key={app} className="blocklist-item">
                                    <span>{app}</span>
                                    <button onClick={() => handleRemoveApp(app)}>×</button>
                                </div>
                            ))}
                        </div>
                        <div className="add-app">
                            <input
                                type="text"
                                placeholder="Add app name..."
                                value={newApp}
                                onChange={(e) => setNewApp(e.target.value)}
                                onKeyDown={(e) => e.key === "Enter" && handleAddApp()}
                            />
                            <button onClick={handleAddApp}>Add</button>
                        </div>
                    </div>

                    <div className="panel-section danger-zone">
                        <h3>Danger Zone</h3>
                        <button
                            className={`delete-btn ${confirmDelete ? "confirm" : ""}`}
                            onClick={handleDeleteAll}
                        >
                            {confirmDelete ? "Click again to confirm" : "🗑️ Delete All Data"}
                        </button>
                    </div>
                </div>
            )}

            {isOpen && <div className="overlay" onClick={() => setIsOpen(false)} />}
        </>
    );
}
