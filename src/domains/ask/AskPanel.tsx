import { useEffect, useRef, useState } from "react";
import { fndrAnswer, type ComposedAnswer, type MemoryCard } from "@/shared/ipc/tauri";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import { PanelState } from "@/shared/components/PanelState";
import { PanelHeader } from "@/shared/components/PanelHeader";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import { beginActivityTrace, recordActivityStep, type ActivityTraceSnapshot } from "@/shared/activity/activityTrace";
import { OpenSourceButton } from "@/shared/components/OpenSourceButton";
import "./AskPanel.css";

const ANSWER_TIMEOUT_MS = 60_000;
const EXAMPLES = [
    "What was the Rust borrow error I fixed this week?",
    "What chunk size did the chunking paper recommend?",
    "When is the alpha dry run?",
];

type Turn = { id: number; question: string } & (
    | { kind: "asking" }
    | { kind: "answer"; answer: ComposedAnswer }
    | { kind: "error"; message: string }
    | { kind: "stopped" }
);

interface AskPanelProps {
    isVisible: boolean;
    onClose: () => void;
    onOpenMemoryById: (id: string) => void;
}

function outcomeLabel(answer: ComposedAnswer): string {
    if (answer.verify_outcome.kind === "not_enough_evidence") return "Not in your memories";
    if (answer.verify_outcome.kind === "partial_answer") return "Partial answer";
    const n = answer.cards.length;
    return `Grounded in ${n} ${n === 1 ? "memory" : "memories"}`;
}

function sourceTime(card: MemoryCard): string {
    const d = new Date(card.timestamp);
    return `${d.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" })} · ${d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}`;
}

class ClientAnswerTimeout extends Error {}

/** A session-only thread over the existing on-device, read-only answer service.
 * Previous replies stay visible but are never treated as retrieval evidence. */
export function AskPanel({ isVisible, onClose, onOpenMemoryById }: AskPanelProps) {
    const [draft, setDraft] = useState("");
    const [turns, setTurns] = useState<Turn[]>([]);
    const [activityTrace, setActivityTrace] = useState<ActivityTraceSnapshot | null>(null);
    const seq = useRef(0);
    const active = useRef<{ id: number; abandon: () => void } | null>(null);
    const dialogRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLTextAreaElement>(null);
    const scrollRef = useRef<HTMLDivElement>(null);
    const followThread = useRef(true);
    const asking = turns.some((turn) => turn.kind === "asking");
    useModalFocus(isVisible, dialogRef, inputRef, onClose);

    useEffect(() => () => {
        ++seq.current;
        active.current?.abandon();
        active.current = null;
    }, []);

    useEffect(() => {
        const scroller = scrollRef.current;
        if (scroller && followThread.current) scroller.scrollTop = scroller.scrollHeight;
    }, [turns, activityTrace, isVisible]);

    const abandon = () => {
        const request = active.current;
        ++seq.current;
        active.current = null;
        request?.abandon();
        return request?.id;
    };

    const newChat = () => {
        abandon();
        setTurns([]);
        setDraft("");
        setActivityTrace(null);
        followThread.current = true;
        inputRef.current?.focus();
    };

    const stopWaiting = () => {
        const id = abandon();
        setTurns((current) => current.map((turn) => turn.id === id ? { id, question: turn.question, kind: "stopped" } : turn));
        setActivityTrace((current) => current ? recordActivityStep(current, {
            id: "request", label: "Stopped waiting for an answer", actor: "Ask FNDR",
            status: "failed", evidence: "frontend-event", atMs: Date.now(),
            detail: "The local request may still finish. Its reply will not be added.",
        }) : null);
        inputRef.current?.focus();
    };

    const ask = async (text: string, retryId?: number) => {
        const question = text.trim();
        if (!question || active.current) return;
        const id = ++seq.current;
        const turnId = retryId ?? id;
        const startedAtMs = Date.now();
        let timer: number | undefined;
        const waiting = new Promise<never>((_, reject) => {
            timer = window.setTimeout(() => reject(new ClientAnswerTimeout()), ANSWER_TIMEOUT_MS);
            active.current = { id: turnId, abandon: () => reject(new Error("Stopped waiting")) };
        });
        const updateTurn = (turn: Turn) => setTurns((current) => current.map((entry) => entry.id === turnId ? turn : entry));
        const pending: Turn = { id: turnId, question, kind: "asking" };
        setTurns((current) => retryId === undefined ? [...current, pending] : current.map((turn) => turn.id === retryId ? pending : turn));
        if (retryId === undefined) setDraft("");
        followThread.current = true;
        setActivityTrace(recordActivityStep(beginActivityTrace({ id: `ask-${id}`, title: "Ask FNDR activity", startedAtMs }), {
            id: "request", label: "Requesting an answer from local memory", actor: "FNDR answer service",
            status: "running", evidence: "ipc-boundary", atMs: startedAtMs,
        }));
        try {
            const result = await Promise.race([fndrAnswer(question), waiting]);
            if (id !== seq.current) return;
            const finishedAtMs = Date.now();
            const routes = Array.from(new Set(result.cards.flatMap((card) => card.matched_routes ?? []))).slice(0, 5);
            const count = result.cards.length;
            const resultDetail = [
                `${count} ${count === 1 ? "memory" : "memories"}`,
                routes.length ? routes.join(" + ") : null,
            ].filter(Boolean).join(" · ");
            const resultLabel = result.verify_outcome.kind === "grounded" ? "Grounded answer returned"
                : result.verify_outcome.kind === "partial_answer" ? "Partial answer returned" : "Answer returned with limited evidence";
            setActivityTrace((current) => {
                if (!current || current.id !== `ask-${id}`) return current;
                return recordActivityStep(recordActivityStep(current, {
                    id: "request", label: "Local-memory answer request completed", actor: "FNDR answer service",
                    status: "completed", evidence: "ipc-boundary", atMs: finishedAtMs,
                    durationMs: Math.max(0, finishedAtMs - startedAtMs),
                }), {
                    id: "result",
                    label: resultLabel,
                    actor: "FNDR answer service", status: result.verify_outcome.kind === "grounded" ? "completed" : "degraded",
                    evidence: "result-metadata", atMs: finishedAtMs,
                    detail: resultDetail,
                });
            });
            updateTurn({ id: turnId, question, kind: "answer", answer: result });
        } catch (err) {
            if (id !== seq.current) return;
            const timedOut = err instanceof ClientAnswerTimeout;
            const failedAtMs = Date.now();
            setActivityTrace((current) => current && current.id === `ask-${id}` ? recordActivityStep(current, {
                id: "request", label: timedOut ? "Answer timed out" : "Answer failed",
                actor: timedOut ? "Ask FNDR" : "FNDR answer service", status: "failed",
                evidence: timedOut ? "frontend-event" : "ipc-boundary", atMs: failedAtMs,
                durationMs: Math.max(0, failedAtMs - startedAtMs), detail: timedOut ? "Client timeout" : "Backend request failed",
            }) : current);
            updateTurn({ id: turnId, question, kind: "error", message: timedOut
                ? "FNDR took too long to answer. Try a shorter question."
                : "FNDR couldn't answer right now. Try again in a moment." });
        } finally {
            window.clearTimeout(timer);
            if (id === seq.current) active.current = null;
        }
    };

    if (!isVisible) return null;

    return (
        <div ref={dialogRef} className="ask-page" role="dialog" aria-modal="true" aria-label="Search and Ask FNDR">
            <PanelHeader title="Search & Ask FNDR" subtitle="On-device and read-only."
                actions={<button type="button" className="ask-new-chat" onClick={newChat}>New chat</button>}
                closeLabel="Close Ask FNDR" onClose={onClose} />
            <main className="ask-workspace">
                <div ref={scrollRef} className="ask-scroll" onScroll={(event) => {
                    const el = event.currentTarget;
                    followThread.current = el.scrollHeight - el.scrollTop - el.clientHeight < 100;
                }}>
                    <div className={`ask-thread${turns.length === 0 ? " ask-thread--empty" : ""}`}>
                        {turns.length === 0 && <section className="ask-welcome">
                            <span className="ask-mark" aria-hidden="true">fndr.</span>
                            <h3>Where did we leave off?</h3>
                            <p>Find a thought, revisit a decision, or pick up a loose thread.<br />Answers come with the memories behind them.</p>
                            <div className="ask-examples" aria-label="Example questions">
                                {EXAMPLES.map((example) => <button key={example} type="button" className="ask-example" onClick={() => void ask(example)}>{example}</button>)}
                            </div>
                        </section>}
                        <div className="ask-messages" role="log" aria-label="Conversation" aria-live="polite" aria-relevant="additions text">
                            {turns.map((turn) => <article key={turn.id} className="ask-exchange" aria-label={`Exchange: ${turn.question}`}>
                                <div className="ask-user-message"><span className="ask-speaker">You</span><p>{turn.question}</p></div>
                                <div className="ask-assistant-message">
                                    <span className="ask-speaker">FNDR</span>
                                    {turn.kind === "asking" && <PanelState kind="loading" title="Searching memories" description="Checking local memories for grounded evidence." announce={false} />}
                                    {turn.kind === "error" && <PanelState kind="error" title="Ask FNDR could not answer" description={turn.message}
                                        action={asking ? undefined : { label: "Try again", onAction: () => void ask(turn.question, turn.id) }} />}
                                    {turn.kind === "stopped" && <p className="ask-hint">Stopped waiting. The local request may still finish; its reply will not be added.</p>}
                                    {turn.kind === "answer" && <div className="ask-result">
                                        <p className="ask-answer">{turn.answer.answer}</p>
                                        <div className={`ask-outcome ask-outcome--${turn.answer.verify_outcome.kind}`}>{outcomeLabel(turn.answer)}</div>
                                        {turn.answer.verify_outcome.kind === "not_enough_evidence" && <p className="ask-hint">FNDR only answers from what it captured on this Mac.</p>}
                                        {turn.answer.cards.length > 0 && <div className="ask-sources">
                                            <h4>Sources</h4>
                                            <div className="ask-source-list">{turn.answer.cards.slice(0, 5).map((card) => <div key={card.id} className="ask-source-row"><button type="button" className="ask-source" onClick={() => onOpenMemoryById(card.id)}>
                                                <span className="ask-source-title">{card.title}</span>
                                                <span className="ask-source-meta">{card.app_name} · {sourceTime(card)}</span>
                                            </button>{card.reopen_target ? <OpenSourceButton memoryId={card.id} /> : null}</div>)}</div>
                                        </div>}
                                    </div>}
                                </div>
                            </article>)}
                        </div>
                        {activityTrace && <ActivityTrace trace={activityTrace} className="ask-activity-trace" announce={asking} />}
                    </div>
                </div>
                <div className="ask-composer-area">
                    <form className="ask-form" onSubmit={(event) => { event.preventDefault(); void ask(draft); }}>
                        <textarea ref={inputRef} className="ask-input" aria-label="Search or ask FNDR" aria-describedby="ask-context-note"
                            placeholder={turns.length ? "Ask another question about your memories…" : "Search or ask about anything you've worked on…"}
                            value={draft} rows={2} onChange={(event) => setDraft(event.target.value)}
                            onKeyDown={(event) => {
                                if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing && event.keyCode !== 229) {
                                    event.preventDefault(); void ask(draft);
                                }
                            }} />
                        <div className="ask-form-footer">
                            <span className="ask-local-label">Local memories only</span>
                            {asking ? <button type="button" className="ask-submit" onClick={stopWaiting}>Stop waiting</button>
                                : <button type="submit" className="ask-submit" disabled={!draft.trim()} aria-label="Search & Ask">
                                    <span>Send</span><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path d="M12 19V5m-6 6 6-6 6 6" /></svg>
                                </button>}
                        </div>
                    </form>
                    <p id="ask-context-note" className="ask-composer-note">Each message searches local memories independently.</p>
                    <p className="ask-session-note">This thread stays in this window until New chat or reload. Shift + Enter for a new line.</p>
                </div>
            </main>
        </div>
    );
}
