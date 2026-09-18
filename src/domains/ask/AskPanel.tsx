import { useEffect, useRef, useState } from "react";
import { fndrAnswer, type ComposedAnswer, type MemoryCard } from "@/shared/ipc/tauri";
import "./AskPanel.css";

const ANSWER_TIMEOUT_MS = 60_000;
const TAKEAWAY =
    "FNDR remembers what you did on your Mac, privately and on-device, and answers questions about it with cited evidence.";
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
    const seq = useRef(0);
    const inputRef = useRef<HTMLTextAreaElement>(null);

    useEffect(() => {
        if (isVisible) window.setTimeout(() => inputRef.current?.focus(), 30);
    }, [isVisible]);

    if (!isVisible) return null;

    const ask = async (text: string) => {
        const question = text.trim();
        if (!question) return;
        const id = ++seq.current;
        setState({ kind: "asking", question });
        try {
            const result = await Promise.race([
                fndrAnswer(question),
                new Promise<never>((_, reject) =>
                    window.setTimeout(() => reject(new Error("timeout")), ANSWER_TIMEOUT_MS)
                ),
            ]);
            if (id === seq.current) setState({ kind: "answer", question, answer: result });
        } catch (err) {
            if (id !== seq.current) return;
            const timedOut = err instanceof Error && err.message === "timeout";
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
        <div className="ask-page" role="dialog" aria-label="Ask FNDR">
            <header className="ask-header">
                <div className="ask-header-title">
                    <h2>Ask FNDR</h2>
                    <span className="ask-badge">On-device · read-only</span>
                </div>
                <button type="button" className="ui-action-btn ask-close-btn" onClick={onClose} aria-label="Close Ask FNDR">
                    ×
                </button>
            </header>

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
                        aria-label="Ask FNDR a question"
                        placeholder="Ask about anything you've worked on…"
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
                        Ask
                    </button>
                </form>

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
                    <div className="ask-status" role="status">
                        <div className="thinking-loader" aria-hidden="true" />
                        <span>Searching your memories…</span>
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
