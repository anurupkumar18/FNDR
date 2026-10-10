import { useCallback, useEffect, useReducer, useRef, useState, type MutableRefObject } from "react";
import { ThinkingOrb } from "thinking-orbs";
import {
    COMPUTER_USE_EVENT,
    NOTCH_DO_ESCAPE_EVENT,
    codexLoginStart,
    computerUsePlan,
    computerUseRespond,
    computerUseStart,
    computerUseStop,
    openWorkSet,
    resolveWorkSet,
    setNotchEscapeMonitor,
    voiceOutStatus,
    type ComputerUseEvent,
} from "@/shared/ipc/tauri";
import { openSystemSettings } from "@/shared/ipc/onboarding";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import { openExternalUrl } from "@/shared/utils/openExternalUrl";
import { useVoice } from "@/shared/voice";
import { speechRegistry, type SpeechRegistry } from "@/shared/voice/speechRegistry";
import {
    AUTO_START_MS,
    CUE_MS,
    ENDPOINT_MS,
    HEARD_MS,
    SILENCE_MS,
    classifyUtterance,
    doRunReducer,
    initialDoState,
    isDeaf,
    micFor,
    resultNote,
    turnOf,
    type DoState,
} from "./doRun";
import { isEchoOfSpeech, narrate, type Narration } from "./doNarration";
import { createNotchVoice, installNotchVoices, loadNotchMuted, providerLine, saveNotchMuted, usageWarning } from "./notchSpeech";

interface NotchOperatorProps {
    /** The notch is open and Do mode is showing. */
    active: boolean;
    /** Filled with the one interrupt (Esc, Alt+N): true when it stopped a run or cut speech. */
    interruptRef?: MutableRefObject<(() => boolean) | null>;
    registry?: SpeechRegistry;
}

/** A run is planning, waiting on its card or chooser, or acting. */
function runInProgress(state: DoState): boolean {
    return state.phase === "heard" || state.phase === "planning" || state.phase === "plan" || state.phase === "running" || state.phase === "choose";
}

function message(reason: unknown): string {
    return reason instanceof Error ? reason.message : String(reason);
}

const QUIET_PHASES = ["listening", "heard", "silence", "mic_denied", "voice_unavailable"];

/**
 * Notch Do: opening the notch starts listening on FNDR's native voice owner.
 * When speech ends, what was heard shows for a beat before it is sent to be
 * planned. From then until FNDR has finished speaking the result the notch is
 * deaf: the helper listens only for "stop" or "cancel" and sends no text, and
 * anything else said shows a short cue, never the words (ADR 020 amendment,
 * 2026-10-09). The Stop button, Esc (in the notch, or anywhere while it
 * works), Alt+N and the spoken stop word are the four interrupts.
 * It talks while it works through the speech registry (`notchSpeech.ts`);
 * every spoken line is also the caption on screen.
 */
export function NotchOperator({ active, interruptRef, registry = speechRegistry }: NotchOperatorProps) {
    const [state, dispatch] = useReducer(doRunReducer, initialDoState);
    const [draft, setDraft] = useState("");
    const [caption, setCaption] = useState("");
    const [providerStatus, setProviderStatus] = useState(() => {
        const id = registry.activeProvider();
        return id ? providerLine(id, "") : "";
    });
    const [usage, setUsage] = useState<string | null>(null);
    const stateRef = useRef(state);
    stateRef.current = state;
    const turn = turnOf(state);
    const deaf = isDeaf(turn);
    const endpointTimer = useRef<number | null>(null);
    const silenceTimer = useRef<number | null>(null);
    /** Events that arrived before `computerUsePlan` returned their run id. */
    const earlyEvents = useRef<ComputerUseEvent[]>([]);
    const listenRef = useRef<ReturnType<typeof useVoice> | null>(null);
    const spotRef = useRef<ReturnType<typeof useVoice> | null>(null);
    const realtimeRef = useRef<ReturnType<typeof installNotchVoices> | null>(null);
    const voiceOutRef = useRef<ReturnType<typeof createNotchVoice> | null>(null);
    if (voiceOutRef.current === null) {
        realtimeRef.current = installNotchVoices(registry);
        voiceOutRef.current = createNotchVoice(registry);
    }
    const voiceOut = voiceOutRef.current;
    const [muted, setMuted] = useState(false);
    const mutedRef = useRef(muted);
    mutedRef.current = muted;
    const spokenState = useRef<DoState>(initialDoState);
    /** The last stop was spoken, so "Stopped." is said once. */
    const spokenStop = useRef(false);

    const clearTimer = (timer: { current: number | null }) => {
        if (timer.current !== null) window.clearTimeout(timer.current);
        timer.current = null;
    };

    const say = useCallback(
        (line: Narration) => {
            setCaption(line.text);
            if (mutedRef.current) return Promise.resolve();
            return voiceOut.say(line);
        },
        [voiceOut],
    );

    const stopRun = useCallback(async () => {
        clearTimer(endpointTimer);
        voiceOut.cancel();
        dispatch({ type: "stopped" });
        try {
            await computerUseStop();
        } catch (reason) {
            dispatch({ type: "error", message: message(reason) });
        }
    }, [voiceOut]);

    /** Esc, Alt+N and the Stop button: stop a run, or cut the readout. */
    const interrupt = useCallback((): boolean => {
        const current = stateRef.current;
        const now = turnOf(current);
        if (!isDeaf(now)) return false;
        if (now === "finishing" || now === "speaking") {
            voiceOut.cancel();
            dispatch({ type: "readoutDone" });
            return true;
        }
        void stopRun();
        return true;
    }, [stopRun, voiceOut]);
    if (interruptRef) interruptRef.current = interrupt;
    useEffect(
        () => () => {
            if (interruptRef) interruptRef.current = null;
        },
        [interruptRef],
    );

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

    /** Typed words narrow a work-set choice; they start no new request. */
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
            if (!spoken && current.phase === "choose") {
                void refine(text.trim());
                return;
            }
            // While deaf nothing but the stop word counts, and that arrives on its own channel.
            if (isDeaf(turnOf(current))) return;
            const intent = classifyUtterance(text);
            if (!intent) return;
            // The notch's own voice comes back through the microphone; a stop never counts as echo.
            if (intent.kind === "request" && spoken && isEchoOfSpeech(text, voiceOut.echoText())) return;
            if (intent.kind === "stop") {
                void stopRun();
                return;
            }
            // Typed words were already read by the person who typed them.
            if (spoken) dispatch({ type: "heard", text: intent.text });
            else void plan(intent.text);
        },
        [plan, refine, stopRun, voiceOut],
    );

    const listen = useVoice({
        surface: "notch_do",
        mode: "toggle",
        onPartial: (text) => {
            if (isDeaf(turnOf(stateRef.current))) return;
            if (isEchoOfSpeech(text, voiceOut.echoText())) return;
            clearTimer(silenceTimer);
            dispatch({ type: "partial", text });
            clearTimer(endpointTimer);
            endpointTimer.current = window.setTimeout(() => void listenRef.current?.stop(), ENDPOINT_MS);
        },
        onFinal: (text) => {
            clearTimer(endpointTimer);
            handleUtterance(text);
        },
    });
    listenRef.current = listen;

    // The stop-word spotter: no text ever arrives here, only that "stop" was said or that something else was.
    const spot = useVoice({
        surface: "notch_do",
        mode: "stop_words",
        onStopWord: () => {
            if (!isDeaf(turnOf(stateRef.current))) return;
            spokenStop.current = true;
            interrupt();
        },
        onIgnoredSpeech: () => dispatch({ type: "ignoredSpeech", at: Date.now() }),
    });
    spotRef.current = spot;

    // The persisted mute; the older per-window flag is migrated the first time.
    useEffect(() => {
        let live = true;
        void loadNotchMuted().then((saved) => {
            if (live) setMuted(saved);
        });
        return () => {
            live = false;
        };
    }, []);

    // Who is speaking, and how much of the plan is used.
    useEffect(
        () => registry.onProviderChange((id, reason) => setProviderStatus(providerLine(id, reason))),
        [registry],
    );
    useEffect(() => {
        if (!active || muted) return;
        let live = true;
        voiceOutStatus()
            .then((status) => live && setUsage(usageWarning(status)))
            .catch(() => undefined);
        return () => {
            live = false;
        };
    }, [active, muted]);

    useEffect(
        () =>
            voiceOut.onSpeaking((on) => dispatch({ type: on ? "speechStarted" : "speechEnded" })),
        [voiceOut],
    );

    // Opening the notch starts listening; closing it ends any run, the microphone and the voice call.
    useEffect(() => {
        if (active) {
            if (!runInProgress(stateRef.current)) dispatch({ type: "listening" });
            return;
        }
        clearTimer(endpointTimer);
        clearTimer(silenceTimer);
        voiceOut.cancel();
        void listenRef.current?.cancel();
        void spotRef.current?.cancel();
        void realtimeRef.current?.close();
        if (runInProgress(stateRef.current)) void stopRun();
    }, [active, stopRun, voiceOut]);

    // Quitting closes the voice call too.
    useEffect(() => {
        const close = () => void realtimeRef.current?.close();
        window.addEventListener("pagehide", close);
        return () => window.removeEventListener("pagehide", close);
    }, []);

    // Say what changed. Every spoken line is also the caption.
    useEffect(() => {
        const previous = spokenState.current;
        spokenState.current = state;
        if (QUIET_PHASES.includes(state.phase)) {
            voiceOut.cancel();
            if (state.phase === "listening" && previous.phase !== "listening") setCaption("");
            return;
        }
        if (state.phase === "stopped") {
            if (previous.phase === "stopped") return;
            voiceOut.cancel();
            if (spokenStop.current && active) void say({ text: "Stopped.", urgent: true });
            spokenStop.current = false;
            return;
        }
        const entersResult = state.phase === "finished" && previous.phase !== "finished";
        const line = active ? narrate(previous, state) : null;
        const done = line ? say(line) : Promise.resolve();
        if (entersResult) void done.then(() => dispatch({ type: "readoutDone" }));
    }, [state, active, say, voiceOut]);

    const toggleMuted = () => {
        const next = !muted;
        void saveNotchMuted(next);
        setMuted(next);
        if (next) voiceOut.cancel();
    };

    // The microphone follows the turn: open to hear a request, the stop-word spotter while deaf, else off.
    const mic = active ? micFor(turn) : "closed";
    useEffect(() => {
        const listenState = listen.state;
        const spotState = spot.state;
        if (mic === "open") {
            if (listen.isActive) return;
            if (listenState.kind === "unavailable" || (listenState.kind === "error" && listenState.code !== "cancelled")) return;
            void listen.start();
        } else if (mic === "stop_only") {
            if (spot.isActive) return;
            if (spotState.kind === "unavailable" || (spotState.kind === "error" && spotState.code !== "cancelled")) return;
            void spot.start();
        } else {
            if (listen.isActive) void listen.cancel();
            if (spot.isActive) void spot.cancel();
        }
    }, [mic, listen, spot]);

    // Esc in another app stops the run while it works or waits on an approval.
    const watchEscape = active && (turn === "working" || turn === "awaiting_approval");
    useEffect(() => {
        if (!watchEscape) return;
        void setNotchEscapeMonitor(true).catch(() => undefined);
        return () => void setNotchEscapeMonitor(false).catch(() => undefined);
    }, [watchEscape]);
    useTauriEvent<null>(NOTCH_DO_ESCAPE_EVENT, () => {
        interrupt();
    });

    // "Working on it. Say stop to interrupt." shows for a moment.
    useEffect(() => {
        if (!state.cue) return;
        const timer = window.setTimeout(() => dispatch({ type: "cueEnded" }), CUE_MS);
        return () => window.clearTimeout(timer);
    }, [state.cue, state.lastCueAt]);

    // Silence: nothing heard for a while before a request.
    useEffect(() => {
        if (listen.state.kind !== "listening" || stateRef.current.phase !== "listening" || stateRef.current.partial) return;
        if (silenceTimer.current !== null) return;
        silenceTimer.current = window.setTimeout(() => {
            silenceTimer.current = null;
            if (stateRef.current.phase === "listening" && !stateRef.current.partial) {
                void listenRef.current?.cancel();
                dispatch({ type: "silence" });
            }
        }, SILENCE_MS);
    }, [listen.state]);

    // Private Mode turned on: the run stops, the microphone closes and nothing more is sent to the voice.
    const privateNow = [listen.state, spot.state].some((v) => v.kind === "unavailable" && v.reason === "private_context");
    useEffect(() => {
        if (!privateNow) return;
        voiceOut.cancel();
        void realtimeRef.current?.close();
        if (runInProgress(stateRef.current)) void stopRun();
    }, [privateNow, stopRun, voiceOut]);

    // Microphone and recognizer failures while hearing a request are their own notch states.
    useEffect(() => {
        const v = listen.state;
        if (v.kind === "unavailable") {
            const denied = v.reason === "permission_denied" || v.reason === "permission_restricted";
            dispatch({ type: denied ? "micDenied" : "voiceUnavailable", message: v.message });
        } else if (v.kind === "error" && v.code === "permission_denied") {
            dispatch({ type: "micDenied", message: v.message });
        } else if (v.kind === "error" && v.code !== "cancelled") {
            dispatch({ type: "voiceUnavailable", message: v.message });
        }
    }, [listen.state]);

    // What was heard is sent to be planned after a beat, unless stopped.
    useEffect(() => {
        if (state.phase !== "heard") return;
        const heard = state.transcript;
        const timer = window.setTimeout(() => void plan(heard), HEARD_MS);
        return () => window.clearTimeout(timer);
    }, [state.phase, state.transcript, plan]);

    // The plan card starts the run by itself unless stopped.
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
            voiceOut.cancel();
            if (runInProgress(stateRef.current)) void computerUseStop().catch(() => undefined);
        },
        [voiceOut],
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
        voiceOut.cancel();
        dispatch({ type: "listening" });
    };

    // Number keys pick from the chooser; Esc is Stop in every deaf state.
    const onKeyDown = (event: React.KeyboardEvent) => {
        if (event.key === "Escape" && interrupt()) {
            event.preventDefault();
            event.stopPropagation();
            return;
        }
        if (state.phase === "choose" && /^[1-9]$/.test(event.key) && !(event.target instanceof HTMLInputElement)) {
            const set = state.options[Number(event.key) - 1];
            if (set) {
                event.preventDefault();
                dispatch({ type: "workSetChosen", set });
            }
        }
    };

    const thinking = turn === "thinking" || turn === "working" || turn === "awaiting_approval";
    const status = (() => {
        switch (state.phase) {
            case "idle":
            case "listening":
                return state.partial || "Listening. Say what to do";
            case "heard":
                return "Heard this. Say stop if it is wrong";
            case "silence":
                return "Didn't hear anything.";
            case "mic_denied":
                return "FNDR can't use the microphone.";
            case "voice_unavailable":
                return state.error ?? "Voice isn't available right now.";
            case "planning":
                return state.usedMemories > 0
                    ? `Planning with ${state.usedMemories} memories. Say stop to cancel`
                    : "Planning. Say stop to cancel";
            case "choose":
                return "Which one? Tap it or press its number";
            case "plan":
                return state.autoStart ? "Starting. Say stop to cancel" : "Ready. Press Start";
            case "running":
                return "Working. Say stop to interrupt";
            case "finished":
                return state.result?.summary ?? "Done.";
            case "failed":
                return state.error ?? "Something went wrong.";
            case "stopped":
                return "Stopped. Nothing else will run.";
        }
    })();
    const typedDisabled = deaf && state.phase !== "choose";
    const placeholder = typedDisabled
        ? "Working. Stop first to ask something new."
        : state.phase === "choose"
          ? "Type to narrow it down"
          : "Or type what to do";

    return (
        <div
            className={`notch-operator is-turn-${turn}${state.speaking ? " is-speaking" : ""}`}
            data-turn={turn}
            onKeyDown={onKeyDown}
        >
            <p className="notch-operator-status" role="status" aria-live="polite">
                {thinking ? (
                    <span className="notch-operator-orb">
                        <ThinkingOrb state="working" size={20} theme="dark" />
                    </span>
                ) : null}
                <span>{status}</span>
            </p>

            {caption && caption !== status ? (
                <p className="notch-operator-caption" aria-live="polite" data-speaking={state.speaking || undefined}>
                    <span className="notch-operator-speaker" aria-hidden="true">
                        ♪
                    </span>
                    {caption}
                </p>
            ) : null}

            {deaf ? (
                <p
                    className={`notch-operator-chip${state.cue ? " is-cue" : ""}`}
                    aria-live={state.cue ? "polite" : "off"}
                >
                    {state.cue ? "Working on it. Say stop to interrupt." : "Mic off while working. Say stop to interrupt"}
                </p>
            ) : null}

            {state.transcript ? <p className="notch-operator-heard">“{state.transcript}”</p> : null}

            {state.phase === "choose" ? (
                <ol className="notch-operator-steps" role="alertdialog" aria-label="Pieces of work">
                    {state.options.map((option, index) => (
                        <li key={option.id} className="notch-operator-step is-pending">
                            <button
                                type="button"
                                className="notch-operator-btn"
                                autoFocus={index === 0}
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
                        <button type="button" className="notch-operator-btn" autoFocus onClick={() => void respond(false)}>
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
                        <button
                            type="button"
                            className="notch-operator-btn notch-operator-btn-primary"
                            autoFocus={!state.autoStart}
                            onClick={() => void startRun()}
                        >
                            {state.autoStart ? "Start now" : "Start"}
                        </button>
                    </>
                ) : null}
                {state.phase === "planning" || state.phase === "running" ? (
                    <button type="button" className="notch-operator-btn notch-operator-stop" onClick={() => void stopRun()}>
                        Stop
                    </button>
                ) : null}
                {turn === "finishing" || turn === "speaking" ? (
                    <button type="button" className="notch-operator-btn notch-operator-stop" onClick={() => interrupt()}>
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
                <button type="button" className="notch-operator-btn" aria-pressed={muted} onClick={toggleMuted}>
                    {muted ? "Unmute voice" : "Mute voice"}
                </button>
                {!deaf && ["silence", "finished", "failed", "stopped", "voice_unavailable"].includes(state.phase) ? (
                    <button type="button" className="notch-operator-btn" onClick={listenAgain}>
                        Listen again
                    </button>
                ) : null}
            </div>

            {providerStatus || usage ? (
                <p className="notch-operator-provider">{[providerStatus, usage].filter(Boolean).join(" · ")}</p>
            ) : null}

            <form
                className="notch-operator-type"
                onSubmit={(event) => {
                    event.preventDefault();
                    const text = draft.trim();
                    if (!text || typedDisabled) return;
                    setDraft("");
                    handleUtterance(text, false);
                }}
            >
                <input
                    className="notch-input"
                    value={draft}
                    placeholder={placeholder}
                    aria-label="Instruction for FNDR"
                    disabled={typedDisabled}
                    onChange={(event) => setDraft(event.target.value)}
                />
            </form>
        </div>
    );
}

