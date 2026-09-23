import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
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
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import { STORAGE_KEYS } from "@/shared/utils/config";
import {
    applyPalette,
    listPalettes,
    PALETTES,
    resolveStoredPalette,
    type PaletteKey,
} from "@/shared/theme/cinematic-palettes";
import { PanelHeader } from "@/shared/components/PanelHeader";
import { SegmentedControl } from "@/shared/components/SegmentedControl";
import { PrivacyPanel } from "./PrivacyPanel";
import "./ControlPanel.css";

interface ControlPanelProps {
    status: CaptureStatus | null;
    compact?: boolean;
    evalUi?: boolean;
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
    const [settingsError, setSettingsError] = useState<string | null>(null);
    const [capturePaused, setCapturePaused] = useState(status?.is_paused ?? false);
    const [captureBusy, setCaptureBusy] = useState(false);
    const [captureMessage, setCaptureMessage] = useState<string | null>(null);
    const [blocklistBusy, setBlocklistBusy] = useState(false);
    const [blocklistMessage, setBlocklistMessage] = useState<string | null>(null);
    const settingsButtonRef = useRef<HTMLButtonElement>(null);
    const settingsPanelRef = useRef<HTMLElement>(null);
    const closeButtonRef = useRef<HTMLButtonElement>(null);
    const settingsWasOpen = useRef(false);

    const loadSettings = useCallback(async () => {
        setSettingsError(null);
        const results = await Promise.allSettled([
            getBlocklist(),
            getOnboardingState(),
            listAvailableModels(),
            fndrQualityStatus(),
        ] as const);
        const [blocklistResult, onboardingResult, modelsResult, qualityResult] = results;
        const unavailable: string[] = [];

        if (blocklistResult.status === "fulfilled") {
            setBlocklistState(blocklistResult.value);
        } else {
            unavailable.push("blocked apps and sites");
        }
        if (onboardingResult.status === "fulfilled") {
            const name = onboardingResult.value.display_name ?? "";
            setProfileName(name);
            setProfileDraft(name);
        } else {
            unavailable.push("profile");
        }
        if (modelsResult.status === "fulfilled") {
            setModels(modelsResult.value);
        } else {
            unavailable.push("local models");
        }
        if (qualityResult.status === "fulfilled") {
            setQualityStatus(qualityResult.value);
        } else {
            unavailable.push("capture totals");
        }

        if (unavailable.length > 0) {
            setSettingsError(`Some settings could not be loaded: ${unavailable.join(", ")}.`);
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
        setCapturePaused(status?.is_paused ?? false);
    }, [status?.is_paused]);

    useEffect(() => {
        if (!isOpen) {
            if (settingsWasOpen.current) {
                settingsButtonRef.current?.focus();
                settingsWasOpen.current = false;
            }
            return;
        }

        settingsWasOpen.current = true;
        const focusFrame = window.requestAnimationFrame(() => closeButtonRef.current?.focus());
        const keepFocusInDrawer = (event: KeyboardEvent) => {
            if (event.key === "Escape") {
                event.preventDefault();
                event.stopPropagation();
                setIsOpen(false);
                return;
            }
            if (event.key !== "Tab") return;

            const focusable = Array.from(
                settingsPanelRef.current?.querySelectorAll<HTMLElement>(
                    'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
                ) ?? [],
            ).filter((element) => element.getAttribute("aria-hidden") !== "true");
            if (focusable.length === 0) return;

            const first = focusable[0];
            const last = focusable[focusable.length - 1];
            if (!settingsPanelRef.current?.contains(document.activeElement)) {
                event.preventDefault();
                (event.shiftKey ? last : first).focus();
            } else if (event.shiftKey && document.activeElement === first) {
                event.preventDefault();
                last.focus();
            } else if (!event.shiftKey && document.activeElement === last) {
                event.preventDefault();
                first.focus();
            }
        };

        document.addEventListener("keydown", keepFocusInDrawer, true);
        return () => {
            window.cancelAnimationFrame(focusFrame);
            document.removeEventListener("keydown", keepFocusInDrawer, true);
        };
    }, [isOpen]);

    const [palette, setPalette] = useState<PaletteKey>(() =>
        resolveStoredPalette(localStorage.getItem(STORAGE_KEYS.palette)),
    );

    useEffect(() => {
        document.documentElement.setAttribute("data-theme", theme);
        localStorage.setItem(STORAGE_KEYS.theme, theme);
        localStorage.setItem(STORAGE_KEYS.palette, palette);
        applyPalette(palette, theme);
        window.dispatchEvent(
            new CustomEvent("fndr-appearance-changed", {
                detail: { palette, mode: theme },
            }),
        );
    }, [theme, palette]);

    const toggleTheme = () => setTheme((current) => (current === "dark" ? "light" : "dark"));

    const toggleSettings = () => {
        if (isOpen) {
            setIsOpen(false);
            return;
        }
        setIsOpen(true);
        window.setTimeout(() => closeButtonRef.current?.focus(), 0);
    };

    const handleToggleCapture = async () => {
        if (!status || captureBusy) return;
        setCaptureBusy(true);
        setCaptureMessage(null);
        try {
            if (capturePaused) {
                await resumeCapture();
                setCapturePaused(false);
                setCaptureMessage("Capture resumed. New screen context can be processed locally.");
            } else {
                await pauseCapture();
                setCapturePaused(true);
                setCaptureMessage("Capture paused. It will remain paused after relaunch.");
            }
        } catch (error) {
            setCaptureMessage(`Capture action failed: ${String(error)}`);
        } finally {
            setCaptureBusy(false);
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
            setProfileMessage("Profile saved.");
        } catch (error) {
            setProfileMessage(`Profile save failed: ${String(error)}`);
        } finally {
            setProfileBusy(false);
        }
    };

    const handleAddBlocklist = async () => {
        const value = newApp.trim();
        if (!value || blocklistBusy) return;
        if (blocklist.some((entry) => entry.toLocaleLowerCase() === value.toLocaleLowerCase())) {
            setBlocklistMessage(`${value} is already blocked.`);
            return;
        }
        setBlocklistBusy(true);
        setBlocklistMessage(null);
        try {
            const nextBlocklist = [...blocklist, value];
            await setBlocklist(nextBlocklist);
            setBlocklistState(nextBlocklist);
            setNewApp("");
            setBlocklistMessage(`${value} added to the blocklist.`);
        } catch (error) {
            setBlocklistMessage(`Blocklist update failed: ${String(error)}`);
        } finally {
            setBlocklistBusy(false);
        }
    };

    const handleRemoveBlocklist = async (app: string) => {
        if (blocklistBusy) return;
        setBlocklistBusy(true);
        setBlocklistMessage(null);
        try {
            const nextBlocklist = blocklist.filter((entry) => entry !== app);
            await setBlocklist(nextBlocklist);
            setBlocklistState(nextBlocklist);
            setBlocklistMessage(`${app} removed from the blocklist.`);
        } catch (error) {
            setBlocklistMessage(`Blocklist update failed: ${String(error)}`);
        } finally {
            setBlocklistBusy(false);
        }
    };

    const stored = status?.pipeline.stored_total ?? qualityStatus?.stored_count ?? 0;
    const skipped = status?.pipeline.skipped_total ?? qualityStatus?.dropped_count ?? 0;
    const readyModels = models.filter((model) => model.download_url === "already_downloaded");

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
                    ref={settingsButtonRef}
                    type="button"
                    className="fndr-os-chrome-btn control-panel-settings-btn"
                    onClick={toggleSettings}
                    aria-label={privacyAlertCount > 0 ? `Open settings, ${privacyAlertCount} privacy alert${privacyAlertCount === 1 ? "" : "s"}` : "Open settings"}
                    aria-expanded={isOpen}
                    aria-controls="fndr-settings-panel"
                    title="Open settings"
                >
                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" aria-hidden="true">
                        <circle cx="12" cy="12" r="3" />
                        <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.86l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.7 1.7 0 0 0-1.86-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 0 1-4 0v-.09a1.7 1.7 0 0 0-1-1.55 1.7 1.7 0 0 0-1.86.34l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.86 1.7 1.7 0 0 0-1.55-1H3a2 2 0 0 1 0-4h.09a1.7 1.7 0 0 0 1.55-1 1.7 1.7 0 0 0-.34-1.86l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.7 1.7 0 0 0 1.86.34 1.7 1.7 0 0 0 1-1.55V3a2 2 0 0 1 4 0v.09a1.7 1.7 0 0 0 1 1.55 1.7 1.7 0 0 0 1.86-.34l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.86 1.7 1.7 0 0 0 1.55 1H21a2 2 0 0 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1Z" />
                    </svg>
                    {privacyAlertCount > 0 && <span className="privacy-badge">{privacyAlertCount}</span>}
                </button>
            </div>

            {typeof document !== "undefined" && isOpen && createPortal(
                <>
                    <div className="panel-backdrop" onClick={() => setIsOpen(false)} />
                    <aside
                        ref={settingsPanelRef}
                        id="fndr-settings-panel"
                        className="settings-panel open"
                        role="dialog"
                        aria-modal="true"
                        aria-labelledby="fndr-settings-title"
                    >
                        <PanelHeader
                            title="Settings"
                            titleId="fndr-settings-title"
                            subtitle="Local-first controls and current capability status."
                            closeLabel="Close settings"
                            closeRef={closeButtonRef}
                            onClose={() => setIsOpen(false)}
                        />

                        <div className="panel-content">
                            {settingsError && (
                                <p className="settings-message settings-message--error" role="alert">
                                    {settingsError}
                                </p>
                            )}

                            <section className="panel-section" aria-labelledby="settings-profile-title">
                                <h3 id="settings-profile-title">Profile</h3>
                                <p className="section-hint">Who FNDR greets on the Home screen.</p>
                                <label className="settings-field-label" htmlFor="fndr-profile-name">Display name</label>
                                <div className="profile-row">
                                    <input
                                        id="fndr-profile-name"
                                        type="text"
                                        value={profileDraft}
                                        onChange={(event) => {
                                            setProfileDraft(event.target.value);
                                            setProfileMessage(null);
                                        }}
                                        autoComplete="name"
                                        className="profile-input"
                                        onKeyDown={(event) => {
                                            if (
                                                event.key === "Enter"
                                                && !profileBusy
                                                && profileDraft.trim() !== profileName.trim()
                                            ) {
                                                void handleSaveProfile();
                                            }
                                        }}
                                    />
                                    <button
                                        type="button"
                                        className="ui-action-btn btn-secondary"
                                        onClick={() => void handleSaveProfile()}
                                        disabled={profileBusy || profileDraft.trim() === profileName.trim()}
                                    >
                                        {profileBusy ? "Saving…" : "Save"}
                                    </button>
                                </div>
                                {profileMessage && (
                                    <p
                                        className="profile-msg"
                                        role={profileMessage.startsWith("Profile save failed") ? "alert" : "status"}
                                    >
                                        {profileMessage}
                                    </p>
                                )}
                            </section>

                            <section className="panel-section" aria-labelledby="settings-appearance-title">
                                <h3 id="settings-appearance-title">Appearance</h3>
                                <SegmentedControl
                                    ariaLabel="Theme"
                                    value={theme}
                                    onChange={setTheme}
                                    options={[
                                        { value: "light", label: "Light" },
                                        { value: "dark", label: "Dark" },
                                    ]}
                                />
                                <div className="palette-list" role="radiogroup" aria-label="Color palette">
                                    {listPalettes().map((key) => {
                                        const option = PALETTES[key];
                                        const selected = key === palette;
                                        return (
                                            <button
                                                key={key}
                                                type="button"
                                                role="radio"
                                                aria-checked={selected}
                                                className={`palette-option${selected ? " is-selected" : ""}`}
                                                onClick={() => setPalette(key)}
                                            >
                                                <span className="palette-swatch" aria-hidden="true">
                                                    <span style={{ background: option[theme].bg }} />
                                                    <span style={{ background: option[theme].surfaceRaised }} />
                                                    <span style={{ background: option[theme].accent }} />
                                                </span>
                                                <span className="palette-name">{option.name}</span>
                                            </button>
                                        );
                                    })}
                                </div>
                            </section>

                            <section className="panel-section" aria-labelledby="settings-capture-title">
                                <h3 id="settings-capture-title">Capture</h3>
                                <p className="section-hint">
                                    Control when FNDR may process new screen context on this Mac.
                                </p>
                                <button
                                    type="button"
                                    className={`ui-action-btn capture-toggle ${capturePaused ? "is-paused" : "is-capturing"}`}
                                    onClick={() => void handleToggleCapture()}
                                    disabled={!status || captureBusy}
                                >
                                    {!status
                                        ? "Checking capture status…"
                                        : captureBusy
                                            ? "Updating…"
                                            : capturePaused
                                                ? "Resume capture"
                                                : "Pause capture"}
                                </button>
                                {captureMessage && (
                                    <p
                                        className={`settings-message ${captureMessage.startsWith("Capture action failed") ? "settings-message--error" : ""}`}
                                        role={captureMessage.startsWith("Capture action failed") ? "alert" : "status"}
                                        aria-label={captureMessage.startsWith("Capture action failed") ? "Capture action failed" : undefined}
                                    >
                                        {captureMessage}
                                    </p>
                                )}
                                <div className="capture-stats capture-stats--pipeline" aria-label="This session">
                                    <span>Stored this session: {stored.toLocaleString()}</span>
                                    <span>Skipped this session: {skipped.toLocaleString()}</span>
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

                            <section className="panel-section" aria-labelledby="settings-blocklist-title">
                                <h3 id="settings-blocklist-title">Blocked apps &amp; sites</h3>
                                <p className="section-hint">
                                    Matching apps and websites are excluded from future capture.
                                </p>
                                <div className="blocklist">
                                    {blocklist.length === 0 ? (
                                        <p className="blocklist-empty">No custom apps or sites are blocked.</p>
                                    ) : (
                                        blocklist.map((app) => (
                                            <div key={app} className="blocklist-item">
                                                <span>{app}</span>
                                                <button
                                                    type="button"
                                                    onClick={() => void handleRemoveBlocklist(app)}
                                                    aria-label={`Remove ${app} from blocklist`}
                                                    disabled={blocklistBusy}
                                                >
                                                    <span aria-hidden="true">×</span>
                                                </button>
                                            </div>
                                        ))
                                    )}
                                </div>
                                <label className="settings-field-label" htmlFor="fndr-blocklist-entry">
                                    App or website
                                </label>
                                <div className="add-app-row">
                                    <input
                                        id="fndr-blocklist-entry"
                                        type="text"
                                        placeholder="Example: 1Password or bank.example"
                                        value={newApp}
                                        onChange={(event) => {
                                            setNewApp(event.target.value);
                                            setBlocklistMessage(null);
                                        }}
                                        onKeyDown={(event) => {
                                            if (event.key === "Enter" && newApp.trim() && !blocklistBusy) {
                                                void handleAddBlocklist();
                                            }
                                        }}
                                        className="add-app-input"
                                    />
                                    <button
                                        type="button"
                                        onClick={() => void handleAddBlocklist()}
                                        className="ui-action-btn btn-primary"
                                        disabled={!newApp.trim() || blocklistBusy}
                                    >
                                        {blocklistBusy ? "Updating…" : "Add"}
                                    </button>
                                </div>
                                {blocklistMessage && (
                                    <p
                                        className={`settings-message ${blocklistMessage.startsWith("Blocklist update failed") ? "settings-message--error" : ""}`}
                                        role={blocklistMessage.startsWith("Blocklist update failed") ? "alert" : "status"}
                                        aria-label={blocklistMessage.startsWith("Blocklist update failed") ? "Blocklist update failed" : undefined}
                                    >
                                        {blocklistMessage}
                                    </p>
                                )}
                            </section>

                            <section className="panel-section" aria-labelledby="settings-models-title">
                                <h3 id="settings-models-title">Local models</h3>
                                <p className="section-hint">
                                    What intelligence is available locally, and why a feature may be limited.
                                </p>
                                {status?.embedding_backend === "unavailable" && (
                                    <p className="model-error" role="alert">
                                        Capture is paused until the embedding model is available, so FNDR does not store memories with unusable embeddings.
                                    </p>
                                )}
                                <ul className="model-readiness-list">
                                    {readyModels.map((model) => (
                                        <li key={model.id}><strong>{model.name}</strong> <span>Ready</span></li>
                                    ))}
                                    {models.length === 0 && <li>Checking local model readiness…</li>}
                                    {models.length > 0 && readyModels.length === 0 && (
                                        <li>No local models are ready. Finish model setup to enable AI features.</li>
                                    )}
                                </ul>
                            </section>
                        </div>
                    </aside>
                </>,
                document.body,
            )}
        </div>
    );
}
