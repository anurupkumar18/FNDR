import { useState, useEffect } from "react";
import { SearchBar } from "./components/SearchBar";
import { Timeline } from "./components/Timeline";
import { ControlPanel } from "./components/ControlPanel";
import { TodoModal } from "./components/TodoModal";
import { AgentPanel } from "./components/AgentPanel";
import { MemoryReconstructionPanel } from "./components/MemoryReconstructionPanel";
import { GraphPanel } from "./components/GraphPanel";
import { MeetingRecorderPanel } from "./components/MeetingRecorderPanel";
import { Onboarding } from "./components/Onboarding";
import { useSearch } from "./hooks/useSearch";
import { getStatus, getAppNames, CaptureStatus, Task, startAgentTask } from "./api/tauri";
import { getOnboardingState } from "./api/onboarding";
import "./styles/App.css";

function App() {
    const [query, setQuery] = useState("");
    const [timeFilter, setTimeFilter] = useState<string | null>(null);
    const [appFilter, setAppFilter] = useState<string | null>(null);
    const [appNames, setAppNames] = useState<string[]>([]);
    const [status, setStatus] = useState<CaptureStatus | null>(null);
    const [showAgentPanel, setShowAgentPanel] = useState(false);
    const [showGraphPanel, setShowGraphPanel] = useState(false);
    const [showMeetingPanel, setShowMeetingPanel] = useState(false);
    const [onboardingDone, setOnboardingDone] = useState<boolean | null>(null);

    const { results, isLoading, error } = useSearch(query, timeFilter, appFilter);

    // Check if onboarding is complete on first mount
    useEffect(() => {
        getOnboardingState()
            .then((s) => setOnboardingDone(s.step === "complete"))
            .catch(() => setOnboardingDone(true)); // If state can't load, skip onboarding
    }, []);


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

    const handleExecuteTask = async (task: Task) => {
        try {
            // Start the agent
            await startAgentTask(
                task.title,
                task.linked_urls,
                task.linked_memory_ids.map((id) => `linked memory: ${id}`)
            );
            setShowAgentPanel(true);
        } catch (err) {
            console.error("Failed to start agent:", err);
            alert(`Failed to start agent: ${err}`);
        }
    };

    // Show todo modal when no search and no results
    const showTodoModal = !query && results.length === 0 && !isLoading;

    // Show nothing until we know the onboarding state
    if (onboardingDone === null) return null;

    // Show onboarding if not complete
    if (!onboardingDone) {
        return <Onboarding onComplete={() => setOnboardingDone(true)} />;
    }

    return (
        <div className="app">
            <header className="app-header">
                <div className="logo">
                    <span className="logo-icon">⌘</span>
                    <h1>FNDR</h1>
                </div>
                <div className="header-actions">
                    <button
                        className={`meeting-toggle-btn ${showMeetingPanel ? "active" : ""}`}
                        onClick={() => setShowMeetingPanel(!showMeetingPanel)}
                        title="Toggle Meeting Recorder"
                    >
                        🎙️ Meetings
                    </button>
                    <button
                        className={`graph-toggle-btn ${showGraphPanel ? "active" : ""}`}
                        onClick={() => setShowGraphPanel(!showGraphPanel)}
                        title="Toggle Knowledge Graph"
                    >
                        🕸️ Graph
                    </button>
                    <div className="status-badge">
                        {status?.is_capturing ? (
                            <span className="status-active">● Capturing</span>
                        ) : status?.is_paused ? (
                            <span className="status-paused">⏸ Paused</span>
                        ) : (
                            <span className="status-idle">○ Idle</span>
                        )}
                    </div>
                </div>
            </header>

            <main className="app-main">
                <div className={`main-layout ${query.trim() ? "with-reconstruction" : ""}`}>
                    <section className="main-column">
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
                    </section>

                    <MemoryReconstructionPanel query={query} />
                </div>
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
                searchResults={results}
            />

            <ControlPanel status={status} />

            {/* Agent Panel Overlay */}
            <AgentPanel
                isVisible={showAgentPanel}
                onClose={() => setShowAgentPanel(false)}
            />

            {/* Knowledge Graph Panel */}
            <GraphPanel
                isVisible={showGraphPanel}
                onClose={() => setShowGraphPanel(false)}
            />

            <MeetingRecorderPanel
                isVisible={showMeetingPanel}
                onClose={() => setShowMeetingPanel(false)}
                onOpenAgent={() => setShowAgentPanel(true)}
            />
        </div>
    );
}

export default App;
