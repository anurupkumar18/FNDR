import { useState, useEffect } from "react";
import { summarizeSearch, SearchResult } from "../api/tauri";
import "./SearchBar.css";

interface SearchBarProps {
    value: string;
    onChange: (value: string) => void;
    timeFilter: string | null;
    onTimeFilterChange: (filter: string | null) => void;
    appFilter: string | null;
    onAppFilterChange: (filter: string | null) => void;
    appNames: string[];
    resultCount: number;
    searchResults: SearchResult[];
}

export function SearchBar({
    value,
    onChange,
    timeFilter,
    onTimeFilterChange,
    appFilter,
    onAppFilterChange,
    appNames,
    resultCount,
    searchResults,
}: SearchBarProps) {
    const [summary, setSummary] = useState<string | null>(null);
    const [isSummarizing, setIsSummarizing] = useState(false);

    // Generate summary when search results change
    useEffect(() => {
        if (!value.trim() || resultCount === 0 || searchResults.length === 0) {
            setSummary(null);
            setIsSummarizing(false);
            return;
        }

        // Show loading state immediately while debouncing
        setIsSummarizing(true);
        setSummary(null);

        const timer = setTimeout(async () => {
            try {
                // Extract snippets from top 5 results
                const snippets = searchResults
                    .slice(0, 5)
                    .map(r => `[${r.app_name}] ${r.snippet}`);

                const aiSummary = await summarizeSearch(value, snippets);
                setSummary(aiSummary || "Found relevant memories.");
            } catch (err) {
                console.error("Summary generation failed:", err);
                setSummary(`Found ${resultCount} relevant memories.`);
            } finally {
                setIsSummarizing(false);
            }
        }, 600);

        return () => clearTimeout(timer);
    }, [value, resultCount]); // Only depend on key triggers

    return (
        <div className="search-overlay">
            {/* Summary Bubble */}
            {value.trim() && resultCount > 0 && (
                <div className="summary-bubble">
                    {isSummarizing ? (
                        <div className="summary-loading">
                            <span className="summary-spinner" />
                            <span>Synthesizing memories...</span>
                        </div>
                    ) : (
                        <p className="summary-text">
                            <span className="summary-icon">💡</span>
                            {summary}
                        </p>
                    )}
                </div>
            )}

            {/* Search Bar */}
            <div className="search-bar">
                <div className="search-input-group">
                    <svg className="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <circle cx="11" cy="11" r="8" />
                        <path d="M21 21l-4.35-4.35" />
                    </svg>

                    <input
                        type="text"
                        value={value}
                        onChange={(e) => onChange(e.target.value)}
                        placeholder="Search your memories..."
                        className="search-input"
                        autoComplete="off"
                    />

                    {value && (
                        <button
                            className="search-clear"
                            onClick={() => onChange("")}
                            aria-label="Clear search"
                        >
                            ✕
                        </button>
                    )}
                </div>

                <div className="search-filters">
                    <div className="select-wrapper">
                        <select
                            value={timeFilter || ""}
                            onChange={(e) => onTimeFilterChange(e.target.value || null)}
                            className={`filter-select ${timeFilter ? "active" : ""}`}
                        >
                            <option value="">⏱ Any Time</option>
                            <option value="1h">Last Hour</option>
                            <option value="24h">Last 24 Hours</option>
                            <option value="7d">Last 7 Days</option>
                        </select>
                        <svg className="select-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                            <path d="M6 9l6 6 6-6" />
                        </svg>
                    </div>

                    <div className="select-wrapper">
                        <select
                            value={appFilter || ""}
                            onChange={(e) => onAppFilterChange(e.target.value || null)}
                            className={`filter-select ${appFilter ? "active" : ""}`}
                        >
                            <option value="">📱 All Apps</option>
                            {appNames.map((name) => (
                                <option key={name} value={name}>{name}</option>
                            ))}
                        </select>
                        <svg className="select-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                            <path d="M6 9l6 6 6-6" />
                        </svg>
                    </div>
                </div>
            </div>
        </div>
    );
}
