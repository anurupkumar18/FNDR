import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { ThinkingOrb } from "thinking-orbs";
import {
    COMPUTER_USE_EVENT,
    codexLoginStart,
    computerUsePlan,
    computerUseRespond,
    computerUseStart,
    computerUseStop,
    openWorkSet,
    resolveWorkSet,
    type ComputerUseEvent,
} from "@/shared/ipc/tauri";
import { openSystemSettings } from "@/shared/ipc/onboarding";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import { openExternalUrl } from "@/shared/utils/openExternalUrl";
import { useVoice } from "@/shared/voice";
import {
    AUTO_START_MS,
    ENDPOINT_MS,
    HEARD_MS,
    SILENCE_MS,
    classifyUtterance,
    doRunReducer,
    initialDoState,
    isStopWord,
    resultNote,
    type DoState,
} from "./doRun";
import { isEchoOfSpeech, narrate } from "./doNarration";
import { createSpeaker, readMuted, writeMuted, type NotchSpeaker } from "./notchSpeech";

interface NotchOperatorProps {
    /** The notch is open and Do mode is showing. */
    active: boolean;
}

/** A run is planning, waiting on its plan card, or acting. */
function runInProgress(state: DoState): boolean {
    return state.phase === "planning" || state.phase === "plan" || state.phase === "running";
}

/** Listening continues through a run so "stop" works; it ends with the run. */
function wantsMicrophone(state: DoState): boolean {
    return state.phase === "listening" || state.phase === "heard" || state.phase === "choose" || runInProgress(state);
}

function message(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
}

/**
 * Notch Do: opening the notch starts listening on FNDR's native voice owner.
 * When speech ends, what was heard shows for a beat before it is sent to be
 * planned, so a misheard request can be stopped while it is still on the Mac.
 * The plan is a step list; it starts by itself only when no step can need a
 * yes, otherwise it waits for Start or "go".
 * Saying "stop" or pressing Stop kills the run at any point.
 * It talks while it works (`doNarration.ts`) unless muted: what it understood,
 * each step, anything it asks first or leaves out, and what it checked. Speech
 * is announcement only; an approval is still a tap.
 */
export function NotchOperator({ active }: NotchOperatorProps) {
    const [state, dispatch] = useReducer(doRunReducer, initialDoState);
    const [draft, setDraft] = useState("");
    const stateRef = useRef(state);
    stateRef.current = state;
    const endpointTimer = useRef<number | null>(null);
    const silenceTimer = useRef<number | null>(null);
    /** Events that arrived before `computerUsePlan` returned their run id. */
    const earlyEvents = useRef<ComputerUseEvent[]>([]);
    const voiceRef = useRef<ReturnType<typeof useVoice> | null>(null);
    const speakerRef = useRef<NotchSpeaker | null | undefined>(undefined);
    if (speakerRef.current === undefined) speakerRef.current = createSpeaker();
    const [muted, setMuted] = useState(readMuted);
    const mutedRef = useRef(muted);
    mutedRef.current = muted;
    const spokenState = useRef<DoState>(initialDoState);

    const clearTimer = (timer: { current: number | null }) => {
        if (timer.current !== null) window.clearTimeout(timer.current);
        timer.current = null;
    };

    const stopRun = useCallback(async () => {
        clearTimer(endpointTimer);
        speakerRef.current?.cancel();
        dispatch({ type: "stopped" });
        try {
            await computerUseStop();
        } catch (reason) {
            dispatch({ type: "error", message: message(reason) });
        }
    }, []);

    const plan = useCallback(async (text: string) => {
        const transcript = text.trim();
        if (!transcript) return;
        try {
            const runId = await computerUsePlan(transcript);
            dispatch({ type: "planRequested", runId, transcript });
            const early = earlyEvents.current.filter((event) => event.runId === runId);
            earlyEvents.current = [];
            early.forEach((event) => dispatch({ type: "event", event }));
        } catch (reason) {
            dispatch({ type: "error", message: message(reason) });
        }
    }, []);

    /** A picked work set is opened by FNDR itself, through the same reopen core. */
    const openPicked = useCallback(async () => {
        const set = stateRef.current.workSet;
        if (!set || stateRef.current.phase !== "plan") return;
        dispatch({ type: "workSetOpening" });
        try {
            const outcomes = await openWorkSet(set.items.map((item) => item.memoryId));
            // Stopped while opening: what opened stays open, and the card stays stopped.
            if (runInProgress(stateRef.current)) dispatch({ type: "workSetOpened", outcomes });
        } catch (reason) {
            dispatch({ type: "error", message: message(reason) });
        }
    }, []);

    /** Words that did not name an offered set narrow the request instead. */
    const refine = useCallback(async (words: string) => {
        try {
            const resolution = await resolveWorkSet(`${stateRef.current.transcript} ${words}`);
            dispatch({ type: "workSetResolved", resolution });
        } catch (reason) {
            dispatch({ type: "error", message: message(reason) });
        }
    }, []);

    const startRun = useCallback(async () => {
        if (stateRef.current.workSet) {
            await openPicked();
            return;
        }
        const runId = stateRef.current.runId;
        if (!runId || stateRef.current.phase !== "plan") return;
        try {
            await computerUseStart(runId);
        } catch (reason) {
            dispatch({ type: "error", message: message(reason) });
        }
    }, [openPicked]);

    const respond = useCallback(async (approve: boolean) => {
        const approval = stateRef.current.approval;
        if (!approval) return;
        try {
            await computerUseRespond(approval.requestKey, approve);
        } catch (reason) {
            dispatch({ type: "error", message: message(reason) });
        }
    }, []);

    const handleUtterance = useCallback(
        (text: string, spoken = true) => {
            const current = stateRef.current;
            // Typed words narrow a work-set choice; they start no new request.
            if (!spoken && current.phase === "choose") {
                void refine(text.trim());
                return;
            }
            const intent = classifyUtterance(text);
            if (!intent) return;
            // The notch's own voice comes back through the microphone; a stop never counts as echo.
            if (intent.kind === "request" && spoken && isEchoOfSpeech(text, speakerRef.current?.echoText() ?? "")) return;
            if (intent.kind === "stop") {
                void stopRun();
                return;
            }
            // Typed words were already read by the person who typed them.
            if (spoken) dispatch({ type: "heard", text: intent.text });
            else void plan(intent.text);
        },
        [plan, refine, stopRun],
    );

    const voice = useVoice({
        surface: "notch_do",
        mode: "toggle",
        onPartial: (text) => {
            if (isEchoOfSpeech(text, speakerRef.current?.echoText() ?? "")) return;
            clearTimer(silenceTimer);
            if (runInProgress(stateRef.current) && isStopWord(text)) {
                void voiceRef.current?.cancel();
                void stopRun();
                return;
            }
            dispatch({ type: "partial", text });
            clearTimer(endpointTimer);
            endpointTimer.current = window.setTimeout(() => void voiceRef.current?.stop(), ENDPOINT_MS);
        },
        onFinal: (text) => {
            clearTimer(endpointTimer);
            handleUtterance(text);
        },
    });
    voiceRef.current = voice;

    // Opening the notch starts listening; closing it stops the microphone.
    useEffect(() => {
        if (active) {
            if (!runInProgress(stateRef.current)) dispatch({ type: "listening" });
            return;
        }
        clearTimer(endpointTimer);
        clearTimer(silenceTimer);
        speakerRef.current?.cancel();
        void voiceRef.current?.cancel();
    }, [active]);

    // Say what changed. A new turn, a stop or a failure of the ears ends whatever is being said.
    useEffect(() => {
        const previous = spokenState.current;
        spokenState.current = state;
        const speaker = speakerRef.current;
        if (!speaker) return;
        if (["listening", "heard", "silence", "mic_denied", "voice_unavailable", "stopped"].includes(state.phase)) {
            speaker.cancel();
            return;
        }
        if (!active || mutedRef.current) return;
        const line = narrate(previous, state);
        if (line) speaker.say(line);
    }, [state, active]);

    const toggleMuted = () => {
        const next = !muted;
        writeMuted(next);
        setMuted(next);
        if (next) speakerRef.current?.cancel();
    };

    // Keep a session open while one is wanted: a new one after each utterance.
    useEffect(() => {
        if (!active || voice.isActive || !wantsMicrophone(state)) return;
        const kind = voice.state.kind;
        if (kind === "unavailable" || (kind === "error" && voice.state.code !== "cancelled")) return;
        void voice.start();
    }, [active, state, voice]);

    // Silence: nothing heard for a while before a request.
    useEffect(() => {
        if (voice.state.kind !== "listening" || stateRef.current.phase !== "listening" || stateRef.current.partial) return;
        if (silenceTimer.current !== null) return;
        silenceTimer.current = window.setTimeout(() => {
            silenceTimer.current = null;
            if (stateRef.current.phase === "listening" && !stateRef.current.partial) {
                void voiceRef.current?.cancel();
                dispatch({ type: "silence" });
            }
        }, SILENCE_MS);
    }, [voice.state]);

    // Microphone and recognizer failures are their own notch states.
    useEffect(() => {
        const v = voice.state;
        if (v.kind === "unavailable") {
            const denied = v.reason === "permission_denied" || v.reason === "permission_restricted";
            dispatch({ type: denied ? "micDenied" : "voiceUnavailable", message: v.message });
        } else if (v.kind === "error" && v.code === "permission_denied") {
            dispatch({ type: "micDenied", message: v.message });
        } else if (v.kind === "error" && v.code !== "cancelled") {
            dispatch({ type: "voiceUnavailable", message: v.message });
        }
    }, [voice.state]);

    // What was heard is sent to be planned after a beat, unless stopped or corrected.
    useEffect(() => {
        if (state.phase !== "heard") return;
        const heard = state.transcript;
        const timer = window.setTimeout(() => void plan(heard), HEARD_MS);
        return () => window.clearTimeout(timer);
    }, [state.phase, state.transcript, plan]);

    // The plan card starts the run by itself unless stopped or redirected.
    useEffect(() => {
        if (state.phase !== "plan" || !state.autoStart) return;
        const timer = window.setTimeout(() => void startRun(), AUTO_START_MS);
        return () => window.clearTimeout(timer);
    }, [state.phase, state.runId, state.autoStart, startRun]);

    // Leaving Do mode ends any run.
    useEffect(
        () => () => {
            clearTimer(endpointTimer);
            clearTimer(silenceTimer);
            speakerRef.current?.cancel();
            if (runInProgress(stateRef.current)) void computerUseStop().catch(() => undefined);
        },
        [],
    );

    useTauriEvent<ComputerUseEvent>(COMPUTER_USE_EVENT, (event) => {
        if (event.runId !== stateRef.current.runId) {
            earlyEvents.current = [...earlyEvents.current, event].slice(-20);
            return;
        }
        dispatch({ type: "event", event });
    });

    const reconnect = async () => {
        try {
            const started = await codexLoginStart();
            await openExternalUrl(started.authUrl);
        } catch (reason) {
            dispatch({ type: "error", message: message(reason) });
        }
    };

    const listenAgain = () => {
        dispatch({ type: "listening" });
    };

    const busy = state.phase === "planning" || state.phase === "running";
    const status = (() => {
        switch (state.phase) {
            case "idle":
            case "listening":
                return state.partial || "Listening — say what to do";
            case "heard":
                return "Heard this. Say “stop” if it is wrong";
            case "silence":
                return "Didn't hear anything.";
            case "mic_denied":
                return "FNDR can't use the microphone.";
            case "voice_unavailable":
                return state.error ?? "Voice isn't available right now.";
            case "planning":
                return state.usedMemories > 0 ? `Planning with ${state.usedMemories} memories…` : "Planning…";
            case "choose":
                return "Which one? Tap it or say its number";
            case "plan":
                return state.autoStart ? "Starting. Say “stop” to cancel" : "Ready. Tap Start or say “go”";
            case "running":
                return state.partial || "Working — say “stop” anytime";
            case "finished":
                return state.result?.summary ?? "Done.";
            case "failed":
                return state.error ?? "Something went wrong.";
            case "stopped":
                return "Stopped.";
        }
    })();

    return (
        <div className="notch-operator">
            <p className="notch-operator-status" role="status" aria-live="polite">
                {busy ? <ThinkingOrb state="working" size={20} theme="dark" /> : null}
                <span>{status}</span>
            </p>

            {state.transcript ? <p className="notch-operator-heard">“{state.transcript}”</p> : null}

            {state.phase === "choose" ? (
                <ol className="notch-operator-steps" aria-label="Pieces of work">
                    {state.options.map((option, index) => (
                        <li key={option.id} className="notch-operator-step is-pending">
                            <button
                                type="button"
                                className="notch-operator-btn"
                                onClick={() => dispatch({ type: "workSetChosen", set: option })}
                            >
                                {index + 1}. {option.title}
                            </button>
                            <span className="notch-operator-step-detail">
                                {option.items.map((item) => item.label).join(" · ")}
                            </span>
                            <span className="notch-operator-step-detail">{option.reason}</span>
                        </li>
                    ))}
                </ol>
            ) : null}

            {state.steps.length > 0 ? (
                <ol className="notch-operator-steps" aria-label="Plan">
                    {state.steps.map((step, index) => (
                        <li
                            key={`${index}-${step.label}`}
                            className={`notch-operator-step is-${step.status}${state.current === index ? " is-current" : ""}`}
                            aria-current={state.current === index ? "step" : undefined}
                        >
                            <span className={`notch-operator-dot is-${step.status}`} aria-hidden="true" />
                            <span className="notch-operator-step-label">{step.label}</span>
                            {step.detail && step.status !== "running" ? (
                                <span className="notch-operator-step-detail">{step.detail}</span>
                            ) : null}
                        </li>
                    ))}
                </ol>
            ) : null}

            {state.phase === "plan" && state.autoStart ? (
                <div className="notch-operator-countdown" aria-hidden="true">
                    <span style={{ animationDuration: `${AUTO_START_MS}ms` }} />
                </div>
            ) : null}

            {state.phase === "finished" && resultNote(state.steps) ? (
                <p className="notch-operator-heard">{resultNote(state.steps)}</p>
            ) : null}

            {state.phase === "running" && state.actions.length > 0 ? (
                <ul className="notch-operator-log" aria-label="Actions">
                    {state.actions.map((action) => (
                        <li key={action.id} className="notch-operator-action">
                            <span className={`notch-operator-dot is-${action.state}`} aria-hidden="true" />
                            <span>{action.state === "blocked" ? `Refused: ${action.summary}` : action.summary}</span>
                        </li>
                    ))}
                </ul>
            ) : null}

            {state.approval ? (
                <div className="notch-operator-approval" role="alertdialog" aria-label="Approve action">
                    <p>
                        Okay to <strong>{state.approval.summary}</strong>?
                    </p>
                    <div className="notch-operator-approval-actions">
                        <button type="button" className="notch-operator-btn" onClick={() => void respond(false)}>
                            Don&apos;t
                        </button>
                        <button type="button" className="notch-operator-btn notch-operator-btn-primary" onClick={() => void respond(true)}>
                            Allow
                        </button>
                    </div>
                </div>
            ) : null}

            <div className="notch-operator-controls">
                {state.phase === "heard" ? (
                    <>
                        <button type="button" className="notch-operator-btn" onClick={() => void stopRun()}>
                            Cancel
                        </button>
                        <button
                            type="button"
                            className="notch-operator-btn notch-operator-btn-primary"
                            onClick={() => void plan(state.transcript)}
                        >
                            Send now
                        </button>
                    </>
                ) : null}
                {state.phase === "choose" ? (
                    <button type="button" className="notch-operator-btn" onClick={() => void stopRun()}>
                        Cancel
                    </button>
                ) : null}
                {state.phase === "plan" ? (
                    <>
                        <button type="button" className="notch-operator-btn" onClick={() => void stopRun()}>
                            Cancel
                        </button>
                        <button type="button" className="notch-operator-btn notch-operator-btn-primary" onClick={() => void startRun()}>
                            {state.autoStart ? "Start now" : "Start"}
                        </button>
                    </>
                ) : null}
                {state.phase === "planning" || state.phase === "running" ? (
                    <button type="button" className="notch-operator-btn notch-operator-stop" onClick={() => void stopRun()}>
                        Stop
                    </button>
                ) : null}
                {state.phase === "mic_denied" ? (
                    <button type="button" className="notch-operator-btn" onClick={() => void openSystemSettings("microphone")}>
                        Open Microphone Settings
                    </button>
                ) : null}
                {state.reconnect ? (
                    <button type="button" className="notch-operator-btn notch-operator-btn-primary" onClick={() => void reconnect()}>
                        Reconnect ChatGPT
                    </button>
                ) : null}
                {speakerRef.current ? (
                    <button
                        type="button"
                        className="notch-operator-btn"
                        aria-pressed={muted}
                        onClick={toggleMuted}
                    >
                        {muted ? "Unmute voice" : "Mute voice"}
                    </button>
                ) : null}
                {["silence", "finished", "failed", "stopped", "voice_unavailable"].includes(state.phase) ? (
                    <button type="button" className="notch-operator-btn" onClick={listenAgain}>
                        Listen again
                    </button>
                ) : null}
            </div>

            <form
                className="notch-operator-type"
                onSubmit={(event) => {
                    event.preventDefault();
                    const text = draft.trim();
                    if (!text) return;
                    setDraft("");
                    handleUtterance(text, false);
                }}
            >
                <input
                    className="notch-input"
                    value={draft}
                    placeholder="Or type what to do"
                    aria-label="Instruction for FNDR"
                    onChange={(event) => setDraft(event.target.value)}
                />
            </form>
        </div>
    );
}
