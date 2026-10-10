import { OperatorPermissions } from "./OperatorPermissions";
import { VoiceOutputSection } from "@/shared/voice/VoiceOutputSection";
import { type KeyboardEvent as ReactKeyboardEvent, useEffect, useRef, useState } from "react";
import {
    armScreenGuideDiagnostic,
    deleteScreenGuideDiagnostics,
    getScreenGuideDiagnosticStatus,
    revealScreenGuideDiagnostics,
    type ScreenGuideDiagnosticStatus,
    type ScreenGuideSettings,
    type ScreenGuideStateEvent,
    getScreenGuideSettings,
    onScreenGuideState,
    screenGuidePress,
    screenGuideRelease,
    setScreenGuideSettings,
    submitScreenGuideText,
} from "@/shared/ipc/tauri";
import {
    finishScreenGuideActivity,
    recordScreenGuideActivity,
    screenGuideErrorMessage,
} from "./screenGuideState";
import "./ScreenGuidePanel.css";
import { PanelHeader } from "@/shared/components/PanelHeader";
import { SegmentedControl } from "@/shared/components/SegmentedControl";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import type { ActivityTraceSnapshot } from "@/shared/activity/activityTrace";
import { formatBytes } from "@/shared/utils/format";
import {
    computerUseStatus,
    setComputerUseEnabled,
    openClickyBridgeStatus,
    type ComputerUseStatus,
    type OpenClickyBridgeStatus,
} from "@/shared/ipc/tauri";

interface ScreenGuidePanelProps {
    isVisible: boolean;
    isPrivateMode?: boolean;
    onClose: () => void;
}

const IDLE_STATUS: ScreenGuideStateEvent = { phase: "idle", message: null, generation: 0 };

function statusCopy(
    settings: ScreenGuideSettings | null,
    status: ScreenGuideStateEvent,
    loading: boolean,
    isPrivateMode: boolean,
): string {
    if (loading) return "Loading Screen Guide…";
    if (!settings?.enabled) return "Screen Guide is off";
    if (isPrivateMode) return "FNDR Private Mode is on";
    if (status.message) return status.message;
    switch (status.phase) {
        case "listening":
            return "Listening…";
        case "transcribing":
            return "Transcribing on this Mac…";
        case "thinking":
            return "Finding the answer on this Mac…";
        case "answer":
            return "Answer ready";
        case "error":
            return "Screen Guide needs attention";
        case "idle":
            return "Enabled";
    }
}

export function ScreenGuidePanel({
    isVisible,
    isPrivateMode = false,
    onClose,
}: ScreenGuidePanelProps) {
    const [settings, setSettings] = useState<ScreenGuideSettings | null>(null);
    const [shortcutDraft, setShortcutDraft] = useState("");
    const [status, setStatus] = useState<ScreenGuideStateEvent>(IDLE_STATUS);
    const [activityTrace, setActivityTrace] = useState<ActivityTraceSnapshot | null>(null);
    const [question, setQuestion] = useState("");
    const [loading, setLoading] = useState(false);
    const [saving, setSaving] = useState(false);
    const [submitting, setSubmitting] = useState(false);
    const [holding, setHolding] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [loadAttempt, setLoadAttempt] = useState(0);
    const [listenerAttempt, setListenerAttempt] = useState(0);
    const [liveStatusError, setLiveStatusError] = useState<string | null>(null);
    const [diagnosticStatus, setDiagnosticStatus] = useState<ScreenGuideDiagnosticStatus | null>(null);
    const [diagnosticBusy, setDiagnosticBusy] = useState(false);
    const [diagnosticError, setDiagnosticError] = useState<string | null>(null);
    const holdingRef = useRef(false);
    const pressPromiseRef = useRef<Promise<number> | null>(null);
    const closeButtonRef = useRef<HTMLButtonElement | null>(null);
    const invokerRef = useRef<HTMLElement | null>(null);

    useEffect(() => {
        if (!isVisible) return;

        let active = true;
        setLoading(true);
        setSettings(null);
        setError(null);

        void getScreenGuideSettings()
            .then((nextSettings) => {
                if (active) setSettings(nextSettings);
            })
            .catch((reason: unknown) => {
                if (active) {
                    setError(screenGuideErrorMessage(reason, "Screen Guide is unavailable."));
                }
            })
            .finally(() => {
                if (active) setLoading(false);
            });

        return () => {
            active = false;
            setHolding(false);
            const pressed = pressPromiseRef.current;
            pressPromiseRef.current = null;
            if (holdingRef.current) {
                holdingRef.current = false;
                if (pressed) {
                    void pressed
                        .then((generation) => screenGuideRelease(generation))
                        .catch(() => undefined);
                }
            }
        };
    }, [isVisible, loadAttempt]);

    useEffect(() => {
        if (!isVisible) return;
        let active = true;
        setDiagnosticError(null);
        void getScreenGuideDiagnosticStatus()
            .then((nextStatus) => {
                if (active) setDiagnosticStatus(nextStatus);
            })
            .catch((reason: unknown) => {
                if (active) {
                    setDiagnosticError(
                        screenGuideErrorMessage(reason, "Diagnostic storage is unavailable."),
                    );
                }
            });
        return () => {
            active = false;
        };
    }, [isVisible, isPrivateMode, status.phase]);

    useEffect(() => {
        if (
            !isVisible
            || !diagnosticStatus?.armed
            || diagnosticStatus.expiresInMs === null
        ) {
            return;
        }

        let active = true;
        const refreshAtExpiry = window.setTimeout(() => {
            void getScreenGuideDiagnosticStatus()
                .then((nextStatus) => {
                    if (active) setDiagnosticStatus(nextStatus);
                })
                .catch((reason: unknown) => {
                    if (active) {
                        setDiagnosticError(
                            screenGuideErrorMessage(
                                reason,
                                "Could not refresh diagnostic status.",
                            ),
                        );
                    }
                });
        }, Math.max(1, diagnosticStatus.expiresInMs));

        return () => {
            active = false;
            window.clearTimeout(refreshAtExpiry);
        };
    }, [diagnosticStatus?.armed, diagnosticStatus?.expiresInMs, isVisible]);

    useEffect(() => {
        let active = true;
        let unlisten: (() => void) | null = null;
        setLiveStatusError(null);

        void onScreenGuideState((nextStatus) => {
            if (!active) return;
            setStatus(nextStatus);
            const activityStage = nextStatus.activity_stage;
            if (activityStage) {
                setActivityTrace((current) => recordScreenGuideActivity(current, {
                    stage: activityStage,
                    targetApp: nextStatus.target_app?.trim() || null,
                    generation: nextStatus.generation,
                    atMs: Date.now(),
                }));
            } else if (nextStatus.phase === "idle") {
                setActivityTrace(null);
            } else if (nextStatus.phase === "error") {
                setActivityTrace((current) =>
                    finishScreenGuideActivity(current, "failed", Date.now()));
            } else if (nextStatus.phase === "answer") {
                setActivityTrace((current) =>
                    finishScreenGuideActivity(current, "completed", Date.now()));
            }
        })
            .then((dispose) => {
                if (active) unlisten = dispose;
                else dispose();
            })
            .catch((reason: unknown) => {
                if (active) {
                    setLiveStatusError(
                        screenGuideErrorMessage(
                            reason,
                            "Live activity updates are unavailable.",
                        ),
                    );
                }
            });

        return () => {
            active = false;
            unlisten?.();
        };
    }, [listenerAttempt]);

    useEffect(() => {
        if (!isVisible) return;
        const activeElement = document.activeElement;
        invokerRef.current = activeElement instanceof HTMLElement ? activeElement : null;
        closeButtonRef.current?.focus();

        return () => {
            const invoker = invokerRef.current;
            invokerRef.current = null;
            if (invoker?.isConnected) invoker.focus();
        };
    }, [isVisible]);

    useEffect(() => {
        setShortcutDraft(settings?.shortcut ?? "");
    }, [settings?.shortcut]);

    const usesChatGpt = settings?.model === "codex";
    const [openClicky, setOpenClicky] = useState<OpenClickyBridgeStatus | null>(null);
    const [computerUse, setComputerUse] = useState<ComputerUseStatus | null>(null);
    useEffect(() => {
        if (!isVisible) return;
        let active = true;
        computerUseStatus()
            .then((status) => active && setComputerUse(status))
            .catch(() => active && setComputerUse(null));
        return () => {
            active = false;
        };
    }, [isVisible]);
    useEffect(() => {
        if (!isVisible || !settings?.openclicky_bridge) return;
        let active = true;
        openClickyBridgeStatus()
            .then((status) => active && setOpenClicky(status))
            .catch(() => active && setOpenClicky(null));
        return () => {
            active = false;
        };
    }, [isVisible, settings?.openclicky_bridge]);

    if (!isVisible) return null;

    const enabled = settings?.enabled ?? false;
    const controlsDisabled = loading || saving || !settings;
    const questionDisabled = controlsDisabled || !enabled || isPrivateMode || submitting;
    const settingsLoadFailed = !loading && !settings && Boolean(error);
    const activityOwnsLiveStatus = activityTrace !== null && status.phase !== "idle";
    const hasDiagnosticFiles = (diagnosticStatus?.bundleCount ?? 0) > 0
        || (diagnosticStatus?.partialCount ?? 0) > 0;
    const sendsScreenshotToChatGpt = usesChatGpt && Boolean(settings?.send_screenshot_to_codex);
    const canOperateComputer = Boolean(computerUse?.enabled);
    const privacyHeading = !settings
        ? "Checking privacy and agency settings"
        : canOperateComputer
          ? usesChatGpt
              ? "Cloud answers with approval-gated control"
              : "Local answers with approval-gated control"
          : usesChatGpt
            ? sendsScreenshotToChatGpt
                ? "Cloud answer with screenshot"
                : "Cloud answer without screenshot"
            : "Local and read-only";

    const handlePanelKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
        if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            onClose();
            return;
        }
        if (event.key !== "Tab") return;

        const focusable = Array.from(
            event.currentTarget.querySelectorAll<HTMLElement>(
                'button:not([disabled]), input:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
            ),
        ).filter((element) => !element.hasAttribute("hidden"));
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (!first || !last) return;
        if (event.shiftKey && document.activeElement === first) {
            event.preventDefault();
            last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
            event.preventDefault();
            first.focus();
        }
    };

    const updateSettings = async (patch: Partial<ScreenGuideSettings>) => {
        if (!settings || saving) return;
        const previous = settings;
        const optimistic = { ...previous, ...patch };
        setSettings(optimistic);
        setSaving(true);
        setError(null);
        try {
            setSettings(await setScreenGuideSettings(optimistic));
        } catch (reason) {
            setSettings(previous);
            setError(screenGuideErrorMessage(reason, "Could not update Screen Guide."));
        } finally {
            setSaving(false);
        }
    };

    const saveShortcut = () => {
        const shortcut = shortcutDraft.trim();
        if (!shortcut || shortcut === settings?.shortcut) return;
        void updateSettings({ shortcut });
    };

    const submitQuestion = async () => {
        const text = question.trim();
        if (!text || questionDisabled) return;
        setSubmitting(true);
        setError(null);
        try {
            await submitScreenGuideText(text);
            setQuestion("");
        } catch (reason) {
            setError(screenGuideErrorMessage(reason, "Could not ask Screen Guide."));
        } finally {
            setSubmitting(false);
        }
    };

    const beginHold = () => {
        if (questionDisabled || holdingRef.current) return;
        holdingRef.current = true;
        setHolding(true);
        setError(null);
        const pressed = screenGuidePress();
        pressPromiseRef.current = pressed;
        void pressed.catch((reason: unknown) => {
            if (pressPromiseRef.current !== pressed) return;
            pressPromiseRef.current = null;
            holdingRef.current = false;
            setHolding(false);
            setError(screenGuideErrorMessage(reason, "Could not start listening."));
        });
    };

    const endHold = () => {
        if (!holdingRef.current) return;
        holdingRef.current = false;
        setHolding(false);
        const pressed = pressPromiseRef.current;
        if (!pressed) return;
        void pressed
            .then((generation) => screenGuideRelease(generation))
            .catch((reason: unknown) => {
                if (pressPromiseRef.current !== pressed) return;
                setError(screenGuideErrorMessage(reason, "Could not finish listening."));
            })
            .finally(() => {
                if (pressPromiseRef.current === pressed) {
                    pressPromiseRef.current = null;
                }
            });
    };

    const armDiagnostic = async () => {
        if (diagnosticBusy || isPrivateMode) return;
        setDiagnosticBusy(true);
        setDiagnosticError(null);
        try {
            setDiagnosticStatus(await armScreenGuideDiagnostic());
        } catch (reason) {
            setDiagnosticError(
                screenGuideErrorMessage(reason, "Could not arm the next diagnostic turn."),
            );
        } finally {
            setDiagnosticBusy(false);
        }
    };

    const deleteDiagnostics = async () => {
        if (diagnosticBusy) return;
        setDiagnosticBusy(true);
        setDiagnosticError(null);
        try {
            setDiagnosticStatus(await deleteScreenGuideDiagnostics());
        } catch (reason) {
            setDiagnosticError(
                screenGuideErrorMessage(reason, "Could not delete Screen Guide diagnostics."),
            );
        } finally {
            setDiagnosticBusy(false);
        }
    };

    const revealDiagnostics = async () => {
        if (diagnosticBusy || !hasDiagnosticFiles) return;
        setDiagnosticBusy(true);
        setDiagnosticError(null);
        try {
            setDiagnosticStatus(await revealScreenGuideDiagnostics());
        } catch (reason) {
            setDiagnosticError(
                screenGuideErrorMessage(reason, "Could not reveal Screen Guide diagnostics."),
            );
        } finally {
            setDiagnosticBusy(false);
        }
    };

    return (
        <div
            className="sg-panel-page"
            role="dialog"
            aria-modal="true"
            aria-labelledby="sg-panel-title"
            aria-describedby="sg-panel-description"
            aria-busy={loading || saving || submitting}
            onKeyDown={handlePanelKeyDown}
        >
            <PanelHeader
                title="Screen Guide"
                titleId="sg-panel-title"
                subtitle="Ask about your current main display or explicitly find a file by name. FNDR can point you toward a next step without clicking or typing for you."
                subtitleId="sg-panel-description"
                closeLabel="Close Screen Guide"
                closeRef={closeButtonRef}
                onClose={onClose}
            />

            <div className="sg-panel-body">
                <section
                    className={`sg-readiness ${enabled && !isPrivateMode ? "is-ready" : "is-off"}`}
                    role={activityOwnsLiveStatus ? undefined : "status"}
                    aria-live={activityOwnsLiveStatus ? undefined : "polite"}
                    aria-atomic={activityOwnsLiveStatus ? undefined : "true"}
                >
                    <span className="sg-readiness-dot" aria-hidden="true" />
                    <div>
                        <strong>{statusCopy(settings, status, loading, isPrivateMode)}</strong>
                        <span>
                            {isPrivateMode
                                ? "Exit Private Mode in Settings → Capture to use Screen Guide"
                                : enabled
                                ? `Hold ${settings?.shortcut ?? "Control+Alt+Space"} from any app`
                                : `${settings?.shortcut ?? "Control+Alt+Space"} is available when enabled`}
                        </span>
                    </div>
                </section>

                {error && (
                    <div className="sg-panel-error" role="alert">
                        <p>{error}</p>
                        {settingsLoadFailed && (
                            <button
                                type="button"
                                className="ui-action-btn"
                                onClick={() => setLoadAttempt((attempt) => attempt + 1)}
                                aria-label="Retry loading Screen Guide"
                            >
                                Retry
                            </button>
                        )}
                    </div>
                )}

                {activityTrace && status.phase !== "idle" && (
                    <ActivityTrace
                        trace={activityTrace}
                        className="sg-activity-status"
                    />
                )}

                {liveStatusError && (
                    <aside className="sg-status-warning" aria-label="Live status unavailable">
                        <div>
                            <strong>Live activity updates are unavailable.</strong>
                            <p>
                                Settings still work, but this panel may not show the current listening
                                or answer state.
                            </p>
                        </div>
                        <button
                            type="button"
                            className="ui-action-btn"
                            onClick={() => setListenerAttempt((attempt) => attempt + 1)}
                            aria-label="Retry Screen Guide live status"
                        >
                            Retry
                        </button>
                    </aside>
                )}

                <section className="sg-settings-card" aria-label="Screen Guide settings">
                    <label className="sg-setting-row">
                        <span>
                            <strong>Enable Screen Guide</strong>
                            <small>Keep FNDR ready beside the notch for hold-to-talk</small>
                        </span>
                        <input
                            type="checkbox"
                            role="switch"
                            className="fndr-switch"
                            aria-label="Enable Screen Guide"
                            checked={enabled}
                            disabled={controlsDisabled}
                            onChange={(event) => void updateSettings({ enabled: event.target.checked })}
                        />
                    </label>
                    <div className="sg-shortcut-row">
                        <label htmlFor="sg-shortcut">
                            <strong>Shortcut</strong>
                            <small>Enter a chord such as Control+Alt+Space</small>
                        </label>
                        <div>
                            <input
                                id="sg-shortcut"
                                type="text"
                                aria-label="Screen Guide shortcut"
                                value={shortcutDraft}
                                disabled={controlsDisabled}
                                spellCheck={false}
                                onChange={(event) => setShortcutDraft(event.target.value)}
                                onKeyDown={(event) => {
                                    if (event.key === "Enter") {
                                        event.preventDefault();
                                        saveShortcut();
                                    }
                                }}
                            />
                            <button
                                type="button"
                                className="ui-action-btn"
                                aria-label="Save shortcut"
                                disabled={
                                    controlsDisabled ||
                                    !shortcutDraft.trim() ||
                                    shortcutDraft.trim() === settings?.shortcut
                                }
                                onClick={saveShortcut}
                            >
                                {saving ? "Saving…" : "Save"}
                            </button>
                        </div>
                    </div>
                    <label className="sg-setting-row">
                        <span>
                            <strong>Speak responses</strong>
                            <small>Read the answer aloud with the system voice</small>
                        </span>
                        <input
                            type="checkbox"
                            role="switch"
                            className="fndr-switch"
                            aria-label="Speak responses"
                            checked={settings?.speak_responses ?? false}
                            disabled={controlsDisabled}
                            onChange={(event) =>
                                void updateSettings({ speak_responses: event.target.checked })
                            }
                        />
                    </label>
                    <label className="sg-setting-row">
                        <span>
                            <strong>Show guidance cursor</strong>
                            <small>Point to the relevant place without taking control</small>
                        </span>
                        <input
                            type="checkbox"
                            role="switch"
                            className="fndr-switch"
                            aria-label="Show guidance cursor"
                            checked={settings?.show_cursor ?? false}
                            disabled={controlsDisabled}
                            onChange={(event) =>
                                void updateSettings({ show_cursor: event.target.checked })
                            }
                        />
                    </label>
                </section>

                <section className="sg-settings-card" aria-labelledby="sg-model-title">
                    <div className="sg-setting-row sg-model-row">
                        <span>
                            <strong id="sg-model-title">Answer with</strong>
                            <small>
                                {usesChatGpt
                                    ? "Your question and the text on screen go to OpenAI under your ChatGPT plan."
                                    : "The on-device model. Nothing leaves this Mac."}
                            </small>
                        </span>
                        <SegmentedControl
                            className="sg-model-toggle"
                            ariaLabel="Screen Guide model"
                            value={settings?.model ?? "local"}
                            onChange={(model) => void updateSettings({
                                model,
                                send_screenshot_to_codex: model === "codex" ? settings?.send_screenshot_to_codex : false,
                            })}
                            options={[
                                { value: "local", label: "On-device" },
                                { value: "codex", label: "ChatGPT" },
                            ]}
                        />
                    </div>
                    {usesChatGpt && (
                        <label className="sg-setting-row">
                            <span>
                                <strong>Include a screenshot</strong>
                                <small>
                                    Also sends a downscaled image of your main display. Better guidance, but
                                    everything visible leaves the Mac for that question.
                                </small>
                            </span>
                            <input
                                type="checkbox"
                                role="switch"
                                className="fndr-switch"
                                aria-label="Include a screenshot"
                                checked={settings?.send_screenshot_to_codex ?? false}
                                disabled={controlsDisabled}
                                onChange={(event) =>
                                    void updateSettings({ send_screenshot_to_codex: event.target.checked })
                                }
                            />
                        </label>
                    )}
                    <label className="sg-setting-row">
                        <span>
                            <strong>
                                Point with OpenClicky <span className="sg-labs-tag">Labs</span>
                            </strong>
                            <small>
                                {openClicky?.reachable
                                    ? openClicky.tokenFound
                                        ? "OpenClicky's cursor shows the way instead of FNDR's."
                                        : "Add OPENCLICKY_BRIDGE_TOKEN to ~/.config/openclicky/secrets.env."
                                    : "Open OpenClicky to use its cursor. FNDR's cursor is used until then."}
                            </small>
                        </span>
                        <input
                            type="checkbox"
                            role="switch"
                            className="fndr-switch"
                            aria-label="Point with OpenClicky"
                            checked={settings?.openclicky_bridge ?? false}
                            disabled={controlsDisabled}
                            onChange={(event) =>
                                void updateSettings({ openclicky_bridge: event.target.checked })
                            }
                        />
                    </label>
                    <label className="sg-setting-row">
                        <span>
                            <strong>
                                Operate my Mac <span className="sg-labs-tag">Labs</span>
                            </strong>
                            <small>
                                {computerUse && !computerUse.backend
                                    ? "FNDR could not start its own way of clicking and typing on this Mac, so this stays off. The ChatGPT app's Computer Use is not supported: FNDR could not check its actions one by one."
                                    : "Talk to the notch in Do mode and FNDR opens apps, clicks and types for you. Sent to ChatGPT on your plan: what you say, the name of the app in front, the on-screen text of the app being operated (and a picture of its window, if the helper has Screen Recording access), and up to 5 memory snippets when you refer to the past. Opening apps, playing media, following links and searching run without asking; other clicks, typing and links you did not ask for wait for your tap. Sending, deleting, buying, passwords, Terminal and blocklisted apps are always refused. Say “stop” anytime."}
                            </small>
                        </span>
                        <input
                            type="checkbox"
                            role="switch"
                            className="fndr-switch"
                            aria-label="Operate my Mac"
                            checked={canOperateComputer}
                            disabled={
                                controlsDisabled ||
                                !computerUse ||
                                // Turning it off always works; on needs a helper.
                                (!computerUse.backend && !computerUse.enabled)
                            }
                            onChange={(event) =>
                                void setComputerUseEnabled(event.target.checked)
                                    .then((enabled) =>
                                        setComputerUse((current) => (current ? { ...current, enabled } : current)),
                                    )
                                    .catch((reason) =>
                                        setError(reason instanceof Error ? reason.message : String(reason)),
                                    )
                            }
                        />
                    </label>
                    {canOperateComputer ? <OperatorPermissions /> : null}
                </section>

                <VoiceOutputSection />

                <section className="sg-ask-card">
                    <label htmlFor="sg-question">Ask about your display or find a named file</label>
                    <p className="sg-ask-hint" id="sg-question-hint">
                        Type a question now, or hold to talk. Screen Guide reads the display only for
                        this question; explicit file lookups check names only in Documents, Desktop,
                        and Downloads.
                    </p>
                    <div className="sg-question-row">
                        <input
                            id="sg-question"
                            type="text"
                            value={question}
                            disabled={questionDisabled}
                            aria-describedby="sg-question-hint"
                            placeholder="Where is the setting I need? or Find my I-20 document"
                            onChange={(event) => setQuestion(event.target.value)}
                            onKeyDown={(event) => {
                                if (event.key === "Enter") {
                                    event.preventDefault();
                                    void submitQuestion();
                                }
                            }}
                        />
                        <button
                            type="button"
                            className="ui-action-btn sg-ask-button"
                            disabled={questionDisabled || !question.trim()}
                            onClick={() => void submitQuestion()}
                            aria-label="Ask Screen Guide"
                        >
                            {submitting ? "Sending…" : "Ask"}
                        </button>
                    </div>

                    <button
                        type="button"
                        className={`sg-hold-button ${holding ? "is-holding" : ""}`}
                        disabled={questionDisabled}
                        aria-label={holding ? "Release to ask" : "Hold to talk"}
                        aria-pressed={holding}
                        onPointerDown={(event) => {
                            event.currentTarget.setPointerCapture?.(event.pointerId);
                            beginHold();
                        }}
                        onPointerUp={endHold}
                        onPointerCancel={endHold}
                        onKeyDown={(event) => {
                            if (!event.repeat && (event.key === " " || event.key === "Enter")) {
                                event.preventDefault();
                                beginHold();
                            }
                        }}
                        onKeyUp={(event) => {
                            if (event.key === " " || event.key === "Enter") {
                                event.preventDefault();
                                endHold();
                            }
                        }}
                    >
                        <span className="sg-mic-dot" aria-hidden="true" />
                        {holding ? "Release to ask" : "Hold to talk"}
                    </button>
                </section>

                <section className="sg-diagnostics-card" aria-labelledby="sg-diagnostics-title">
                    <div>
                        <strong id="sg-diagnostics-title">Troubleshoot the next turn</strong>
                        <p>
                            With your explicit consent, save the exact screenshot, OCR output, and
                            privacy-safe stage timings from one Screen Guide turn. The bundle stays
                            in FNDR app data, is never indexed or uploaded, and expires within 24 hours.
                        </p>
                        <p className="sg-diagnostics-warning">
                            The screenshot and OCR can contain anything visible on your display.
                        </p>
                        {diagnosticStatus?.armed && (
                            <p className="sg-diagnostics-state" role="status">
                                Next turn is armed. It expires in about {Math.max(
                                    1,
                                    Math.ceil((diagnosticStatus.expiresInMs ?? 0) / 60_000),
                                )} minute(s) if unused.
                            </p>
                        )}
                        {!diagnosticStatus?.armed && (diagnosticStatus?.bundleCount ?? 0) > 0 && (
                            <p className="sg-diagnostics-state" role="status">
                                {diagnosticStatus?.bundleCount} local diagnostic {diagnosticStatus?.bundleCount === 1
                                    ? "bundle"
                                    : "bundles"} saved
                                {diagnosticStatus && diagnosticStatus.totalBytes > 0
                                    ? ` (${formatBytes(diagnosticStatus.totalBytes)})`
                                    : ""}
                                .
                            </p>
                        )}
                        {(diagnosticStatus?.partialCount ?? 0) > 0 && (
                            <p className="sg-diagnostics-state" role="status">
                                {diagnosticStatus?.partialCount} diagnostic bundle
                                {diagnosticStatus?.partialCount === 1 ? " write is" : " writes are"}
                                {" "}still incomplete.
                            </p>
                        )}
                        {diagnosticStatus?.lastResult && (
                            <p
                                className={
                                    diagnosticStatus.lastResult.kind === "error"
                                        ? "sg-diagnostics-error"
                                        : "sg-diagnostics-state"
                                }
                                role={diagnosticStatus.lastResult.kind === "error" ? "alert" : "status"}
                            >
                                {diagnosticStatus.lastResult.message}
                                {diagnosticStatus.lastResult.kind === "saved"
                                    ? ` Receipt: screenshot ${
                                        diagnosticStatus.lastResult.screenshotSaved ? "saved" : "not saved"
                                    }; OCR ${
                                        diagnosticStatus.lastResult.ocrSaved ? "saved" : "not saved"
                                    }.`
                                    : diagnosticStatus.lastResult.kind === "error"
                                      ? ` (${diagnosticStatus.lastResult.code})`
                                      : ""}
                            </p>
                        )}
                        {isPrivateMode && (
                            <p className="sg-diagnostics-state">
                                Exit Private Mode before arming a diagnostic turn.
                            </p>
                        )}
                        {diagnosticError && <p className="sg-diagnostics-error" role="alert">{diagnosticError}</p>}
                    </div>
                    <div className="sg-diagnostics-actions">
                        <button
                            type="button"
                            className="ui-action-btn"
                            disabled={diagnosticBusy || isPrivateMode || diagnosticStatus?.armed}
                            onClick={() => void armDiagnostic()}
                        >
                            {diagnosticStatus?.armed ? "Next turn armed" : "Save next turn"}
                        </button>
                        {hasDiagnosticFiles && (
                            <button
                                type="button"
                                className="ui-action-btn"
                                disabled={diagnosticBusy}
                                onClick={() => void revealDiagnostics()}
                            >
                                Reveal diagnostics in Finder
                            </button>
                        )}
                        {(diagnosticStatus?.armed || hasDiagnosticFiles) && (
                            <button
                                type="button"
                                className="ui-action-btn"
                                disabled={diagnosticBusy}
                                onClick={() => void deleteDiagnostics()}
                            >
                                Delete diagnostics
                            </button>
                        )}
                    </div>
                </section>

                <aside className="sg-privacy-note">
                    <strong>{privacyHeading}</strong>
                    <p>
                        {!settings
                            ? "FNDR is loading the current model, egress, and action settings."
                            : usesChatGpt
                            ? sendsScreenshotToChatGpt
                                ? "Your question, visible text, and a downscaled screenshot are sent to OpenAI for this turn."
                                : "Your question and visible text are sent to OpenAI for this turn; screen pixels stay on this Mac."
                            : "The question, temporary display image, transcription, and answer stay on this Mac for this turn."}{" "}
                        {canOperateComputer
                            ? "Operate mode acts from the notch: opening apps, playback, links and searches you asked for run without asking; other clicks and typing wait for your tap. Say Stop at any time."
                            : "Screen Guide does not click or type."}{" "}
                        The turn is not added to Memory Vault. File search checks only file names in
                        Documents, Desktop, and Downloads; it never reads or opens files. Normal turns
                        are not saved; only an explicitly armed diagnostic turn creates a temporary,
                        deletable local bundle.
                    </p>
                </aside>

            </div>
        </div>
    );
}
