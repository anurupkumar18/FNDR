// Inspired by CC's AgentTool + WebSearchTool pattern:
// Takes a memory card as seed context, generates research angles,
// then runs the AI agent with those angles as a structured task.
// The agent loop polls status and streams progress just like CC's agent sub-tasks.
import { useCallback, useEffect, useRef, useState } from "react";
import {
    MemoryCard,
    getAgentStatus,
    startAgentTask,
    stopAgent,
} from "../api/tauri";
import "./ResearchPanel.css";

// Research angles are like CC's sub-agent prompts — each angle is a named,
// focused question derived from the memory context.
interface ResearchAngle {
    id: string;
    label: string;
    prompt: string;
}

function buildAngles(memory: MemoryCard | null, freeQuery: string): ResearchAngle[] {
    if (!memory && !freeQuery) return [];

    const base = memory
        ? `${memory.title} (${memory.app_name}) — ${memory.summary.slice(0, 200)}`
        : freeQuery;

    const hasUrl = !!memory?.url;
    const isCode = memory
        ? ["code", "cursor", "vim", "terminal", "iterm"].some((k) =>
              memory.app_name.toLowerCase().includes(k)
          )
        : false;

    const angles: ResearchAngle[] = [
        {
            id: "deep-dive",
            label: "Deep dive",
            prompt: `Research the following topic in depth and provide a structured summary with key facts, recent developments, and actionable insights:\n\n${base}`,
        },
        {
            id: "related-context",
            label: "Related context",
            prompt: `Find related concepts, background knowledge, and adjacent topics to understand this more fully:\n\n${base}`,
        },
    ];

    if (hasUrl) {
        angles.push({
            id: "current-state",
            label: "What's changed?",
            prompt: `I visited ${memory!.url} and captured this context. Research what has changed recently or what the current state is:\n\n${base}`,
        });
    }

    if (isCode) {
        angles.push({
            id: "best-practices",
            label: "Best practices",
            prompt: `Provide best practices, common pitfalls, and expert recommendations related to this code context:\n\n${base}`,
        });
        angles.push({
            id: "alternatives",
            label: "Alternatives",
            prompt: `What are the modern alternatives or competing approaches to what I was working on? Compare tradeoffs:\n\n${base}`,
        });
    }

    angles.push({
        id: "action-items",
        label: "Action items",
        prompt: `Based on this context, what are the most important follow-up actions, decisions, or things to investigate:\n\n${base}`,
    });

    return angles;
}

// ── Component ─────────────────────────────────────────────────────────────────

interface ResearchPanelProps {
    isVisible: boolean;
    onClose: () => void;
    seedMemory: MemoryCard | null;
}

type ResearchPhase = "idle" | "selecting" | "running" | "done" | "error";

export function ResearchPanel({ isVisible, onClose, seedMemory }: ResearchPanelProps) {
    const [freeQuery, setFreeQuery] = useState("");
    const [selectedAngle, setSelectedAngle] = useState<ResearchAngle | null>(null);
    const [phase, setPhase] = useState<ResearchPhase>("idle");
    const [log, setLog] = useState<string[]>([]);
    const pollRef = useRef<number | null>(null);
    const logRef = useRef<HTMLDivElement>(null);

    const angles = buildAngles(seedMemory, freeQuery);

    // Auto-select "deep dive" when memory changes
    useEffect(() => {
        if (seedMemory) {
            setSelectedAngle(null);
            setPhase("idle");
            setLog([]);
        }
    }, [seedMemory]);

    // Scroll log to bottom
    useEffect(() => {
        logRef.current?.scrollTo({ top: logRef.current.scrollHeight, behavior: "smooth" });
    }, [log]);

    const startPolling = useCallback(() => {
        if (pollRef.current !== null) return;
        pollRef.current = window.setInterval(async () => {
            try {
                const status = await getAgentStatus();

                if (status.last_message) {
                    const msg = status.last_message;
                    setLog((prev) => {
                        const last = prev[prev.length - 1];
                        if (last === msg) return prev;
                        return [...prev, msg];
                    });
                }

                if (status.status === "completed" || status.status === "error") {
                    if (pollRef.current !== null) {
                        window.clearInterval(pollRef.current);
                        pollRef.current = null;
                    }
                    setPhase(status.status === "completed" ? "done" : "error");
                }
            } catch {
                // transient
            }
        }, 800);
    }, []);

    const stopPolling = useCallback(() => {
        if (pollRef.current !== null) {
            window.clearInterval(pollRef.current);
            pollRef.current = null;
        }
    }, []);

    useEffect(() => {
        if (!isVisible) stopPolling();
        return stopPolling;
    }, [isVisible, stopPolling]);

    const runResearch = async (angle: ResearchAngle) => {
        setSelectedAngle(angle);
        setPhase("running");
        setLog([`Starting: ${angle.label}…`]);

        try {
            await startAgentTask(
                angle.prompt,
                seedMemory?.url ? [seedMemory.url] : undefined,
                seedMemory ? [seedMemory.summary] : undefined
            );
            startPolling();
        } catch (err) {
            setLog((prev) => [...prev, `Error: ${err instanceof Error ? err.message : String(err)}`]);
            setPhase("error");
        }
    };

    const handleStop = async () => {
        stopPolling();
        try { await stopAgent(); } catch { /* ignore */ }
        setPhase("idle");
    };

    const handleReset = () => {
        setPhase("idle");
        setSelectedAngle(null);
        setLog([]);
    };

    if (!isVisible) return null;

    return (
        <div className="rp-page">
            <header className="rp-header">
                <div>
                    <h2>Research</h2>
                    <p>AI-powered deep-dive seeded from your memories</p>
                </div>
                <button className="ui-action-btn rp-close-btn" onClick={onClose}>X</button>
            </header>

            <div className="rp-body">
                {/* Seed display */}
                {seedMemory ? (
                    <div className="rp-seed-card">
                        <div className="rp-seed-icon">MEM</div>
                        <div className="rp-seed-content">
                            <div className="rp-seed-label">Researching from memory</div>
                            <div className="rp-seed-title">{seedMemory.title}</div>
                            <div className="rp-seed-meta">{seedMemory.app_name} · {new Date(seedMemory.timestamp).toLocaleString()}</div>
                        </div>
                    </div>
                ) : (
                    <div className="rp-free-query">
                        <input
                            className="rp-query-input"
                            placeholder="Enter a topic to research…"
                            value={freeQuery}
                            onChange={(e) => setFreeQuery(e.target.value)}
                            disabled={phase === "running"}
                            autoFocus
                        />
                    </div>
                )}

                {/* Research angles — like CC's sub-agent selection */}
                {phase === "idle" && (angles.length > 0 || freeQuery.trim()) && (
                    <div className="rp-angles">
                        <div className="rp-angles-label">Choose a research angle</div>
                        <div className="rp-angles-grid">
                            {(freeQuery.trim() && !seedMemory
                                ? buildAngles(null, freeQuery)
                                : angles
                            ).map((angle) => (
                                <button
                                    key={angle.id}
                                    className="rp-angle-card"
                                    onClick={() => void runResearch(angle)}
                                >
                                    <span className="rp-angle-label">{angle.label}</span>
                                </button>
                            ))}
                        </div>
                    </div>
                )}

                {/* Running state — streaming agent output */}
                {(phase === "running" || phase === "done" || phase === "error") && selectedAngle && (
                    <div className="rp-run-container">
                        <div className="rp-run-header">
                            <span className="rp-run-label">{selectedAngle.label}</span>
                            <div className={`rp-status-pill ${phase}`}>
                                {phase === "running" && <span className="rp-status-dot" />}
                                {phase === "running" ? "Running" : phase === "done" ? "Done" : "Error"}
                            </div>
                        </div>

                        {/* Log stream — like CC's streaming tool output */}
                        <div className="rp-log" ref={logRef}>
                            {log.map((line, i) => (
                                <div key={i} className={`rp-log-line ${i === log.length - 1 && phase === "running" ? "active" : ""}`}>
                                    {phase === "running" && i === log.length - 1 && (
                                        <span className="rp-log-cursor" />
                                    )}
                                    {line}
                                </div>
                            ))}
                            {phase === "running" && log.length === 0 && (
                                <div className="rp-log-line active">
                                    <span className="rp-log-cursor" />
                                    Initializing agent…
                                </div>
                            )}
                        </div>

                        {/* Agent actions */}
                        <div className="rp-run-actions">
                            {phase === "running" && (
                                <button className="ui-action-btn rp-stop-btn" onClick={() => void handleStop()}>
                                    ⏹ Stop
                                </button>
                            )}
                            {(phase === "done" || phase === "error") && (
                                <>
                                    <button className="ui-action-btn rp-reset-btn" onClick={handleReset}>
                                        ← New research
                                    </button>
                                    {phase === "done" && (
                                        <span className="rp-done-hint">Check the Agent panel for full results.</span>
                                    )}
                                </>
                            )}
                        </div>
                    </div>
                )}

                {/* Empty state */}
                {!seedMemory && !freeQuery.trim() && phase === "idle" && (
                    <div className="rp-empty">
                        <p>Open from a memory card to seed automatically,</p>
                        <p>or type a topic above to start research.</p>
                    </div>
                )}
            </div>
        </div>
    );
}
