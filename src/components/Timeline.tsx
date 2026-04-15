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
    onDeleteMemory?: (memoryId: string) => void;
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

function normalizePreview(value: string): string {
    return value
        .trim()
        .toLowerCase()
        .replace(/\s+/g, " ");
}

function isLowSignalPreview(summary: string, appName: string): boolean {
    const normalized = normalizePreview(summary);
    if (!normalized) {
        return true;
    }
    const app = normalizePreview(appName);
    if (normalized === app || normalized === "fndr" || normalized === "codex") {
        return true;
    }
    return normalized.split(" ").length <= 2;
}

function stripLegacySources(summary: string): string {
    return summary.replace(/\s*Sources:\s*[A-Za-z0-9,\-\s]+\.?$/i, "").trim();
}

export function Timeline({
    results,
    isLoading,
    query,
    selectedResultId,
    onSelectResult,
    onDeleteMemory,
    evalUi = false,
}: TimelineProps) {
    const [visibleCount, setVisibleCount] = useState(INITIAL_VISIBLE);

    useEffect(() => {
        setVisibleCount(INITIAL_VISIBLE);
    }, [query]);

    if (isLoading) {
        return (
            <div className="timeline-state">
                <div className="thinking-loader thinking-loader-lg" aria-hidden="true" />
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
                {filteredResults.map((result) => {
                    const cleanSummary = stripLegacySources(result.summary);
                    return (
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
                                <div className="result-meta-main">
                                    <span className="result-app">{result.app_name}</span>
                                    <span className="result-time">
                                        {formatDay(result.timestamp)} ·{" "}
                                        {new Date(result.timestamp).toLocaleTimeString(undefined, {
                                            hour: "2-digit",
                                            minute: "2-digit",
                                        })}
                                    </span>
                                </div>
                                <div className="result-meta-actions">
                                    {evalUi && (
                                        <span className="result-score" title="Relevance score">
                                            score {result.score.toFixed(3)}
                                        </span>
                                    )}
                                    {onDeleteMemory && (
                                        <button
                                            className="ui-action-btn timeline-delete-btn"
                                            onClick={(event) => {
                                                event.stopPropagation();
                                                onDeleteMemory(result.id);
                                            }}
                                            aria-label="Delete this memory"
                                            title="Delete this memory"
                                        >
                                            Delete
                                        </button>
                                    )}
                                </div>
                            </div>
                            <h3 className="result-title">{result.title || "Untitled memory"}</h3>
                            {!isLowSignalPreview(cleanSummary, result.app_name) && (
                                <p className="result-preview">{cleanSummary}</p>
                            )}

                            {result.context.some((item) => !/^sources\s*:/i.test(item)) && (
                                <div className="result-context-chips">
                                    {result.context
                                        .filter((item) => !/^sources\s*:/i.test(item))
                                        .slice(0, 4)
                                        .map((item, idx) => (
                                            <span key={`${result.id}-ctx-${idx}`} className="result-chip">
                                                {item}
                                            </span>
                                        ))}
                                </div>
                            )}
                        </article>
                    );
                })}
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
