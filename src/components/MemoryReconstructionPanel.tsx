import { useEffect, useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { MemoryReconstruction, reconstructMemory } from "../api/tauri";
import "./MemoryReconstructionPanel.css";

interface MemoryReconstructionPanelProps {
    query: string;
}

export function MemoryReconstructionPanel({ query }: MemoryReconstructionPanelProps) {
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

    if (!activeQuery) {
        return null;
    }

    return (
        <aside className="reconstruction-panel" aria-live="polite">
            <header className="reconstruction-header">
                <div className="reconstruction-badge">Artifact</div>
                <h3>Memory Reconstruction</h3>
                <p>Graph-linked evidence from your local capture history.</p>
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
                        <h4>Synthesized Response</h4>
                        <p>{data.answer}</p>
                    </section>

                    {data.structural_context.length > 0 && (
                        <section className="reconstruction-structural">
                            <h4>Graph Context</h4>
                            <ul>
                                {data.structural_context.map((note, idx) => (
                                    <li key={`${note}-${idx}`}>{note}</li>
                                ))}
                            </ul>
                        </section>
                    )}

                    <section className="reconstruction-cards">
                        <h4>Memory Cards</h4>
                        {!hasCards && <p className="empty-cards">No matching memory cards.</p>}

                        {data.cards.map((card) => {
                            const screenshotSrc = card.screenshot_path
                                ? convertFileSrc(card.screenshot_path)
                                : null;

                            return (
                                <article className="reconstruction-card" key={card.id}>
                                    {screenshotSrc && (
                                        <img
                                            src={screenshotSrc}
                                            alt={card.window_title}
                                            className="reconstruction-shot"
                                            loading="lazy"
                                        />
                                    )}
                                    <div className="reconstruction-card-meta">
                                        <div className="meta-top">
                                            <span className="app">{card.app_name}</span>
                                            <span className="score">{Math.round(card.score * 100)}%</span>
                                        </div>
                                        <div className="window">{card.window_title}</div>
                                        <p className="snippet">{card.snippet}</p>
                                        {card.url && (
                                            <a href={card.url} target="_blank" rel="noreferrer">
                                                {card.url}
                                            </a>
                                        )}
                                        {card.related_tasks.length > 0 && (
                                            <p className="tasks">
                                                Linked tasks: {card.related_tasks.join(", ")}
                                            </p>
                                        )}
                                    </div>
                                </article>
                            );
                        })}
                    </section>
                </div>
            )}
        </aside>
    );
}
