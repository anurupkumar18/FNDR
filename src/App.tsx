import { useEffect, useState } from "react";
import { SearchBar } from "./components/SearchBar";
import { Timeline } from "./components/Timeline";
import { ControlPanel } from "./components/ControlPanel";
import { TodoModal } from "./components/TodoModal";
import { AgentPanel } from "./components/AgentPanel";
import { GraphPanel } from "./components/GraphPanel";
import { MeetingRecorderPanel } from "./components/MeetingRecorderPanel";
import { ModelDownloadBanner } from "./components/ModelDownloadBanner";
import { Onboarding } from "./components/Onboarding";

import { useSearch } from "./hooks/useSearch";
import {
    CaptureStatus,
    SearchResult,
    SystemReadiness,
    Task,
    getAppNames,
    getReadiness,
    getStatus,
    startAgentTask,
} from "./api/tauri";
import { getOnboardingState } from "./api/onboarding";
import { EVAL_UI } from "./evalUi";
import "./styles/App.css";

function App() {
    const [query, setQuery] = useState("");
    const [timeFilter, setTimeFilter] = useState<string | null>(null);
    const [appFilter, setAppFilter] = useState<string | null>(null);
    const [appNames, setAppNames] = useState<string[]>([]);
    const [status, setStatus] = useState<CaptureStatus | null>(null);
    const [readiness, setReadiness] = useState<SystemReadiness | null>(null);
    const [showAgentPanel, setShowAgentPanel] = useState(false);
    const [showGraphPanel, setShowGraphPanel] = useState(false);
    const [showMeetingPanel, setShowMeetingPanel] = useState(false);
    const [onboardingDone, setOnboardingDone] = useState<boolean | null>(null);
    const [selectedResult, setSelectedResult] = useState<SearchResult | null>(null);
    const [isSidebarOpen, setIsSidebarOpen] = useState(false);

    const searchAllowed = Boolean(readiness?.ready_for_search);
    const { results, isLoading, error } = useSearch(
        searchAllowed ? query : "",
        timeFilter,
        appFilter
    );

    useEffect(() => {
        getOnboardingState()
            .then((s) => setOnboardingDone(s.step === "complete" && s.model_downloaded))
            .catch(() => setOnboardingDone(false));
    }, []);

    const showTodoModal = !EVAL_UI && !query.trim() && results.length === 0 && !isLoading;
    const showCenteredSearch = !query.trim();
    const isFocusMode = !query.trim();

    useEffect(() => {
        const loadAppNames = async () => {
            try {
                setAppNames(await getAppNames());
            } catch {
                setAppNames([]);
            }
        };

        void loadAppNames();
        const id = window.setInterval(() => {
            void loadAppNames();
        }, 30_000);

        return () => window.clearInterval(id);
    }, []);

    useEffect(() => {
        const fetchStatus = async () => {
            try {
                const nextStatus = await getStatus();
                setStatus(nextStatus);
            } catch (e) {
                console.error("Failed to get status:", e);
            }
        };

        void fetchStatus();
        const interval = window.setInterval(() => {
            void fetchStatus();
        }, 2000);

        return () => window.clearInterval(interval);
    }, []);

    useEffect(() => {
        const loadReadiness = async () => {
            try {
                const nextReadiness = await getReadiness();
                setReadiness(nextReadiness);
            } catch (e) {
                console.error("Failed to get readiness:", e);
            }
        };

        void loadReadiness();
        const interval = window.setInterval(() => {
            void loadReadiness();
        }, 5000);

        return () => window.clearInterval(interval);
    }, []);

    useEffect(() => {
        if (!results.length) {
            setSelectedResult(null);
            return;
        }

        setSelectedResult((previous) => {
            if (!previous) {
                return results[0];
            }

            const stillVisible = results.find((item) => item.id === previous.id);
            return stillVisible ?? results[0];
        });
    }, [results]);

    const handleExecuteTask = async (task: Task) => {
        try {
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

    if (onboardingDone === null) {
        return null;
    }

    if (!onboardingDone) {
        return <Onboarding onComplete={() => setOnboardingDone(true)} />;
    }

    return (
        <div className="app">
            {!EVAL_UI && (
                <button
                    className="ui-action-btn sidebar-toggle"
                    onClick={() => setIsSidebarOpen((prev) => !prev)}
                    aria-label={isSidebarOpen ? "Close sidebar" : "Open sidebar"}
                >
                    {isSidebarOpen ? "×" : "☰"}
                </button>
            )}

            <div className="top-right-control">
                <ControlPanel status={status} compact={true} evalUi={EVAL_UI} />
            </div>

            {status && !status.ai_model_available && <ModelDownloadBanner />}

            {!EVAL_UI && isSidebarOpen && (
                <button
                    className="sidebar-scrim"
                    onClick={() => setIsSidebarOpen(false)}
                    aria-label="Close sidebar overlay"
                />
            )}

            {!EVAL_UI && (
                <aside className={`left-sidebar ${isSidebarOpen ? "open" : ""}`}>
                    <div className="sidebar-group sidebar-actions">
                        <div className="sidebar-label">Experimental</div>
                        <button
                            className={`ui-action-btn meeting-toggle-btn ${showMeetingPanel ? "active" : ""}`}
                            onClick={() => setShowMeetingPanel((open) => !open)}
                        >
                            Meetings
                        </button>
                        <button
                            className={`ui-action-btn graph-toggle-btn ${showGraphPanel ? "active" : ""}`}
                            onClick={() => setShowGraphPanel((open) => !open)}
                        >
                            Graph
                        </button>
                    </div>
                </aside>
            )}

            <main className={`app-main ${showCenteredSearch ? "search-centered" : ""}`}>
                <section className={`search-shell ${query.trim() ? "is-active" : ""}`}>
                    <SearchBar
                        value={query}
                        onChange={setQuery}
                        timeFilter={timeFilter}
                        onTimeFilterChange={setTimeFilter}
                        appFilter={appFilter}
                        onAppFilterChange={setAppFilter}
                        onSetMeetingPanelOpen={setShowMeetingPanel}
                        onSetGraphPanelOpen={setShowGraphPanel}
                        appNames={appNames}
                        resultCount={results.length}
                        searchResults={results}
                        disabled={!searchAllowed}
                        disabledHint={
                            readiness && !readiness.ready_for_search
                                ? "Waiting for search backend..."
                                : undefined
                        }
                    />
                </section>

                {!isFocusMode && (
                    <div className="main-layout">
                        <section className="main-column">
                            {error && <div className="error-banner">{error}</div>}

                            {showTodoModal && (
                                <TodoModal
                                    isVisible={true}
                                    onExecuteTask={handleExecuteTask}
                                />
                            )}

                            {!showTodoModal && (
                                <Timeline
                                    results={results}
                                    isLoading={isLoading}
                                    query={query}
                                    selectedResultId={selectedResult?.id ?? null}
                                    onSelectResult={setSelectedResult}
                                    evalUi={EVAL_UI}
                                />
                            )}
                        </section>
                    </div>
                )}
            </main>

            {!EVAL_UI && (
                <>
                    <AgentPanel
                        isVisible={showAgentPanel}
                        onClose={() => setShowAgentPanel(false)}
                    />
                    <GraphPanel
                        isVisible={showGraphPanel}
                        onClose={() => setShowGraphPanel(false)}
                    />
                    <MeetingRecorderPanel
                        isVisible={showMeetingPanel}
                        onClose={() => setShowMeetingPanel(false)}
                        onOpenAgent={() => setShowAgentPanel(true)}
                    />
                </>
            )}
        </div>
    );
}

export default App;
