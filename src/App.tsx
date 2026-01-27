import { useState, useEffect } from "react";
import { SearchBar } from "./components/SearchBar";
import { Timeline } from "./components/Timeline";
import { ControlPanel } from "./components/ControlPanel";
import { useSearch } from "./hooks/useSearch";
import { getStatus, CaptureStatus } from "./api/tauri";
import "./styles/App.css";

import { AskFndr } from "./components/AskFndr";

function App() {
    const [query, setQuery] = useState("");
    const [timeFilter, setTimeFilter] = useState<string | null>(null);
    const appFilter = null; // Filter not implemented in UI yet
    const [status, setStatus] = useState<CaptureStatus | null>(null);

    const { results, isLoading, error } = useSearch(query, timeFilter, appFilter);

    // Poll status every 2 seconds
    useEffect(() => {
        const fetchStatus = async () => {
            try {
                const s = await getStatus();
                setStatus(s);
            } catch (e) {
                console.error("Failed to get status:", e);
            }
        };

        fetchStatus();
        const interval = setInterval(fetchStatus, 2000);
        return () => clearInterval(interval);
    }, []);

    return (
        <div className="app">
            <header className="app-header">
                <div className="logo">
                    <span className="logo-icon">🔍</span>
                    <h1>FNDR</h1>
                </div>
                <div className="status-badge">
                    {status?.is_capturing ? (
                        <span className="status-active">● Capturing</span>
                    ) : status?.is_paused ? (
                        <span className="status-paused">⏸ Paused</span>
                    ) : (
                        <span className="status-idle">○ Idle</span>
                    )}
                </div>
            </header>

            <main className="app-main">
                <AskFndr />

                <SearchBar
                    value={query}
                    onChange={setQuery}
                    timeFilter={timeFilter}
                    onTimeFilterChange={setTimeFilter}
                />

                {error && <div className="error-banner">{error}</div>}

                {results.length > 0 && query && (
                    <div className="search-stats">
                        <span className="stats-icon">📊</span>
                        <div className="stats-text">
                            Detected <strong>{results.length}</strong> related moments
                            {results.length > 1 && ` over a period of ${Math.round((results[0].timestamp - results[results.length - 1].timestamp) / 60000)} minutes`}
                        </div>
                    </div>
                )}

                <Timeline results={results} isLoading={isLoading} query={query} />
            </main>

            <ControlPanel status={status} />
        </div>
    );
}

export default App;
