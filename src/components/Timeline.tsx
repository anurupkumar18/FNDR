import { useState, useEffect } from "react";
import { SearchResult, summarizeMemory } from "../api/tauri";
import { MemoryCard } from "./MemoryCard";
import "./Timeline.css";

const INITIAL_VISIBLE = 30;
const LOAD_MORE_STEP = 30;

interface TimelineProps {
    results: SearchResult[];
    isLoading: boolean;
    query: string;
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

export function Timeline({ results, isLoading, query }: TimelineProps) {
    const [visibleCount, setVisibleCount] = useState(INITIAL_VISIBLE);
    const [selectedMemory, setSelectedMemory] = useState<SearchResult | null>(null);
    const [memorySummary, setMemorySummary] = useState<string | null>(null);
    const [isSummarizing, setIsSummarizing] = useState(false);

    useEffect(() => {
        setVisibleCount(INITIAL_VISIBLE);
    }, [query]);

    // Close modal on escape
    useEffect(() => {
        const handleEscape = (e: KeyboardEvent) => {
            if (e.key === "Escape") {
                setSelectedMemory(null);
                setMemorySummary(null);
            }
        };
        window.addEventListener("keydown", handleEscape);
        return () => window.removeEventListener("keydown", handleEscape);
    }, []);

    // Generate LLM summary when memory is selected
    useEffect(() => {
        if (!selectedMemory) {
            setMemorySummary(null);
            return;
        }

        const generateSummary = async () => {
            setIsSummarizing(true);
            setMemorySummary(null);
            try {
                const summary = await summarizeMemory(
                    selectedMemory.app_name,
                    selectedMemory.window_title,
                    selectedMemory.text
                );
                setMemorySummary(summary);
            } catch (e) {
                console.error("Failed to summarize:", e);
                setMemorySummary("Unable to generate summary.");
            } finally {
                setIsSummarizing(false);
            }
        };

        generateSummary();
    }, [selectedMemory]);

    const handleCloseModal = () => {
        setSelectedMemory(null);
        setMemorySummary(null);
    };

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

    // Group by day
    const groups: Record<string, SearchResult[]> = {};
    visibleResults.forEach((r) => {
        const day = formatDay(r.timestamp);
        if (!groups[day]) groups[day] = [];
        groups[day].push(r);
    });

    return (
        <>
            <div className="timeline-container">
                {Object.entries(groups).map(([day, dayResults]) => {
                    const filtered = filterConsecutiveSimilar(dayResults);

                    return (
                        <section key={day} className="timeline-section">
                            <header className="section-header">
                                <h2>{day}</h2>
                                <span className="count-badge">{filtered.length} memories</span>
                            </header>

                            <div className="bento-grid">
                                {filtered.map((result, index) => (
                                    <MemoryCard
                                        key={result.id}
                                        result={result}
                                        query={query}
                                        onClick={() => setSelectedMemory(result)}
                                        isLarge={index % 5 === 0}
                                    />
                                ))}
                            </div>
                        </section>
                    );
                })}

                {hasMore && (
                    <div className="load-more-container">
                        <button
                            onClick={() => setVisibleCount(n => n + LOAD_MORE_STEP)}
                            className="load-more-btn"
                        >
                            Load {Math.min(LOAD_MORE_STEP, results.length - visibleCount)} more
                        </button>
                    </div>
                )}
            </div>

            {/* Memory Detail Overlay with LLM Summary */}
            {selectedMemory && (
                <div className="modal-backdrop" onClick={handleCloseModal}>
                    <div className="modal-content" onClick={(e) => e.stopPropagation()}>
                        <button className="modal-close" onClick={handleCloseModal} aria-label="Close">
                            ✕
                        </button>

                        <div className="modal-header">
                            <div className="modal-app-info">
                                <span className="modal-app-icon">📱</span>
                                <div>
                                    <h2 className="modal-app-name">{selectedMemory.app_name}</h2>
                                    <p className="modal-window-title">{selectedMemory.window_title}</p>
                                </div>
                            </div>
                            <time className="modal-time">
                                {new Date(selectedMemory.timestamp).toLocaleString(undefined, {
                                    weekday: 'short',
                                    month: 'short',
                                    day: 'numeric',
                                    hour: '2-digit',
                                    minute: '2-digit'
                                })}
                            </time>
                        </div>

                        <div className="modal-body">
                            {/* LLM Summary Section */}
                            <div className="summary-section">
                                <h3>
                                    <span className="summary-icon">✨</span>
                                    AI Summary
                                </h3>
                                {isSummarizing ? (
                                    <div className="summary-loading">
                                        <div className="summary-spinner" />
                                        <span>Analyzing memory...</span>
                                    </div>
                                ) : (
                                    <div className="summary-content">
                                        {memorySummary?.split('\n').map((line, i) => (
                                            <p key={i} className={
                                                line.startsWith('ACTIVITY:') ? 'summary-activity' :
                                                    line.startsWith('DETAILS:') ? 'summary-details' : ''
                                            }>
                                                {line}
                                            </p>
                                        ))}
                                    </div>
                                )}
                            </div>

                            {/* Raw Content (Collapsed by default) */}
                            <details className="raw-content-section">
                                <summary>View Raw Content</summary>
                                <div className="modal-text-content">
                                    {selectedMemory.text}
                                </div>
                            </details>
                        </div>

                        <div className="modal-footer">
                            <div className="modal-score">
                                Match: {Math.round(selectedMemory.score * 100)}%
                            </div>
                            <button
                                className="modal-copy-btn"
                                onClick={() => {
                                    navigator.clipboard.writeText(
                                        memorySummary || selectedMemory.text
                                    );
                                }}
                            >
                                Copy {memorySummary ? "Summary" : "Text"}
                            </button>
                        </div>
                    </div>
                </div>
            )}
        </>
    );
}

function filterConsecutiveSimilar(results: SearchResult[]): SearchResult[] {
    if (results.length <= 1) return results;

    const filtered: SearchResult[] = [results[0]];
    for (let i = 1; i < results.length; i++) {
        const prev = filtered[filtered.length - 1];
        const curr = results[i];

        // Skip if same app and < 30s diff
        if (curr.app_name === prev.app_name && Math.abs(curr.timestamp - prev.timestamp) < 30000) {
            continue;
        }
        filtered.push(curr);
    }
    return filtered;
}
