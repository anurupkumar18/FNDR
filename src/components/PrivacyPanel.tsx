import { useEffect, useState } from "react";
import { PrivacyAlert, getPrivacyAlerts, addSiteToBlocklist, dismissPrivacyAlert } from "../api/tauri";
import "./PrivacyPanel.css";

interface PrivacyPanelProps {
    isVisible: boolean;
    onClose: () => void;
    onAlertsChange?: (count: number) => void;
}

export function PrivacyPanel({ isVisible, onClose, onAlertsChange }: PrivacyPanelProps) {
    const [alerts, setAlerts] = useState<PrivacyAlert[]>([]);
    const [loading, setLoading] = useState(false);

    const refreshAlerts = async () => {
        try {
            const data = await getPrivacyAlerts();
            setAlerts(data);
            if (onAlertsChange) {
                onAlertsChange(data.length);
            }
        } catch (err) {
            console.error("Failed to load privacy alerts:", err);
        }
    };

    useEffect(() => {
        refreshAlerts();
        // Periodically refresh alerts
        const interval = setInterval(refreshAlerts, 2000);
        return () => clearInterval(interval);
    }, []);

    const handleAddBlocklist = async (site: string) => {
        setLoading(true);
        try {
            await addSiteToBlocklist(site);
            await refreshAlerts();
            if (alerts.length <= 1) {
                onClose(); // auto close if it was the last one
            }
        } catch (err) {
            console.error("Failed to add to blocklist:", err);
        } finally {
            setLoading(false);
        }
    };

    const handleDismiss = async (site: string) => {
        setLoading(true);
        try {
            await dismissPrivacyAlert(site);
            await refreshAlerts();
            if (alerts.length <= 1) {
                onClose();
            }
        } catch (err) {
            console.error("Failed to dismiss alert:", err);
        } finally {
            setLoading(false);
        }
    };

    if (!isVisible) return null;

    return (
        <aside className="privacy-panel open">
            <header className="privacy-header">
                <h2>Privacy Alerts</h2>
                <button className="ui-action-btn close-btn" onClick={onClose}>
                    ✕
                </button>
            </header>

            <div className="privacy-content">
                {alerts.length === 0 ? (
                    <div className="empty-alerts">
                        <span className="empty-icon">🛡️</span>
                        <p>No active privacy alerts.</p>
                        <small>Your data is secure.</small>
                    </div>
                ) : (
                    <div className="alerts-list">
                        {alerts.map((alert) => (
                            <div key={alert.id} className="privacy-alert-card">
                                <div className="shield-icon-container">
                                    <span className="shield-icon">🛡️</span>
                                </div>
                                <div className="alert-details">
                                    <h4>Privacy Alert</h4>
                                    <p>
                                        You recently visited <strong>{alert.domain_or_title}</strong>, which appears to contain sensitive information. Would you like to add this site to your blocklist to prevent future recording?
                                    </p>
                                    <div className="alert-actions">
                                        <button
                                            className="ui-action-btn primary-action block-btn"
                                            onClick={() => handleAddBlocklist(alert.domain_or_title)}
                                            disabled={loading}
                                        >
                                            Add to Blocklist
                                        </button>
                                        <button
                                            className="ui-action-btn secondary-action dismiss-btn"
                                            onClick={() => handleDismiss(alert.domain_or_title)}
                                            disabled={loading}
                                        >
                                            Dismiss
                                        </button>
                                    </div>
                                    <small className="destructive-warning">Adding to blocklist will delete existing local recordings for this site.</small>
                                </div>
                            </div>
                        ))}
                    </div>
                )}
            </div>
        </aside>
    );
}
