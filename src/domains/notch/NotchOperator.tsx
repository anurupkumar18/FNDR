import { useCallback, useEffect, useRef, useState } from "react";
import { ThinkingOrb } from "thinking-orbs";
import {
    COMPUTER_USE_EVENT,
    computerUseInterrupt,
    computerUseRespond,
    computerUseSay,
    computerUseStop,
    type ComputerUseEvent,
} from "@/shared/ipc/tauri";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
    type ActivityTraceStep,
} from "@/shared/activity/activityTrace";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import { DuplexListener, Speaker, classifyUtterance } from "./duplexVoice";

type LogEntry =
    | { id: string; who: "you"; text: string }
    | { id: string; who: "fndr"; text: string }
    | { id: string; who: "action"; text: string; state: "running" | "done" | "failed" };

interface PendingApproval {
    requestKey: string;
    summary: string;
}

type Phase = "starting" | "listening" | "working" | "waiting";

/** Most recent entries kept on screen; the notch is small. */
const MAX_LOG = 8;

let logSeq = 0;
const nextId = () => `op-${++logSeq}`;

function settleOpenActivitySteps(
    trace: ActivityTraceSnapshot,
    atMs: number,
): ActivityTraceSnapshot {
    return trace.steps.reduce((next, step) => {
        if (step.status !== "running" && step.status !== "waiting") return next;
        return recordActivityStep(next, {
            ...step,
            status: "completed",
            atMs,
            durationMs: Math.max(0, atMs - step.atMs),
        });
    }, trace);
}

interface NotchOperatorProps {
    /** The notch is open and this mode is showing. */
    active: boolean;
    onStreamChange?: (stream: MediaStream | null) => void;
}

/**
 * "Do" mode in the notch: a spoken conversation in which FNDR operates the Mac.
 * The microphone stays open, FNDR narrates each step aloud, every action waits
 * for a spoken or tapped yes, and saying "stop" halts it mid-step.
 */
export function NotchOperator({ active, onStreamChange }: NotchOperatorProps) {
    const [log, setLog] = useState<LogEntry[]>([]);
    const [pending, setPending] = useState<PendingApproval | null>(null);
    const [phase, setPhase] = useState<Phase>("starting");
    const [partial, setPartial] = useState("");
    const [error, setError] = useState<string | null>(null);
    const [draft, setDraft] = useState("");
    const [activityTrace, setActivityTrace] = useState<ActivityTraceSnapshot | null>(null);

    const speakerRef = useRef<Speaker | null>(null);
    const listenerRef = useRef<DuplexListener | null>(null);
    const pendingRef = useRef<PendingApproval | null>(null);
    const activityTraceSeqRef = useRef(0);
    const activityStepSeqRef = useRef(0);
    const actionStepIdsRef = useRef(new Map<string, string>());
    pendingRef.current = pending;

    const append = useCallback((entry: LogEntry) => {
        setLog((current) => [...current, entry].slice(-MAX_LOG));
    }, []);

    const speak = useCallback((text: string) => speakerRef.current?.speak(text), []);

    const beginOperatorActivity = useCallback((step: ActivityTraceStep) => {
        const traceId = `computer-use-${++activityTraceSeqRef.current}`;
        setActivityTrace(recordActivityStep(
            beginActivityTrace({
                id: traceId,
                title: "Computer use activity",
                startedAtMs: step.atMs,
            }),
            step,
        ));
        return traceId;
    }, []);

    const recordOperatorActivity = useCallback((step: ActivityTraceStep) => {
        setActivityTrace((current) => {
            const base = current ?? beginActivityTrace({
                id: `computer-use-${++activityTraceSeqRef.current}`,
                title: "Computer use activity",
                startedAtMs: step.atMs,
            });
            return recordActivityStep(settleOpenActivitySteps(base, step.atMs), step);
        });
    }, []);

    const finishCurrentActivityStep = useCallback((input: {
        traceId?: string;
        stepId: string;
        label: string;
        status: "completed" | "failed";
        atMs: number;
        startedAtMs: number;
    }) => {
        setActivityTrace((current) => {
            if (!current || (input.traceId && current.id !== input.traceId)) return current;
            const latest = current.steps[current.steps.length - 1];
            if (latest?.id !== input.stepId) return current;
            return recordActivityStep(current, {
                ...latest,
                label: input.label,
                status: input.status,
                atMs: input.atMs,
                durationMs: Math.max(0, input.atMs - input.startedAtMs),
            });
        });
    }, []);

    const respond = useCallback(
        async (approve: boolean) => {
            const current = pendingRef.current;
            if (!current) return;
            setPending(null);
            speakerRef.current?.cancel();
            const requestedAt = Date.now();
            const stepId = `approval-response-${++activityStepSeqRef.current}`;
            setActivityTrace((trace) => {
                const base = trace ?? beginActivityTrace({
                    id: `computer-use-${++activityTraceSeqRef.current}`,
                    title: "Computer use activity",
                    startedAtMs: requestedAt,
                });
                return recordActivityStep(settleOpenActivitySteps(base, requestedAt), {
                    id: stepId,
                    label: "Sending approval decision",
                    actor: "Approval gate",
                    status: "running",
                    evidence: "ipc-boundary",
                    atMs: requestedAt,
                });
            });
            try {
                await computerUseRespond(current.requestKey, approve);
                const finishedAt = Date.now();
                finishCurrentActivityStep({
                    stepId,
                    label: "Approval decision sent",
                    status: "completed",
                    atMs: finishedAt,
                    startedAtMs: requestedAt,
                });
                if (!approve) speak("Okay, I won't.");
            } catch (reason) {
                setError(reason instanceof Error ? reason.message : String(reason));
                const failedAt = Date.now();
                finishCurrentActivityStep({
                    stepId,
                    label: "Approval decision failed",
                    status: "failed",
                    atMs: failedAt,
                    startedAtMs: requestedAt,
                });
            }
        },
        [finishCurrentActivityStep, speak],
    );

    const handleUtterance = useCallback(
        async (text: string) => {
            setPartial("");
            const intent = classifyUtterance(text, pendingRef.current !== null);
            if (!intent) return;
            try {
                if (intent.kind === "stop") {
                    speakerRef.current?.cancel();
                    setPending(null);
                    const requestedAt = Date.now();
                    const stepId = `interrupt-${++activityStepSeqRef.current}`;
                    setActivityTrace((trace) => {
                        const base = trace ?? beginActivityTrace({
                            id: `computer-use-${++activityTraceSeqRef.current}`,
                            title: "Computer use activity",
                            startedAtMs: requestedAt,
                        });
                        return recordActivityStep(settleOpenActivitySteps(base, requestedAt), {
                            id: stepId,
                            label: "Requesting computer-use interruption",
                            actor: "Computer use",
                            status: "running",
                            evidence: "ipc-boundary",
                            atMs: requestedAt,
                        });
                    });
                    await computerUseInterrupt();
                    const finishedAt = Date.now();
                    finishCurrentActivityStep({
                        stepId,
                        label: "Interruption request accepted",
                        status: "completed",
                        atMs: finishedAt,
                        startedAtMs: requestedAt,
                    });
                    append({ id: nextId(), who: "you", text });
                    speak("Stopped.");
                    setPhase("listening");
                    return;
                }
                if (intent.kind === "approve" || intent.kind === "decline") {
                    append({ id: nextId(), who: "you", text });
                    await respond(intent.kind === "approve");
                    return;
                }
                append({ id: nextId(), who: "you", text: intent.text });
                setError(null);
                setPhase("working");
                const requestedAt = Date.now();
                const stepId = "instruction-request";
                const traceId = beginOperatorActivity({
                    id: stepId,
                    label: "Sending instruction to computer use",
                    actor: "Computer use",
                    status: "running",
                    evidence: "ipc-boundary",
                    atMs: requestedAt,
                });
                await computerUseSay(intent.text);
                const finishedAt = Date.now();
                setActivityTrace((current) => {
                    if (current?.id !== traceId) return current;
                    const latest = current.steps[current.steps.length - 1];
                    if (latest?.id !== stepId) return current;
                    return recordActivityStep(current, {
                        ...latest,
                        label: "Instruction accepted by computer use",
                        status: "waiting",
                        atMs: finishedAt,
                        durationMs: Math.max(0, finishedAt - requestedAt),
                    });
                });
            } catch (reason) {
                const message = reason instanceof Error ? reason.message : String(reason);
                setError(message);
                speak(message);
                setPhase("listening");
                const failedAt = Date.now();
                setActivityTrace((current) => {
                    if (!current) return current;
                    const latest = current.steps[current.steps.length - 1];
                    if (!latest || (latest.status !== "running" && latest.status !== "waiting")) {
                        return current;
                    }
                    return recordActivityStep(current, {
                        ...latest,
                        label: latest.id.startsWith("interrupt-")
                            ? "Interruption request failed"
                            : "Computer-use request failed",
                        status: "failed",
                        evidence: "ipc-boundary",
                        atMs: failedAt,
                        durationMs: Math.max(0, failedAt - latest.atMs),
                    });
                });
            }
        },
        [append, beginOperatorActivity, finishCurrentActivityStep, respond, speak],
    );

    // Hear and speak only while this mode is on screen.
    useEffect(() => {
        if (!active) return;
        let live = true;
        const startedAt = Date.now();
        const traceId = beginOperatorActivity({
            id: "listener",
            label: "Starting voice control",
            actor: "Speech listener",
            status: "running",
            evidence: "frontend-event",
            atMs: startedAt,
        });
        const speaker = new Speaker();
        const listener = new DuplexListener(speaker, {
            onUtterance: (text) => void handleUtterance(text),
            onBargeIn: () => setPartial(""),
            onPartial: setPartial,
            onError: (message) => {
                setError(message);
                recordOperatorActivity({
                    id: `listener-error-${++activityStepSeqRef.current}`,
                    label: "Voice listener reported an error",
                    actor: "Speech listener",
                    status: "failed",
                    evidence: "frontend-event",
                    atMs: Date.now(),
                });
            },
        });
        speakerRef.current = speaker;
        listenerRef.current = listener;
        listener
            .start()
            .then(() => {
                if (!live) return;
                setPhase("listening");
                onStreamChange?.(listener.mediaStream);
                const readyAt = Date.now();
                setActivityTrace((current) => {
                    if (current?.id !== traceId) return current;
                    const withStart = recordActivityStep(current, {
                        id: "listener",
                        label: "Voice control started",
                        actor: "Speech listener",
                        status: "completed",
                        evidence: "frontend-event",
                        atMs: readyAt,
                        durationMs: Math.max(0, readyAt - startedAt),
                    });
                    return recordActivityStep(withStart, {
                        id: "listening",
                        label: "Listening for an instruction",
                        actor: "Speech listener",
                        status: "waiting",
                        evidence: "frontend-event",
                        atMs: readyAt,
                    });
                });
            })
            .catch((reason) => {
                if (!live) return;
                setError(reason instanceof Error ? reason.message : String(reason));
                const failedAt = Date.now();
                setActivityTrace((current) => current?.id === traceId
                    ? recordActivityStep(current, {
                        id: "listener",
                        label: "Voice control failed to start",
                        actor: "Speech listener",
                        status: "failed",
                        evidence: "frontend-event",
                        atMs: failedAt,
                        durationMs: Math.max(0, failedAt - startedAt),
                    })
                    : current);
            });
        return () => {
            live = false;
            listener.stop();
            speaker.cancel();
            onStreamChange?.(null);
            speakerRef.current = null;
            listenerRef.current = null;
        };
    }, [active, beginOperatorActivity, handleUtterance, onStreamChange, recordOperatorActivity]);

    // Leaving Do mode ends the Codex conversation.
    useEffect(() => () => void computerUseStop().catch(() => undefined), []);

    useTauriEvent<ComputerUseEvent>(COMPUTER_USE_EVENT, (event) => {
        switch (event.kind) {
            case "message":
                append({ id: nextId(), who: "fndr", text: event.text });
                speak(event.text);
                recordOperatorActivity({
                    id: `message-${++activityStepSeqRef.current}`,
                    label: event.final
                        ? "Computer use returned a final response"
                        : "Computer use reported progress",
                    actor: "Computer use",
                    status: event.final ? "completed" : "running",
                    evidence: "backend-event",
                    atMs: Date.now(),
                });
                break;
            case "action":
                append({ id: event.itemId, who: "action", text: event.summary, state: "running" });
                setPhase("working");
                {
                    const stepId = `action-${++activityStepSeqRef.current}`;
                    actionStepIdsRef.current.set(event.itemId, stepId);
                    recordOperatorActivity({
                        id: stepId,
                        label: "Computer action started",
                        actor: "Computer use",
                        status: "running",
                        evidence: "backend-event",
                        atMs: Date.now(),
                    });
                }
                break;
            case "actionDone":
                setLog((current) =>
                    current.map((entry) =>
                        entry.who === "action" && entry.id === event.itemId
                            ? { ...entry, state: event.ok ? "done" : "failed" }
                            : entry,
                    ),
                );
                {
                    const stepId = actionStepIdsRef.current.get(event.itemId)
                        ?? `action-${++activityStepSeqRef.current}`;
                    actionStepIdsRef.current.delete(event.itemId);
                    recordOperatorActivity({
                        id: stepId,
                        label: event.ok ? "Computer action completed" : "Computer action failed",
                        actor: "Computer use",
                        status: event.ok ? "completed" : "failed",
                        evidence: "backend-event",
                        atMs: Date.now(),
                    });
                }
                break;
            case "approval":
                setPending({ requestKey: event.requestKey, summary: event.summary });
                setPhase("waiting");
                speak(`Okay to ${event.summary}?`);
                recordOperatorActivity({
                    id: `approval-${++activityStepSeqRef.current}`,
                    label: "Waiting for action approval",
                    actor: "Approval gate",
                    status: "waiting",
                    evidence: "backend-event",
                    atMs: Date.now(),
                });
                break;
            case "approvalResolved":
                setPending((current) => (current?.requestKey === event.requestKey ? null : current));
                recordOperatorActivity({
                    id: `approval-resolved-${++activityStepSeqRef.current}`,
                    label: "Action approval resolved",
                    actor: "Approval gate",
                    status: "completed",
                    evidence: "backend-event",
                    atMs: Date.now(),
                });
                break;
            case "turnDone":
                setPhase("listening");
                actionStepIdsRef.current.clear();
                recordOperatorActivity({
                    id: `turn-${++activityStepSeqRef.current}`,
                    label: event.status === "failed"
                        ? "Computer-use turn failed"
                        : event.status === "interrupted"
                          ? "Computer-use turn interrupted"
                          : "Computer-use turn completed",
                    actor: "Computer use",
                    status: event.status === "failed"
                        ? "failed"
                        : event.status === "interrupted"
                          ? "cancelled"
                          : "completed",
                    evidence: "backend-event",
                    atMs: Date.now(),
                });
                if (event.status === "failed" && event.error) {
                    setError(event.error);
                    speak("Something went wrong. " + event.error);
                }
                break;
            case "ended":
                setPending(null);
                setPhase("listening");
                actionStepIdsRef.current.clear();
                recordOperatorActivity({
                    id: `session-ended-${++activityStepSeqRef.current}`,
                    label: event.error
                        ? "Computer-use session ended with an error"
                        : "Computer-use session ended",
                    actor: "Computer use",
                    status: event.error ? "failed" : "completed",
                    evidence: "backend-event",
                    atMs: Date.now(),
                });
                if (event.error) setError(event.error);
                break;
            case "ready":
                recordOperatorActivity({
                    id: `session-ready-${++activityStepSeqRef.current}`,
                    label: "Computer-use session ready",
                    actor: "Computer use",
                    status: "completed",
                    evidence: "backend-event",
                    atMs: Date.now(),
                });
                break;
        }
    });

    const status =
        phase === "starting"
            ? "Starting…"
            : phase === "waiting"
              ? "Waiting for your okay"
              : phase === "working"
                ? "Working — say “stop” anytime"
                : partial
                  ? partial
                  : "Listening — tell me what to do";

    return (
        <div className="notch-operator">
            <p
                className="notch-operator-status"
                role={activityTrace ? undefined : "status"}
                aria-live={activityTrace ? undefined : "polite"}
            >
                {phase === "working" ? <ThinkingOrb state="working" size={20} theme="dark" /> : null}
                <span>{status}</span>
            </p>

            {activityTrace ? (
                <ActivityTrace trace={activityTrace} className="notch-activity-trace" />
            ) : null}

            {log.length > 0 ? (
                <ol className="notch-operator-log">
                    {log.map((entry) => (
                        <li key={entry.id} className={`notch-operator-entry notch-operator-${entry.who}`}>
                            {entry.who === "action" ? (
                                <>
                                    <span className={`notch-operator-dot is-${entry.state}`} aria-hidden="true" />
                                    <span>{entry.text}</span>
                                </>
                            ) : (
                                entry.text
                            )}
                        </li>
                    ))}
                </ol>
            ) : null}

            {pending ? (
                <div className="notch-operator-approval" role="alertdialog" aria-label="Approve action">
                    <p>
                        Okay to <strong>{pending.summary}</strong>?
                    </p>
                    <div className="notch-operator-approval-actions">
                        <button type="button" className="notch-operator-btn" onClick={() => void respond(false)}>
                            Don&apos;t
                        </button>
                        <button
                            type="button"
                            className="notch-operator-btn notch-operator-btn-primary"
                            onClick={() => void respond(true)}
                        >
                            Allow
                        </button>
                    </div>
                </div>
            ) : null}

            {error ? (
                <p className="notch-voice-error" role="alert">
                    {error}
                </p>
            ) : null}

            <form
                className="notch-operator-type"
                onSubmit={(event) => {
                    event.preventDefault();
                    const text = draft.trim();
                    if (!text) return;
                    setDraft("");
                    void handleUtterance(text);
                }}
            >
                <input
                    className="notch-input"
                    value={draft}
                    placeholder="Or type an instruction"
                    aria-label="Instruction for FNDR"
                    onChange={(event) => setDraft(event.target.value)}
                />
                {phase === "working" || pending ? (
                    <button
                        type="button"
                        className="notch-operator-btn notch-operator-stop"
                        onClick={() => void handleUtterance("stop")}
                    >
                        Stop
                    </button>
                ) : null}
            </form>
        </div>
    );
}
