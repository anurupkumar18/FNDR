import { SearchResult } from "../api/tauri";
import { MemoryCard } from "./MemoryCard";
import "./Timeline.css";

interface TimelineProps {
    results: SearchResult[];
    isLoading: boolean;
    query: string;
}

export function Timeline({ results, isLoading, query }: TimelineProps) {
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

    // Group results by day
    const groups: Record<string, SearchResult[]> = {};
    results.forEach((r) => {
        const date = new Date(r.timestamp);
        const day = date.toLocaleDateString(undefined, {
            weekday: "long",
            year: "numeric",
            month: "long",
            day: "numeric",
        });
        if (!groups[day]) groups[day] = [];
        groups[day].push(r);
    });

    return (
        <div className="timeline">
            {Object.entries(groups).map(([day, dayResults]) => {
                const filtered = filterConsecutiveSimilar(dayResults);
                const durationMinutes = Math.round((dayResults[0].timestamp - dayResults[dayResults.length - 1].timestamp) / 60000);

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
