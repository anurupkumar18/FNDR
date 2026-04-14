import { useState, useEffect } from "react";
import { MemoryCard } from "../api/tauri";
import "./Timeline.css";

const INITIAL_VISIBLE = 30;
const LOAD_MORE_STEP = 30;

interface TimelineProps {
    results: MemoryCard[];
    isLoading: boolean;
    query: string;
    selectedResultId: string | null;
    onSelectResult: (result: MemoryCard) => void;
    evalUi?: boolean;
}

function formatDay(timestamp: number): string {
    const date = new Date(timestamp);
    const today = new Date();
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    if (date.toDateString() === today.toDateString()) {
        return "Today";
    }
    if (date.toDateString() === yesterday.toDateString()) {
        return "Yesterday";
    }
    return date.toLocaleDateString(undefined, {
        weekday: "long",
        month: "long",
        day: "numeric",
    });
}

export function Timeline({
    results,
    isLoading,
    query,
    selectedResultId,
    onSelectResult,
    evalUi = false,
}: TimelineProps) {
    const [visibleCount, setVisibleCount] = useState(INITIAL_VISIBLE);

    useEffect(() => {
        setVisibleCount(INITIAL_VISIBLE);
    }, [query]);

    if (isLoading) {
        return (
            <div className="timeline-state">
                <div className="spinner" />
                <p>Searching memories...</p>
            </div>
        );
    }

    if (results.length === 0) {
        if (!query.trim()) {
            return (
                <div className="timeline-state timeline-welcome">
                    <div className="welcome-icon">⌘</div>
                    <h2>Welcome to FNDR</h2>
                    <p>Your memories are being captured. Start typing below to search.</p>
                </div>
            );
        }
        return (
            <div className="timeline-state">
                <div className="empty-icon">🔍</div>
                <h3>No memories found</h3>
                <p>Try a different search term</p>
            </div>
        );
    }

    const visibleResults = results.slice(0, visibleCount);
    const hasMore = results.length > visibleCount;
    const filteredResults = filterConsecutiveSimilar(visibleResults);

    return (
        <div className="timeline-container">
            <div className="timeline-stream">
                {filteredResults.map((result) => (
                    <article
                        key={result.id}
                        className={`result-card ${selectedResultId === result.id ? "selected" : ""}`}
                        onClick={() => onSelectResult(result)}
                        role="button"
                        tabIndex={0}
                        onKeyDown={(event) => {
                            if (event.key === "Enter" || event.key === " ") {
                                event.preventDefault();
                                onSelectResult(result);
                            }
                        }}
                    >
                        <div className={`result-meta ${evalUi ? "result-meta-eval" : ""}`}>
                            <span className="result-app">{result.app_name}</span>
                            <span className="result-time">
                                {formatDay(result.timestamp)} ·{" "}
                                {new Date(result.timestamp).toLocaleTimeString(undefined, {
                                    hour: "2-digit",
                                    minute: "2-digit",
                                })}
                            </span>
                            {evalUi && (
                                <span className="result-score" title="Relevance score">
                                    score {result.score.toFixed(3)}
                                </span>
                            )}
                        </div>
                        <h3 className="result-title">{result.title || "Untitled memory"}</h3>
                        <p className="result-preview">{result.summary}</p>

                        {result.context.length > 0 && (
                            <div className="result-context-chips">
                                {result.context.slice(0, 4).map((item, idx) => (
                                    <span key={`${result.id}-ctx-${idx}`} className="result-chip">
                                        {item}
                                    </span>
                                ))}
                            </div>
                        )}

                    </article>
                ))}
            </div>

            {hasMore && (
                <div className="load-more-container">
                    <button
                        onClick={() => setVisibleCount((n) => n + LOAD_MORE_STEP)}
                        className="load-more-btn"
                    >
                        Load {Math.min(LOAD_MORE_STEP, results.length - visibleCount)} more
                    </button>
                </div>
            )}
        </div>
    );
}

function filterConsecutiveSimilar(results: MemoryCard[]): MemoryCard[] {
    if (results.length <= 1) return results;

    const filtered: MemoryCard[] = [results[0]];
    for (let i = 1; i < results.length; i++) {
        const prev = filtered[filtered.length - 1];
        const curr = results[i];

        // Skip if same app and < 30s diff and highly similar title.
        if (
            curr.app_name === prev.app_name &&
            Math.abs(curr.timestamp - prev.timestamp) < 30_000 &&
            curr.title.toLowerCase() === prev.title.toLowerCase()
        ) {
            continue;
        }
        filtered.push(curr);
    }
    return filtered;
}
