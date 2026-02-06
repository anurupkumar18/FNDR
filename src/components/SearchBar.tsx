import { useState, useEffect } from "react";
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
}: SearchBarProps) {
    const [summary, setSummary] = useState<string | null>(null);
    const [isSummarizing, setIsSummarizing] = useState(false);

    // Generate summary when search results change
    useEffect(() => {
        if (!value.trim() || resultCount === 0) {
            setSummary(null);
            return;
        }

        const generateSummary = async () => {
            setIsSummarizing(true);
            // Debounce and simulate summary generation
            await new Promise(r => setTimeout(r, 800));

            const timeContext = timeFilter === "1h" ? "in the last hour" :
                timeFilter === "24h" ? "today" :
                    timeFilter === "7d" ? "this week" : "across your memories";

            const appContext = appFilter ? ` while using ${appFilter}` : "";

            setSummary(
                `Found ${resultCount} memories ${timeContext}${appContext} related to "${value}". ` +
                `These span multiple sessions where you were working on similar topics.`
            );
            setIsSummarizing(false);
        };

        const timer = setTimeout(generateSummary, 400);
        return () => clearTimeout(timer);
    }, [value, resultCount, timeFilter, appFilter]);

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
                </div>
            </div>
        </div>
    );
}
