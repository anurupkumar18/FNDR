import { SearchResult } from "../api/tauri";
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
    disabled?: boolean;
    disabledHint?: string;
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
    disabled = false,
    disabledHint,
}: SearchBarProps) {
    const hasQuery = value.trim().length > 0;

    return (
        <div className="search-panel">
            {disabled && disabledHint && (
                <p className="search-disabled-hint" role="status">
                    {disabledHint}
                </p>
            )}
            <div className="search-bar" role="search">
                <div className="search-input-group">
                    <svg className="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <circle cx="11" cy="11" r="8" />
                        <path d="M21 21l-4.35-4.35" />
                    </svg>

                    <input
                        id="fndr-search-input"
                        type="text"
                        value={value}
                        onChange={(e) => onChange(e.target.value)}
                        placeholder="What do you remember?"
                        className="search-input"
                        autoComplete="off"
                        disabled={disabled}
                        aria-disabled={disabled}
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
            </div>

            {hasQuery && (
                <div className="search-meta-row">
                    <div className="search-filters">
                        <div className="select-wrapper">
                            <select
                                value={timeFilter || ""}
                                onChange={(e) => onTimeFilterChange(e.target.value || null)}
                                className={`filter-select ${timeFilter ? "active" : ""}`}
                            >
                                <option value="">Any time</option>
                                <option value="1h">Last hour</option>
                                <option value="24h">Last 24 hours</option>
                                <option value="7d">Last 7 days</option>
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
                                <option value="">All apps</option>
                                {appNames.map((name) => (
                                    <option key={name} value={name}>{name}</option>
                                ))}
                            </select>
                            <svg className="select-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                                <path d="M6 9l6 6 6-6" />
                            </svg>
                        </div>
                    </div>

                    <div className="result-count">
                        {value.trim() ? `${resultCount} results` : `${searchResults.length} memories indexed`}
                    </div>
                </div>
            )}
        </div>
    );
}
