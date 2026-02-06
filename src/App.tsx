import { useState, useEffect } from "react";
import { SearchBar } from "./components/SearchBar";
import { Timeline } from "./components/Timeline";
import { ControlPanel } from "./components/ControlPanel";
import { TodoModal } from "./components/TodoModal";
import { useSearch } from "./hooks/useSearch";
import { getStatus, getAppNames, CaptureStatus, Task } from "./api/tauri";
import "./styles/App.css";

function App() {
    const [query, setQuery] = useState("");
    const [timeFilter, setTimeFilter] = useState<string | null>(null);
    const [appFilter, setAppFilter] = useState<string | null>(null);
    const [appNames, setAppNames] = useState<string[]>([]);
    const [status, setStatus] = useState<CaptureStatus | null>(null);
    const [_executingTask, setExecutingTask] = useState<Task | null>(null);

    const { results, isLoading, error } = useSearch(query, timeFilter, appFilter);

    // Show todo modal when no search and no results
    const showTodoModal = !query && results.length === 0 && !isLoading;

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

    const handleExecuteTask = (task: Task) => {
        setExecutingTask(task);
        // TODO: Launch CUA agent with task
        console.log("Executing task with CUA:", task);
        alert(`🤖 CUA Agent would now execute:\n\n"${task.title}"\n\nThis feature requires CUA setup.`);
    };

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

                {/* Show Todo Modal on home page (no search) */}
                {showTodoModal && (
                    <TodoModal
                        isVisible={true}
                        onExecuteTask={handleExecuteTask}
                    />
                )}

                {/* Show Timeline when searching or has results */}
                {!showTodoModal && (
                    <Timeline results={results} isLoading={isLoading} query={query} />
                )}
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
