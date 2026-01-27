import "./SearchBar.css";

interface SearchBarProps {
    value: string;
    onChange: (value: string) => void;
    timeFilter: string | null;
    onTimeFilterChange: (filter: string | null) => void;
    appFilter: string | null;
    onAppFilterChange: (app: string | null) => void;
    appNames: string[];
}

export function SearchBar({
    value,
    onChange,
    timeFilter,
    onTimeFilterChange,
    appFilter,
    onAppFilterChange,
    appNames,
}: SearchBarProps) {
    const timeOptions = [
        { id: null, label: "All Time" },
        { id: "today", label: "Today" },
        { id: "yesterday", label: "Yesterday" },
        { id: "week", label: "Past Week" },
    ];

    return (
        <div className="search-container">
            <div className="search-input-wrapper">
                <span className="search-icon">🔍</span>
                <input
                    type="text"
                    className="search-input"
                    placeholder="Search your memories (e.g. 'design meeting', 'invoice')..."
                    value={value}
                    onChange={(e) => onChange(e.target.value)}
                    autoFocus
                />
            </div>

            <div className="filter-bar">
                {timeOptions.map((opt) => (
                    <button
                        key={opt.id ?? "all"}
                        className={`filter-btn ${timeFilter === opt.id ? "active" : ""}`}
                        onClick={() => onTimeFilterChange(opt.id)}
                    >
                        {opt.label}
                    </button>
                ))}
                <select
                    className="filter-app-select"
                    value={appFilter ?? ""}
                    onChange={(e) => onAppFilterChange(e.target.value || null)}
                    title="Filter by app"
                >
                    <option value="">All apps</option>
                    {appNames.map((name) => (
                        <option key={name} value={name}>
                            {name}
                        </option>
                    ))}
                </select>
            </div>
        </div>
    );
}
