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

    const speakerRef = useRef<Speaker | null>(null);
    const listenerRef = useRef<DuplexListener | null>(null);
    const pendingRef = useRef<PendingApproval | null>(null);
    pendingRef.current = pending;

    const append = useCallback((entry: LogEntry) => {
        setLog((current) => [...current, entry].slice(-MAX_LOG));
    }, []);

    const speak = useCallback((text: string) => speakerRef.current?.speak(text), []);

    const respond = useCallback(
        async (approve: boolean) => {
            const current = pendingRef.current;
            if (!current) return;
            setPending(null);
            speakerRef.current?.cancel();
            try {
                await computerUseRespond(current.requestKey, approve);
                if (!approve) speak("Okay, I won't.");
            } catch (reason) {
                setError(reason instanceof Error ? reason.message : String(reason));
            }
        },
        [speak],
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
                    await computerUseInterrupt();
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
                await computerUseSay(intent.text);
            } catch (reason) {
                const message = reason instanceof Error ? reason.message : String(reason);
                setError(message);
                speak(message);
                setPhase("listening");
            }
        },
        [append, respond, speak],
    );

    // Hear and speak only while this mode is on screen.
    useEffect(() => {
        if (!active) return;
        const speaker = new Speaker();
        const listener = new DuplexListener(speaker, {
            onUtterance: (text) => void handleUtterance(text),
            onBargeIn: () => setPartial(""),
            onPartial: setPartial,
            onError: setError,
        });
        speakerRef.current = speaker;
        listenerRef.current = listener;
        listener
            .start()
            .then(() => {
                setPhase("listening");
                onStreamChange?.(listener.mediaStream);
            })
            .catch((reason) => setError(reason instanceof Error ? reason.message : String(reason)));
        return () => {
            listener.stop();
            speaker.cancel();
            onStreamChange?.(null);
            speakerRef.current = null;
            listenerRef.current = null;
        };
    }, [active, handleUtterance, onStreamChange]);

    // Leaving Do mode ends the Codex conversation.
    useEffect(() => () => void computerUseStop().catch(() => undefined), []);

    useTauriEvent<ComputerUseEvent>(COMPUTER_USE_EVENT, (event) => {
        switch (event.kind) {
            case "message":
                append({ id: nextId(), who: "fndr", text: event.text });
                speak(event.text);
                break;
            case "action":
                append({ id: event.itemId, who: "action", text: event.summary, state: "running" });
                setPhase("working");
                break;
            case "actionDone":
                setLog((current) =>
                    current.map((entry) =>
                        entry.who === "action" && entry.id === event.itemId
                            ? { ...entry, state: event.ok ? "done" : "failed" }
                            : entry,
                    ),
                );
                break;
            case "approval":
                setPending({ requestKey: event.requestKey, summary: event.summary });
                setPhase("waiting");
                speak(`Okay to ${event.summary}?`);
                break;
            case "approvalResolved":
                setPending((current) => (current?.requestKey === event.requestKey ? null : current));
                break;
            case "turnDone":
                setPhase("listening");
                if (event.status === "failed" && event.error) {
                    setError(event.error);
                    speak("Something went wrong. " + event.error);
                }
                break;
            case "ended":
                setPending(null);
                setPhase("listening");
                if (event.error) setError(event.error);
                break;
            case "ready":
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
            <p className="notch-operator-status" role="status" aria-live="polite">
                {phase === "working" ? <ThinkingOrb state="working" size={20} theme="dark" /> : null}
                <span>{status}</span>
            </p>

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
