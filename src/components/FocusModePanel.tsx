import { useEffect, useRef, useState } from "react";
import { setFocusTask, getFocusStatus, FocusStatus } from "../api/tauri";
import "./FocusModePanel.css";

interface FocusModePanelProps {
    isVisible: boolean;
    onClose: () => void;
}

export function FocusModePanel({ isVisible, onClose }: FocusModePanelProps) {
    const [status, setStatus] = useState<FocusStatus | null>(null);
    const [draft, setDraft] = useState("");
    const [saving, setSaving] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const inputRef = useRef<HTMLInputElement>(null);

    // Load current status when panel opens
    useEffect(() => {
        if (!isVisible) return;
        let mounted = true;

        const load = async () => {
            try {
                const s = await getFocusStatus();
                if (mounted) {
                    setStatus(s);
                    setDraft(s.task ?? "");
                }
            } catch {
                // non-fatal
            }
        };

        void load();
        const interval = window.setInterval(() => void load(), 5_000);

        // Auto-focus input
        setTimeout(() => inputRef.current?.focus(), 80);

        return () => {
            mounted = false;
            window.clearInterval(interval);
        };
    }, [isVisible]);

    if (!isVisible) return null;

    const isActive = status?.is_active ?? false;
    const driftCount = status?.drift_count ?? 0;
    const driftLevel = driftCount === 0 ? "none" : driftCount < 2 ? "low" : "high";

    const handleSet = async () => {
        const task = draft.trim() || null;
        setSaving(true);
        setError(null);
        try {
            const next = await setFocusTask(task);
            setStatus(next);
            setDraft(next.task ?? "");
        } catch (err) {
            setError(err instanceof Error ? err.message : "Failed to update focus task.");
        } finally {
            setSaving(false);
        }
    };

    const handleClear = async () => {
        setDraft("");
        setSaving(true);
        setError(null);
        try {
            const next = await setFocusTask(null);
            setStatus(next);
        } catch {
            // ignore
        } finally {
            setSaving(false);
        }
    };

    return (
        <div className="fm-page">
            <header className="fm-header">
                <div>
                    <h2>Focus Mode</h2>
                    <p>Semantic drift detection — FNDR alerts you when your screen activity drifts from your goal</p>
                </div>
                <button className="ui-action-btn fm-close-btn" onClick={onClose}>
                    Close
                </button>
            </header>

            <div className="fm-body">
                {/* Status indicator */}
                <div className={`fm-status-card ${isActive ? "active" : "idle"}`}>
                    <div className={`fm-status-dot ${isActive ? "on" : "off"}`} />
                    <div className="fm-status-info">
                        {isActive ? (
                            <>
                                <span className="fm-status-label">Focused on</span>
                                <span className="fm-status-task">{status?.task}</span>
                            </>
                        ) : (
                            <span className="fm-status-label">Focus mode is off</span>
                        )}
                    </div>
                    {isActive && (
                        <div className={`fm-drift-badge drift-${driftLevel}`}>
                            {driftLevel === "none" && "On track"}
                            {driftLevel === "low" && `Drift ×${driftCount}`}
                            {driftLevel === "high" && `Drifting ×${driftCount}`}
                        </div>
                    )}
                </div>

                {/* How it works */}
                <div className="fm-explainer">
                    <p>
                        Each screen capture is embedded and compared to your focus task. After
                        3 consecutive off-task captures FNDR notifies you to refocus.
                    </p>
                </div>

                {/* Task input */}
                <div className="fm-set-section">
                    <label className="fm-input-label" htmlFor="fm-task-input">
                        What are you working on?
                    </label>
                    <div className="fm-input-row">
                        <input
                            id="fm-task-input"
                            ref={inputRef}
                            className="fm-input"
                            type="text"
                            placeholder="e.g. Fix auth bug in payments service"
                            value={draft}
                            onChange={(e) => setDraft(e.target.value)}
                            onKeyDown={(e) => {
                                if (e.key === "Enter") void handleSet();
                                if (e.key === "Escape") onClose();
                            }}
                        />
                        <button
                            className="ui-action-btn fm-set-btn"
                            onClick={() => void handleSet()}
                            disabled={saving || draft.trim() === (status?.task ?? "")}
                        >
                            {saving ? "…" : isActive ? "Update" : "Start Focus"}
                        </button>
                    </div>
                    {error && <p className="fm-error">{error}</p>}
                </div>

                {isActive && (
                    <button
                        className="fm-stop-btn"
                        onClick={() => void handleClear()}
                        disabled={saving}
                    >
                        Stop focus mode
                    </button>
                )}

                {/* Drift history placeholder */}
                <div className="fm-tips">
                    <div className="fm-tip">Keep your task description specific — e.g. "debug the login flow" works better than "coding".</div>
                    <div className="fm-tip">FNDR uses the same embedding model as search to compare screen context vs your task.</div>
                    <div className="fm-tip">Drift alerts won't fire if the AI model isn't loaded yet.</div>
                </div>
            </div>
        </div>
    );
}
