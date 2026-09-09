import { useEffect, useRef, useState } from "react";
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

const IDLE_STATUS: ScreenGuideStateEvent = { phase: "idle", message: null, generation: 0 };

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
            return "Finding the answer on this Mac…";
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
    const holdingRef = useRef(false);
    const pressPromiseRef = useRef<Promise<number> | null>(null);

    useEffect(() => {
        if (!isVisible) return;

        let active = true;
        setLoading(true);
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
    }, [isVisible]);

    useEffect(() => {
        let active = true;
        let unlisten: (() => void) | null = null;

        void onScreenGuideState((nextStatus) => {
            if (active) setStatus(nextStatus);
        })
            .then((dispose) => {
                if (active) unlisten = dispose;
                else dispose();
            })
            .catch(() => {
                // Settings still supply a useful enabled state if event setup fails.
            });

        return () => {
            active = false;
            unlisten?.();
        };
    }, []);

    useEffect(() => {
        setShortcutDraft(settings?.shortcut ?? "");
    }, [settings?.shortcut]);

    if (!isVisible) return null;

    const enabled = settings?.enabled ?? false;
    const controlsDisabled = loading || saving || !settings;
    const questionDisabled = controlsDisabled || !enabled || submitting;

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
        <div className="sg-panel-page">
            <header className="sg-panel-header">
                <div>
                    <p className="sg-panel-kicker">FNDR VOICE</p>
                    <h2>Screen Guide</h2>
                    <p>
                        Hold your shortcut, talk naturally, and FNDR can guide what is on screen or
                        find a file by name.
                    </p>
                </div>
                <button
                    type="button"
                    className="ui-action-btn sg-panel-close"
                    onClick={onClose}
                    aria-label="Close Screen Guide"
                >
                    ×
                </button>
            </header>

            <div className="sg-panel-body">
                <section className={`sg-readiness ${enabled ? "is-ready" : "is-off"}`}>
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

                <section className="sg-settings-card" aria-label="Screen Guide settings">
                    <label className="sg-setting-row">
                        <span>
                            <strong>Enable Screen Guide</strong>
                            <small>Keep FNDR ready beside the notch for hold-to-talk</small>
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
                                Save
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
                    <label htmlFor="sg-question">Ask FNDR about this screen or your files</label>
                    <div className="sg-question-row">
                        <input
                            id="sg-question"
                            type="text"
                            value={question}
                            disabled={questionDisabled}
                            placeholder="Where is Save? or Find my I-20 document"
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
                    <strong>Private by design</strong>
                    <p>
                        Your question, voice transcription, and screen context stay on this Mac.
                        FNDR only listens while you hold the button or shortcut. File search checks
                        only file names in Documents, Desktop, and Downloads; it does not read or
                        open the files.
                    </p>
                </aside>

                {error && (
                    <p className="sg-panel-error" role="alert">
                        {error}
                    </p>
                )}
            </div>
        </div>
    );
}
