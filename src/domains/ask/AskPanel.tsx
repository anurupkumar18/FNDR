import { useRef, useState } from "react";
import { fndrAnswer, type ComposedAnswer, type MemoryCard } from "@/shared/ipc/tauri";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import "./AskPanel.css";
import { ThinkingIndicator } from "@/shared/components/ThinkingIndicator";
import { PanelHeader } from "@/shared/components/PanelHeader";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
} from "@/shared/activity/activityTrace";

const ANSWER_TIMEOUT_MS = 60_000;
const TAKEAWAY =
    "Search finds memories. Ask FNDR uses that same private, on-device evidence to give a cited answer.";
const EXAMPLES = [
    "What was the Rust borrow error I fixed this week?",
    "What chunk size did the chunking paper recommend?",
    "When is the alpha dry run?",
];

type AskState =
    | { kind: "idle" }
    | { kind: "asking"; question: string }
    | { kind: "answer"; question: string; answer: ComposedAnswer }
    | { kind: "error"; question: string; message: string };

interface AskPanelProps {
    isVisible: boolean;
    onClose: () => void;
    onOpenMemoryById: (id: string) => void;
}

function outcomeLabel(answer: ComposedAnswer): string {
    const outcome = answer.verify_outcome;
    if (outcome.kind === "not_enough_evidence") return "Not in your memories";
    if (outcome.kind === "partial_answer") return "Partial answer";
    const n = answer.cards.length;
    return `Grounded in ${n} ${n === 1 ? "memory" : "memories"}`;
}

function sourceTime(card: MemoryCard): string {
    const d = new Date(card.timestamp);
    return `${d.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" })} · ${d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}`;
}

/** Ask FNDR: grounded, citation-validated answers from local memories via
 *  `fndr_answer`, with an honest refusal when evidence is missing. */
export function AskPanel({ isVisible, onClose, onOpenMemoryById }: AskPanelProps) {
    const [draft, setDraft] = useState("");
    const [state, setState] = useState<AskState>({ kind: "idle" });
    const [activityTrace, setActivityTrace] = useState<ActivityTraceSnapshot | null>(null);
    const seq = useRef(0);
    const dialogRef = useRef<HTMLDivElement>(null);
    const inputRef = useRef<HTMLTextAreaElement>(null);
    useModalFocus(isVisible, dialogRef, inputRef, onClose);

    if (!isVisible) return null;

    const ask = async (text: string) => {
        const question = text.trim();
        if (!question) return;
        const id = ++seq.current;
        const startedAtMs = Date.now();
        let nextTrace = beginActivityTrace({
            id: `ask-${id}`,
            title: "Ask FNDR activity",
            startedAtMs,
        });
        nextTrace = recordActivityStep(nextTrace, {
            id: "request",
            label: "Requesting an answer from local memory",
            actor: "FNDR answer service",
            status: "running",
            evidence: "ipc-boundary",
            atMs: startedAtMs,
        });
        setActivityTrace(nextTrace);
        setState({ kind: "asking", question });
        try {
            const result = await Promise.race([
                fndrAnswer(question),
                new Promise<never>((_, reject) =>
                    window.setTimeout(() => reject(new Error("timeout")), ANSWER_TIMEOUT_MS)
                ),
            ]);
            if (id === seq.current) {
                const finishedAtMs = Date.now();
                const routes = Array.from(new Set(
                    result.cards.flatMap((card) => card.matched_routes ?? []),
                )).slice(0, 5);
                const count = result.cards.length;
                const resultDetail = [
                    `${count} ${count === 1 ? "memory" : "memories"}`,
                    routes.length > 0 ? routes.join(" + ") : null,
                ].filter(Boolean).join(" · ");
                setActivityTrace((current) => {
                    if (!current || current.id !== `ask-${id}`) return current;
                    const withRequest = recordActivityStep(current, {
                        id: "request",
                        label: "Local-memory answer request completed",
                        actor: "FNDR answer service",
                        status: "completed",
                        evidence: "ipc-boundary",
                        atMs: finishedAtMs,
                        durationMs: Math.max(0, finishedAtMs - startedAtMs),
                    });
                    const resultLabel = result.verify_outcome.kind === "grounded"
                        ? "Grounded answer returned"
                        : result.verify_outcome.kind === "partial_answer"
                            ? "Partial answer returned"
                            : "Answer returned with limited evidence";
                    return recordActivityStep(withRequest, {
                        id: "result",
                        label: resultLabel,
                        actor: "FNDR answer service",
                        status: result.verify_outcome.kind === "grounded" ? "completed" : "degraded",
                        evidence: "result-metadata",
                        atMs: finishedAtMs,
                        detail: resultDetail,
                    });
                });
                setState({ kind: "answer", question, answer: result });
            }
        } catch (err) {
            if (id !== seq.current) return;
            const timedOut = err instanceof Error && err.message === "timeout";
            const failedAtMs = Date.now();
            setActivityTrace((current) => {
                if (!current || current.id !== `ask-${id}`) return current;
                return recordActivityStep(current, {
                    id: "request",
                    label: timedOut ? "Answer timed out" : "Answer failed",
                    actor: "FNDR answer service",
                    status: "failed",
                    evidence: "ipc-boundary",
                    atMs: failedAtMs,
                    durationMs: Math.max(0, failedAtMs - startedAtMs),
                    detail: timedOut ? "Client timeout" : "Backend request failed",
                });
            });
            setState({
                kind: "error",
                question,
                message: timedOut
                    ? "FNDR took too long to answer. Try a shorter question."
                    : "FNDR couldn't answer right now. Try again in a moment.",
            });
        }
    };

    return (
        <div
            ref={dialogRef}
            className="ask-page"
            role="dialog"
            aria-modal="true"
            aria-label="Search and Ask FNDR"
        >
            <PanelHeader
                title="Search & Ask FNDR"
                subtitle="On-device and read-only."
                closeLabel="Close Ask FNDR"
                onClose={onClose}
            />

            <main className="ask-body">
                <p className="ask-takeaway">{TAKEAWAY}</p>

                <form
                    className="ask-form"
                    onSubmit={(event) => {
                        event.preventDefault();
                        void ask(draft);
                    }}
                >
                    <textarea
                        ref={inputRef}
                        className="ask-input"
                        aria-label="Search or ask FNDR"
                        placeholder="Search or ask about anything you've worked on…"
                        value={draft}
                        rows={2}
                        onChange={(event) => setDraft(event.target.value)}
                        onKeyDown={(event) => {
                            if (event.key === "Enter" && !event.shiftKey) {
                                event.preventDefault();
                                void ask(draft);
                            }
                        }}
                    />
                    <button type="submit" className="ui-action-btn btn-primary ask-submit" disabled={state.kind === "asking"}>
                        Search & Ask
                    </button>
                </form>

                {activityTrace && (
                    <ActivityTrace
                        trace={activityTrace}
                        className="ask-activity-trace"
                        announce={state.kind === "asking"}
                    />
                )}

                {state.kind === "idle" && (
                    <div className="ask-examples" aria-label="Example questions">
                        {EXAMPLES.map((example) => (
                            <button
                                key={example}
                                type="button"
                                className="ask-example"
                                onClick={() => {
                                    setDraft(example);
                                    void ask(example);
                                }}
                            >
                                {example}
                            </button>
                        ))}
                    </div>
                )}

                {state.kind === "asking" && (
                    <div className="ask-status">
                        <ThinkingIndicator state="searching" size="lg" />
                        <span>Searching and checking your memories…</span>
                    </div>
                )}

                {state.kind === "error" && (
                    <div className="ask-result ask-result--error" role="alert">
                        <p>{state.message}</p>
                    </div>
                )}

                {state.kind === "answer" && (
                    <section className="ask-result" aria-live="polite">
                        <div className={`ask-outcome ask-outcome--${state.answer.verify_outcome.kind}`}>
                            {outcomeLabel(state.answer)}
                        </div>
                        <p className="ask-answer">{state.answer.answer}</p>
                        {state.answer.verify_outcome.kind === "not_enough_evidence" && (
                            <p className="ask-hint">FNDR only answers from what it captured on this Mac.</p>
                        )}
                        {state.answer.cards.length > 0 && (
                            <div className="ask-sources">
                                <h3>Sources</h3>
                                <ul>
                                    {state.answer.cards.slice(0, 5).map((card) => (
                                        <li key={card.id}>
                                            <button
                                                type="button"
                                                className="ask-source"
                                                onClick={() => onOpenMemoryById(card.id)}
                                            >
                                                <span className="ask-source-title">{card.title}</span>
                                                <span className="ask-source-meta">
                                                    {card.app_name} · {sourceTime(card)}
                                                </span>
                                            </button>
                                        </li>
                                    ))}
                                </ul>
                            </div>
                        )}
                    </section>
                )}
            </main>
        </div>
    );
}
