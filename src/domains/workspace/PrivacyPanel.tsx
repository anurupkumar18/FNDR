import { useCallback, useEffect, useState } from "react";
import {
    PRIVACY_ALERTS_EVENT,
    PrivacyAlert,
    getBlocklist,
    getPrivacyAlerts,
    addSiteToBlocklist,
    dismissPrivacyAlert,
} from "@/shared/ipc/tauri";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import { Icon } from "@/shared/components/atoms";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
} from "@/shared/activity/activityTrace";
import "./PrivacyPanel.css";

interface PrivacyPanelProps {
    isVisible: boolean;
    onClose: () => void;
    onAlertsChange?: (count: number) => void;
    onBlocklistChange?: (blocklist: string[]) => void;
    embedded?: boolean;
}

export function PrivacyPanel({
    isVisible,
    onClose,
    onAlertsChange,
    onBlocklistChange,
    embedded = false,
}: PrivacyPanelProps) {
    const [alerts, setAlerts] = useState<PrivacyAlert[]>([]);
    const [loading, setLoading] = useState(false);
    const [privacyActivity, setPrivacyActivity] = useState<ActivityTraceSnapshot | null>(null);

    const refreshAlerts = useCallback(async (isMounted: () => boolean = () => true) => {
        try {
            const data = await getPrivacyAlerts();
            if (!isMounted()) {
                return;
            }
            setAlerts(data);
            if (onAlertsChange) {
                onAlertsChange(data.length);
            }
        } catch (err) {
            console.error("Failed to load privacy alerts:", err);
        }
    }, [onAlertsChange]);

    useEffect(() => {
        void refreshAlerts();
    }, [refreshAlerts]);

    useTauriEvent<PrivacyAlert[]>(PRIVACY_ALERTS_EVENT, (data) => {
        setAlerts(data);
        onAlertsChange?.(data.length);
    });

    const handleAddBlocklist = async (site: string) => {
        const startedAtMs = Date.now();
        const startedTrace = recordActivityStep(
            beginActivityTrace({
                id: `privacy-cleanup-${startedAtMs}`,
                title: "Privacy cleanup activity",
                startedAtMs,
            }),
            {
                id: "block-and-delete",
                label: "Applying blocklist and local cleanup",
                actor: "FNDR privacy service",
                status: "running",
                evidence: "ipc-boundary",
                atMs: startedAtMs,
            },
        );
        setPrivacyActivity(startedTrace);
        setLoading(true);
        try {
            await addSiteToBlocklist(site);
            const [nextBlocklist] = await Promise.all([
                getBlocklist(),
                refreshAlerts(),
            ]);
            onBlocklistChange?.(nextBlocklist);
            const finishedAtMs = Date.now();
            setPrivacyActivity(recordActivityStep(startedTrace, {
                id: "block-and-delete",
                label: "Blocklist and local cleanup completed",
                actor: "FNDR privacy service",
                status: "completed",
                evidence: "result-metadata",
                atMs: finishedAtMs,
                durationMs: finishedAtMs - startedAtMs,
            }));
            if (alerts.length <= 1) {
                onClose(); // auto close if it was the last one
            }
        } catch (err) {
            console.error("Failed to add to blocklist:", err);
            const failedAtMs = Date.now();
            setPrivacyActivity(recordActivityStep(startedTrace, {
                id: "block-and-delete",
                label: "Blocklist and local cleanup failed",
                actor: "FNDR privacy service",
                status: "failed",
                evidence: "ipc-boundary",
                atMs: failedAtMs,
                durationMs: failedAtMs - startedAtMs,
            }));
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
        <aside className={`privacy-panel open ${embedded ? "embedded" : ""}`}>
            <header className="privacy-header">
                <h2>Privacy alerts</h2>
                {!embedded && (
                    <button className="ui-action-btn close-btn" onClick={onClose}>X</button>
                )}
            </header>

            <div className="privacy-content">
                {privacyActivity && <ActivityTrace trace={privacyActivity} />}
                {alerts.length === 0 ? (
                    <div className="empty-alerts">
                        <span className="empty-icon"><Icon name="shield" size={32} /></span>
                        <p>No recent apps or sites need your review.</p>
                        <small>
                            Detection can miss sensitive content. Use blocklists or pause capture before
                            opening private material.
                        </small>
                    </div>
                ) : (
                    <div className="alerts-list">
                        {alerts.map((alert) => (
                            <div key={alert.id} className="privacy-alert-card">
                                <div className="shield-icon-container">
                                    <Icon name="shield" size={22} className="shield-icon" />
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
