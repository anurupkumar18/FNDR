import { useState, useEffect } from "react";
import { AgentStatus, getAgentStatus, stopAgent } from "../api/tauri";
import "./AgentPanel.css";

interface AgentPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

export function AgentPanel({ isVisible, onClose }: AgentPanelProps) {
    const [status, setStatus] = useState<AgentStatus | null>(null);

    useEffect(() => {
        if (!isVisible) return;

        let cancelled = false;
        let intervalId: number | null = null;

        const pollStatus = async () => {
            try {
                const s = await getAgentStatus();
                if (cancelled) {
                    return;
                }
                setStatus(s);

                // Stop polling if completed or error
                if (s.status === "completed" || s.status === "error") {
                    if (intervalId !== null) {
                        window.clearInterval(intervalId);
                    }
                    return;
                }
            } catch (err) {
                console.error("Failed to get agent status:", err);
            }
        };

        void pollStatus();
        intervalId = window.setInterval(() => {
            void pollStatus();
        }, 1000);

        return () => {
            cancelled = true;
            if (intervalId !== null) {
                window.clearInterval(intervalId);
            }
        };
    }, [isVisible]);

    const handleStop = async () => {
        try {
            await stopAgent();
            setStatus(null);
            onClose();
        } catch (err) {
            console.error("Failed to stop agent:", err);
        }
    };

    const handleClose = () => {
        setStatus(null);
        onClose();
    };

    if (!isVisible) return null;

    const getStatusIcon = () => {
        switch (status?.status) {
            case "running": return "🤖";
            case "completed": return "✅";
            case "error": return "❌";
            default: return "⏳";
        }
    };

    const getStatusColor = () => {
        switch (status?.status) {
            case "running": return "status-running";
            case "completed": return "status-completed";
            case "error": return "status-error";
            default: return "status-idle";
        }
    };

    return (
        <div className="agent-overlay">
            <div className="agent-panel">
                <header className="agent-header">
                    <div className="agent-title">
                        <span className="agent-icon">{getStatusIcon()}</span>
                        <h2>AI Agent</h2>
                    </div>
                    <button className="btn-close" onClick={handleClose} title="Close">X</button>
                </header>

                <div className="agent-body">
                    {status?.task_title && (
                        <div className="agent-task">
                            <span className="label">Task:</span>
                            <span className="value">{status.task_title}</span>
                        </div>
                    )}

                    <div className={`agent-status ${getStatusColor()}`}>
                        <span className="label">Status:</span>
                        <span className="value">{status?.status || "Initializing..."}</span>
                    </div>

                    {status?.last_message && (
                        <div className="agent-message">
                            <span className="label">Progress:</span>
                            <p className="value">{status.last_message}</p>
                        </div>
                    )}

                    {!status?.is_running && status?.status === "idle" && (
                        <div className="agent-setup">
                            <p className="setup-note">
                                💡 <strong>Setup Required:</strong> Set your <code>ANTHROPIC_API_KEY</code> environment variable to enable the AI agent.
                            </p>
                            <a
                                href="https://console.anthropic.com/"
                                target="_blank"
                                rel="noopener noreferrer"
                                className="btn-setup"
                            >
                                Get API Key →
                            </a>
                        </div>
                    )}
                </div>

                <footer className="agent-footer">
                    {status?.is_running ? (
                        <button className="btn-stop" onClick={handleStop}>
                            ⏹ Stop Agent
                        </button>
                    ) : (
                        <button className="btn-done" onClick={handleClose}>
                            Done
                        </button>
                    )}
                </footer>
            </div>
        </div>
    );
}
