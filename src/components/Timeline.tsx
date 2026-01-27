import { useState, useEffect } from "react";
import { SearchResult } from "../api/tauri";
import { MemoryCard } from "./MemoryCard";
import "./Timeline.css";

const INITIAL_VISIBLE = 80;
const LOAD_MORE_STEP = 80;

interface TimelineProps {
    results: SearchResult[];
    isLoading: boolean;
    query: string;
}

function formatDay(timestamp: number): string {
    return new Date(timestamp).toLocaleDateString(undefined, {
        weekday: "long",
        year: "numeric",
        month: "long",
        day: "numeric",
    });
}

export function Timeline({ results, isLoading, query }: TimelineProps) {
    const [visibleCount, setVisibleCount] = useState(INITIAL_VISIBLE);

    // Reset visible count when search query changes
    useEffect(() => {
        setVisibleCount(INITIAL_VISIBLE);
    }, [query]);

    if (isLoading) {
        return (
            <div className="timeline-loading">
                <div className="spinner"></div>
                <p>Searching for memories...</p>
            </div>
        );
    }

    if (results.length === 0) {
        if (!query.trim()) {
            return (
                <div className="timeline-empty">
                    <span className="empty-icon">⌨️</span>
                    <h3>Start typing to search</h3>
                    <p>FNDR captures your screen every 2 seconds</p>
                </div>
            );
        }
        return (
            <div className="timeline-empty">
                <span className="empty-icon">📂</span>
                <h3>No memories found</h3>
                <p>Try a different search query or filter</p>
            </div>
        );
    }

    const visibleResults = results.slice(0, visibleCount);
    const hasMore = results.length > visibleCount;
    const remaining = results.length - visibleCount;

    // Group visible results by day
    const groups: Record<string, SearchResult[]> = {};
    visibleResults.forEach((r) => {
        const day = formatDay(r.timestamp);
        if (!groups[day]) groups[day] = [];
        groups[day].push(r);
    });

    const handleLoadMore = () => {
        setVisibleCount((n) => Math.min(n + LOAD_MORE_STEP, results.length));
    };

    return (
        <div className="timeline">
            {Object.entries(groups).map(([day, dayResults]) => {
                const filtered = filterConsecutiveSimilar(dayResults);
                const durationMinutes = Math.round(
                    (dayResults[0].timestamp - dayResults[dayResults.length - 1].timestamp) /
                        60000
                );

                return (
                    <div key={day} className="timeline-day">
                        <div className="day-header-group">
                            <h2 className="timeline-day-header">{day}</h2>
                            {query && (
                                <span className="day-summary">
                                    {dayResults.length} matches over {durationMinutes || 1} minutes
                                </span>
                            )}
                        </div>
                        <div className="timeline-cards">
                            {filtered.map((result) => (
                                <MemoryCard key={result.id} result={result} query={query} />
                            ))}
                        </div>
                    </div>
                );
            })}
            {hasMore && (
                <div className="timeline-load-more">
                    <button type="button" className="load-more-btn" onClick={handleLoadMore}>
                        Load more — show {Math.min(LOAD_MORE_STEP, remaining)} of {remaining} remaining
                    </button>
                </div>
            )}
        </div>
    );
}

// Simple helper to filter out consecutive records that are nearly identical
function filterConsecutiveSimilar(results: SearchResult[]): SearchResult[] {
    if (results.length <= 1) return results;

    const filtered: SearchResult[] = [results[0]];
    for (let i = 1; i < results.length; i++) {
        const prev = filtered[filtered.length - 1];
        const curr = results[i];

        const timeDiff = Math.abs(curr.timestamp - prev.timestamp);
        const isSameApp = curr.app_name === prev.app_name;

        // If same app and very close time, check similarity
        if (isSameApp && timeDiff < 15000) {
            // Very simple text similarity check
            const text1 = prev.text.toLowerCase().substring(0, 100);
            const text2 = curr.text.toLowerCase().substring(0, 100);
            if (text1 === text2) continue;
        }

        filtered.push(curr);
    }
    return filtered;
}
