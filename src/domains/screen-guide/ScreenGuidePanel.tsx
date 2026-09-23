import { type KeyboardEvent as ReactKeyboardEvent, useEffect, useRef, useState } from "react";
import {
    type ScreenGuideSettings,
    type ScreenGuideStateEvent,
    getScreenGuideSettings,
    onScreenGuideState,
    screenGuidePress,
    screenGuideRelease,
    setScreenGuideSettings,
    submitScreenGuideText,
} from "@/shared/ipc/tauri";
import { screenGuideErrorMessage } from "./screenGuideState";
import "./ScreenGuidePanel.css";

interface ScreenGuidePanelProps {
    isVisible: boolean;
    onClose: () => void;
}

const IDLE_STATUS: ScreenGuideStateEvent = { phase: "idle", message: null };

function statusCopy(
    settings: ScreenGuideSettings | null,
    status: ScreenGuideStateEvent,
    loading: boolean,
): string {
    if (loading) return "Loading Screen Guide…";
    if (!settings?.enabled) return "Screen Guide is off";
    if (status.message) return status.message;
    switch (status.phase) {
        case "listening":
            return "Listening…";
        case "transcribing":
            return "Transcribing on this Mac…";
        case "thinking":
            return "Looking at this screen…";
        case "answer":
            return "Answer ready";
        case "error":
            return "Screen Guide needs attention";
        case "idle":
            return "Enabled";
    }
}

export function ScreenGuidePanel({ isVisible, onClose }: ScreenGuidePanelProps) {
    const [settings, setSettings] = useState<ScreenGuideSettings | null>(null);
    const [shortcutDraft, setShortcutDraft] = useState("");
    const [status, setStatus] = useState<ScreenGuideStateEvent>(IDLE_STATUS);
    const [question, setQuestion] = useState("");
    const [loading, setLoading] = useState(false);
    const [saving, setSaving] = useState(false);
    const [submitting, setSubmitting] = useState(false);
    const [holding, setHolding] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [loadAttempt, setLoadAttempt] = useState(0);
    const [listenerAttempt, setListenerAttempt] = useState(0);
    const [liveStatusError, setLiveStatusError] = useState<string | null>(null);
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
        let active = true;
        let unlisten: (() => void) | null = null;
        setLiveStatusError(null);

        void onScreenGuideState((nextStatus) => {
            if (active) setStatus(nextStatus);
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

    if (!isVisible) return null;

    const enabled = settings?.enabled ?? false;
    const controlsDisabled = loading || saving || !settings;
    const questionDisabled = controlsDisabled || !enabled || submitting;
    const settingsLoadFailed = !loading && !settings && Boolean(error);

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
            <header className="sg-panel-header">
                <div>
                    <p className="sg-panel-kicker">ON-SCREEN ASSISTANCE</p>
                    <h2 id="sg-panel-title">Screen Guide</h2>
                    <p id="sg-panel-description">
                        Read the current main display after you explicitly ask, then point you toward
                        a next step without clicking or typing for you.
                    </p>
                </div>
                <button
                    ref={closeButtonRef}
                    type="button"
                    className="ui-action-btn sg-panel-close"
                    onClick={onClose}
                    aria-label="Close Screen Guide"
                >
                    ×
                </button>
            </header>

            <div className="sg-panel-body">
                <section
                    className={`sg-readiness ${enabled ? "is-ready" : "is-off"}`}
                    role="status"
                    aria-live="polite"
                    aria-atomic="true"
                >
                    <span className="sg-readiness-dot" aria-hidden="true" />
                    <div>
                        <strong>{statusCopy(settings, status, loading)}</strong>
                        <span>
                            {enabled
                                ? `Hold ${settings?.shortcut ?? "Control+Alt+Space"} from any app`
                                : `${settings?.shortcut ?? "Control+Alt+Space"} is available when enabled`}
                        </span>
                    </div>
                </section>

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
                            <small>Make the hold-to-ask shortcut available</small>
                        </span>
                        <input
                            type="checkbox"
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
                            aria-label="Show guidance cursor"
                            checked={settings?.show_cursor ?? false}
                            disabled={controlsDisabled}
                            onChange={(event) =>
                                void updateSettings({ show_cursor: event.target.checked })
                            }
                        />
                    </label>
                </section>

                <section className="sg-ask-card">
                    <label htmlFor="sg-question">Ask about your main display</label>
                    <p className="sg-ask-hint" id="sg-question-hint">
                        Type a question now, or hold to talk. Screen Guide reads the display only for
                        this question.
                    </p>
                    <div className="sg-question-row">
                        <input
                            id="sg-question"
                            type="text"
                            value={question}
                            disabled={questionDisabled}
                            aria-describedby="sg-question-hint"
                            placeholder="Where is the setting I need?"
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

                <aside className="sg-privacy-note">
                    <strong>Local and read-only</strong>
                    <p>
                        The question, temporary display image, transcription, and answer are processed
                        on this Mac for this turn. Screen Guide does not click, type, or add the turn
                        to Memory Vault.
                    </p>
                </aside>

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
            </div>
        </div>
    );
}
