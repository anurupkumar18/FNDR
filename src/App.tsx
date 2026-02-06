import { useState, useEffect } from "react";
import { SearchBar } from "./components/SearchBar";
import { Timeline } from "./components/Timeline";
import { ControlPanel } from "./components/ControlPanel";
import { useSearch } from "./hooks/useSearch";
import { getStatus, getAppNames, CaptureStatus } from "./api/tauri";
import "./styles/App.css";

function App() {
    const [query, setQuery] = useState("");
    const [timeFilter, setTimeFilter] = useState<string | null>(null);
    const [appFilter, setAppFilter] = useState<string | null>(null);
    const [appNames, setAppNames] = useState<string[]>([]);
    const [status, setStatus] = useState<CaptureStatus | null>(null);

    const { results, isLoading, error } = useSearch(query, timeFilter, appFilter);

    // Load app names for filter
    useEffect(() => {
        getAppNames().then(setAppNames).catch(() => setAppNames([]));
    }, [status?.frames_captured]);

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
                    <span className="logo-icon">⌘</span>
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
                {error && (
                    <div className="error-banner">
                        {error}
                    </div>
                )}

                <Timeline results={results} isLoading={isLoading} query={query} />
            </main>

            {/* Bottom Overlay Search Bar */}
            <SearchBar
                value={query}
                onChange={setQuery}
                timeFilter={timeFilter}
                onTimeFilterChange={setTimeFilter}
                appFilter={appFilter}
                onAppFilterChange={setAppFilter}
                appNames={appNames}
                resultCount={results.length}
            />

            <ControlPanel status={status} />
        </div>
    );
}

export default App;
