import { useCallback, useEffect, useRef, useState } from "react";
import {
    type CaptureStatus,
    PRIVACY_ALERTS_EVENT,
    type PrivacyAlert,
    fndrQualityStatus,
    getBlocklist,
    getPrivacyAlerts,
    pauseCapture,
    resumeCapture,
    setBlocklist,
} from "@/shared/ipc/tauri";
import {
    type ModelInfo,
    type OnboardingState,
    getOnboardingState,
    listAvailableModels,
    saveOnboardingState,
} from "@/shared/ipc/onboarding";
import { type PanelKey } from "@/domains/command-palette/CommandPalette";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import { STORAGE_KEYS } from "@/shared/utils/config";
import { PrivacyPanel } from "./PrivacyPanel";
import "./ControlPanel.css";

interface ControlPanelProps {
    status: CaptureStatus | null;
    compact?: boolean;
    evalUi?: boolean;
    onOpenPanel?: (panel: PanelKey) => void;
}

type QualityStatus = {
    stored_count: number;
    dropped_count: number;
    flagged_count: number;
};

export function ControlPanel({
    status,
    compact: _compact = false,
    evalUi: _evalUi = false,
    onOpenPanel: _onOpenPanel,
}: ControlPanelProps) {
    const [isOpen, setIsOpen] = useState(false);
    const [theme, setTheme] = useState<"dark" | "light">(
        () => (localStorage.getItem(STORAGE_KEYS.theme) as "dark" | "light") || "dark",
    );
    const [blocklist, setBlocklistState] = useState<string[]>([]);
    const [newApp, setNewApp] = useState("");
    const [privacyAlertCount, setPrivacyAlertCount] = useState(0);
    const [profileName, setProfileName] = useState("");
    const [profileDraft, setProfileDraft] = useState("");
    const [profileBusy, setProfileBusy] = useState(false);
    const [profileMessage, setProfileMessage] = useState<string | null>(null);
    const [models, setModels] = useState<ModelInfo[]>([]);
    const [qualityStatus, setQualityStatus] = useState<QualityStatus | null>(null);
    const previousPrivacyAlertCount = useRef(0);

    const loadSettings = useCallback(async () => {
        try {
            const [nextBlocklist, onboarding, availableModels, quality] = await Promise.all([
                getBlocklist(),
                getOnboardingState(),
                listAvailableModels(),
                fndrQualityStatus(),
            ]);
            setBlocklistState(nextBlocklist);
            setModels(availableModels);
            setQualityStatus(quality);
            const name = onboarding.display_name ?? "";
            setProfileName(name);
            setProfileDraft(name);
        } catch (error) {
            console.error("Failed to load demo-safe settings:", error);
        }
    }, []);

    useEffect(() => {
        if (isOpen) void loadSettings();
    }, [isOpen, loadSettings]);

    useEffect(() => {
        let mounted = true;
        void getPrivacyAlerts()
            .then((alerts) => {
                if (mounted) setPrivacyAlertCount(alerts.length);
            })
            .catch((error) => console.error("Failed to load privacy alerts:", error));
        return () => {
            mounted = false;
        };
    }, []);

    useTauriEvent<PrivacyAlert[]>(PRIVACY_ALERTS_EVENT, (alerts) => {
        setPrivacyAlertCount(alerts.length);
    });

    useEffect(() => {
        if (privacyAlertCount > 0 && previousPrivacyAlertCount.current === 0) {
            setIsOpen(true);
        }
        previousPrivacyAlertCount.current = privacyAlertCount;
    }, [privacyAlertCount]);

    useEffect(() => {
        const closeOnEscape = (event: KeyboardEvent) => {
            if (event.key === "Escape") setIsOpen(false);
        };
        if (isOpen) window.addEventListener("keydown", closeOnEscape);
        return () => window.removeEventListener("keydown", closeOnEscape);
    }, [isOpen]);

    useEffect(() => {
        document.documentElement.setAttribute("data-theme", theme);
        localStorage.setItem(STORAGE_KEYS.theme, theme);
    }, [theme]);

    const toggleTheme = () => setTheme((current) => (current === "dark" ? "light" : "dark"));

    const handleToggleCapture = async () => {
        try {
            if (status?.is_paused) {
                await resumeCapture();
            } else {
                await pauseCapture();
            }
        } catch (error) {
            console.error("Failed to toggle capture:", error);
        }
    };

    const handleSaveProfile = async () => {
        setProfileBusy(true);
        setProfileMessage(null);
        try {
            const onboarding: OnboardingState = await getOnboardingState();
            const displayName = profileDraft.trim();
            await saveOnboardingState({ ...onboarding, display_name: displayName || null });
            setProfileName(displayName);
            setProfileDraft(displayName);
            window.dispatchEvent(
                new CustomEvent("fndr-profile-updated", { detail: { displayName: displayName || null } }),
            );
            setProfileMessage("Saved");
        } catch (error) {
            setProfileMessage(`Failed to save: ${String(error)}`);
        } finally {
            setProfileBusy(false);
            window.setTimeout(() => setProfileMessage(null), 1400);
        }
    };

    const handleAddBlocklist = async () => {
        const value = newApp.trim();
        if (!value) return;
        try {
            const nextBlocklist = [...blocklist, value];
            await setBlocklist(nextBlocklist);
            setBlocklistState(nextBlocklist);
            setNewApp("");
        } catch (error) {
            console.error("Failed to update blocklist:", error);
        }
    };

    const handleRemoveBlocklist = async (app: string) => {
        try {
            const nextBlocklist = blocklist.filter((entry) => entry !== app);
            await setBlocklist(nextBlocklist);
            setBlocklistState(nextBlocklist);
        } catch (error) {
            console.error("Failed to update blocklist:", error);
        }
    };

    const stored = status?.pipeline.stored_total ?? qualityStatus?.stored_count ?? 0;
    const skipped = status?.pipeline.skipped_total ?? qualityStatus?.dropped_count ?? 0;
    const needsSignal = qualityStatus?.flagged_count ?? 0;

    return (
        <div className="control-panel-container">
            <div className="control-panel-actions fndr-os-chrome-row">
                <button
                    type="button"
                    className="fndr-os-chrome-btn"
                    onClick={toggleTheme}
                    aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} mode`}
                    title="Toggle theme"
                >
                    {theme === "dark" ? "☾" : "☀"}
                </button>
                <button
                    type="button"
                    className="fndr-os-chrome-btn control-panel-settings-btn"
                    onClick={() => setIsOpen((open) => !open)}
                    aria-label={privacyAlertCount > 0 ? `Open settings, ${privacyAlertCount} privacy alert${privacyAlertCount === 1 ? "" : "s"}` : "Open settings"}
                    title="Open settings"
                >
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                        <circle cx="12" cy="12" r="3" />
                        <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.86l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.7 1.7 0 0 0-1.86-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 0 1-4 0v-.09a1.7 1.7 0 0 0-1-1.55 1.7 1.7 0 0 0-1.86.34l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.86 1.7 1.7 0 0 0-1.55-1H3a2 2 0 0 1 0-4h.09a1.7 1.7 0 0 0 1.55-1 1.7 1.7 0 0 0-.34-1.86l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.7 1.7 0 0 0 1.86.34 1.7 1.7 0 0 0 1-1.55V3a2 2 0 0 1 4 0v.09a1.7 1.7 0 0 0 1 1.55 1.7 1.7 0 0 0 1.86-.34l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.86 1.7 1.7 0 0 0 1.55 1H21a2 2 0 0 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1Z" />
                    </svg>
                    {privacyAlertCount > 0 && <span className="privacy-badge">{privacyAlertCount}</span>}
                </button>
            </div>

            {isOpen && <div className="panel-backdrop" onClick={() => setIsOpen(false)} />}
            <aside className={`settings-panel ${isOpen ? "open" : ""}`} aria-label="FNDR settings">
                <header className="panel-header">
                    <div>
                        <h2>FNDR Settings</h2>
                        <p className="panel-subtitle">Private, local, always in your control.</p>
                    </div>
                    <button className="ui-action-btn panel-close" onClick={() => setIsOpen(false)} aria-label="Close">X</button>
                </header>

                <div className="panel-content">
                    <section className="panel-section">
                        <h3>Profile</h3>
                        <p className="section-hint">FNDR uses this name in your greeting.</p>
                        <div className="profile-row">
                            <input
                                type="text"
                                value={profileDraft}
                                onChange={(event) => setProfileDraft(event.target.value)}
                                placeholder="Your name"
                                className="profile-input"
                                onKeyDown={(event) => {
                                    if (event.key === "Enter") void handleSaveProfile();
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
                        {profileMessage && <p className="profile-msg">{profileMessage}</p>}
                    </section>

                    <section className="panel-section">
                        <h3>Capture Status</h3>
                        <button className={`ui-action-btn capture-toggle ${status?.is_paused ? "paused" : "active"}`} onClick={() => void handleToggleCapture()}>
                            {status?.is_paused ? "Resume capture" : "Pause capture"}
                        </button>
                        <div className="capture-stats capture-stats--pipeline">
                            <span>Stored: {stored.toLocaleString()}</span>
                            <span>Skipped: {skipped.toLocaleString()}</span>
                            <span>Needs signal: {needsSignal.toLocaleString()}</span>
                        </div>
                    </section>

                    <section className="panel-section">
                        <PrivacyPanel
                            isVisible={true}
                            onClose={() => undefined}
                            onAlertsChange={setPrivacyAlertCount}
                            onBlocklistChange={setBlocklistState}
                            embedded={true}
                        />
                    </section>

                    <section className="panel-section">
                        <h3>Blocked Apps &amp; Sites</h3>
                        <p className="section-hint">These apps and websites will not be captured.</p>
                        <div className="blocklist">
                            {blocklist.length === 0 ? (
                                <p className="blocklist-empty">No apps or sites blocked</p>
                            ) : (
                                blocklist.map((app) => (
                                    <div key={app} className="blocklist-item">
                                        <span>{app}</span>
                                        <button onClick={() => void handleRemoveBlocklist(app)} aria-label={`Remove ${app}`}>x</button>
                                    </div>
                                ))
                            )}
                        </div>
                        <div className="add-app-row">
                            <input
                                type="text"
                                placeholder="Add app name or site..."
                                value={newApp}
                                onChange={(event) => setNewApp(event.target.value)}
                                onKeyDown={(event) => {
                                    if (event.key === "Enter") void handleAddBlocklist();
                                }}
                                className="add-app-input"
                            />
                            <button onClick={() => void handleAddBlocklist()} className="ui-action-btn btn-primary">Add</button>
                        </div>
                    </section>

                    <section className="panel-section">
                        <h3>Local Models</h3>
                        <p className="section-hint">Models remain on this Mac and load only when FNDR needs them.</p>
                        {status?.embedding_backend === "unavailable" && (
                            <p className="model-error">Capture is paused until the embedding model is available, so memories are never stored with unusable embeddings.</p>
                        )}
                        <ul className="model-readiness-list">
                            {models.filter((model) => model.download_url === "already_downloaded").map((model) => (
                                <li key={model.id}><strong>{model.name}</strong> <span>Ready</span></li>
                            ))}
                            {models.length === 0 && <li>Checking local model readiness…</li>}
                        </ul>
                    </section>
                </div>
            </aside>
        </div>
    );
}
