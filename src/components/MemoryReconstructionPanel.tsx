import { useEffect, useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { MemoryReconstruction, reconstructMemory, SearchResult } from "../api/tauri";
import "./MemoryReconstructionPanel.css";

interface MemoryReconstructionPanelProps {
    query: string;
    selectedResult: SearchResult | null;
    onShowContext: (value: string) => void;
}

export function MemoryReconstructionPanel({
    query,
    selectedResult,
    onShowContext,
}: MemoryReconstructionPanelProps) {
    const [data, setData] = useState<MemoryReconstruction | null>(null);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const activeQuery = query.trim();

    useEffect(() => {
        if (!activeQuery) {
            setData(null);
            setError(null);
            setIsLoading(false);
            return;
        }

        const timer = setTimeout(async () => {
            setIsLoading(true);
            setError(null);
            try {
                const reconstruction = await reconstructMemory(activeQuery, 8);
                setData(reconstruction);
            } catch (err) {
                console.error("Failed to reconstruct memory:", err);
                setError("Unable to reconstruct memory context.");
                setData(null);
            } finally {
                setIsLoading(false);
            }
        }, 350);

        return () => clearTimeout(timer);
    }, [activeQuery]);

    const hasCards = useMemo(() => (data?.cards?.length ?? 0) > 0, [data]);
    const primaryCard = data?.cards?.[0];

    if (!activeQuery) {
        return null;
    }

    return (
        <aside className="reconstruction-panel" aria-live="polite">
            <header className="reconstruction-header">
                <div className="reconstruction-badge">Preview</div>
                <h3>{selectedResult?.window_title || "Memory preview"}</h3>
                <p>
                    {selectedResult?.app_name ?? "Local capture"} ·{" "}
                    {selectedResult
                        ? new Date(selectedResult.timestamp).toLocaleString(undefined, {
                            month: "short",
                            day: "numeric",
                            hour: "2-digit",
                            minute: "2-digit",
                        })
                        : "No timestamp"}
                </p>
            </header>

            {isLoading && (
                <div className="reconstruction-loading">
                    <div className="spinner" />
                    <span>Synthesizing memory graph context...</span>
                </div>
            )}

            {!isLoading && error && <p className="reconstruction-error">{error}</p>}

            {!isLoading && data && (
                <div className="reconstruction-body">
                    <section className="reconstruction-answer">
                        <h4>Excerpt</h4>
                        <p>{selectedResult?.text || data.answer}</p>
                    </section>

                    <section className="reconstruction-structural">
                        <h4>Source</h4>
                        <ul>
                            <li>Match score: {selectedResult ? `${Math.round(selectedResult.score * 100)}%` : "-"}</li>
                            <li>Cards found: {data.cards.length}</li>
                            {data.structural_context[0] && <li>{data.structural_context[0]}</li>}
                        </ul>
                    </section>

                    <section className="reconstruction-cards">
                        <h4>Actions</h4>
                        {!hasCards && <p className="empty-cards">No linked cards available.</p>}

                        {primaryCard && (
                            <article className="reconstruction-card" key={primaryCard.id}>
                                {primaryCard.screenshot_path && (
                                    <img
                                        src={convertFileSrc(primaryCard.screenshot_path)}
                                        alt={primaryCard.window_title}
                                        className="reconstruction-shot"
                                        loading="lazy"
                                    />
                                )}
                                <div className="reconstruction-card-meta">
                                    <div className="meta-top">
                                        <span className="app">{primaryCard.app_name}</span>
                                        <span className="score">{Math.round(primaryCard.score * 100)}%</span>
                                    </div>
                                    <div className="window">{primaryCard.window_title}</div>
                                    <p className="snippet">{primaryCard.snippet}</p>
                                    <div className="reconstruction-actions">
                                        {primaryCard.url && (
                                            <a href={primaryCard.url} target="_blank" rel="noreferrer">
                                                Open source
                                            </a>
                                        )}
                                        <button
                                            type="button"
                                            className="context-btn"
                                            onClick={() => onShowContext(primaryCard.snippet || query)}
                                        >
                                            Show context
                                        </button>
                                    </div>
                                </div>
                            </article>
                        )}
                    </section>
                </div>
            )}
        </aside>
    );
}
