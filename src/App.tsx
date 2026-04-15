import { useEffect, useMemo, useState } from "react";
import { SearchBar } from "./components/SearchBar";
import { Timeline } from "./components/Timeline";
import { ControlPanel } from "./components/ControlPanel";
import { TodoPanel } from "./components/TodoPanel";
import { AgentPanel } from "./components/AgentPanel";
import { GraphPanel } from "./components/GraphPanel";
import { MeetingRecorderPanel } from "./components/MeetingRecorderPanel";
import { MemoryCardsPanel } from "./components/MemoryCardsPanel";
import { StatsPanel } from "./components/StatsPanel";
import { ModelDownloadBanner } from "./components/ModelDownloadBanner";
import { Onboarding } from "./components/Onboarding";

import { useSearch } from "./hooks/useSearch";
import {
    CaptureStatus,
    MeetingRecorderStatus,
    MemoryCard,
    deleteMemory,
    getAppNames,
    getMeetingStatus,
    getStatus,
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
    const [meetingStatus, setMeetingStatus] = useState<MeetingRecorderStatus | null>(null);
    const [consentPulseTick, setConsentPulseTick] = useState(0);
    const [showAgentPanel, setShowAgentPanel] = useState(false);
    const [showMeetingPanel, setShowMeetingPanel] = useState(false);
    const [showGraphPanel, setShowGraphPanel] = useState(false);
    const [showMemoryCardsPanel, setShowMemoryCardsPanel] = useState(false);
    const [showStatsPanel, setShowStatsPanel] = useState(false);
    const [showTodoPanel, setShowTodoPanel] = useState(false);
    const [onboardingDone, setOnboardingDone] = useState<boolean | null>(null);
    const [selectedResult, setSelectedResult] = useState<MemoryCard | null>(null);
    const [isSidebarOpen, setIsSidebarOpen] = useState(false);
    const [deletedMemoryIds, setDeletedMemoryIds] = useState<Set<string>>(new Set());

    const searchAllowed = true;
    const { results, isLoading, error } = useSearch(
        searchAllowed ? query : "",
        timeFilter,
        appFilter
    );
    const visibleResults = useMemo(
        () => results.filter((item) => !deletedMemoryIds.has(item.id)),
        [results, deletedMemoryIds]
    );

    useEffect(() => {
        getOnboardingState()
            .then((s) => setOnboardingDone(s.step === "complete" && s.model_downloaded))
            .catch(() => setOnboardingDone(false));
    }, []);

    const showCenteredSearch = !query.trim();
    const isFocusMode = !query.trim();
    const consentReminders = [
        "Recording is active. Let everyone know this conversation is being transcribed.",
        "Reminder: confirm participant consent while recording is active.",
    ];
    const consentHint =
        meetingStatus?.consent_state === "detected"
            ? meetingStatus.consent_evidence
                ? `Consent evidence: "${meetingStatus.consent_evidence}"`
                : "Consent language detected in transcript."
            : meetingStatus?.consent_state === "denied"
                ? "Potential objection detected. Pause recording until everyone agrees."
                : "Consent is pending. Ask for explicit permission to record.";

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
        const fetchMeeting = async () => {
            try {
                setMeetingStatus(await getMeetingStatus());
            } catch {
                // Ignore transient polling failures while runtime starts.
            }
        };

        void fetchMeeting();
        const interval = window.setInterval(() => {
            void fetchMeeting();
        }, 2000);

        return () => window.clearInterval(interval);
    }, []);

    useEffect(() => {
        if (!meetingStatus?.is_recording) {
            setConsentPulseTick(0);
            return;
        }

        const pulse = window.setInterval(() => {
            setConsentPulseTick((current) => current + 1);
        }, 6000);
        return () => window.clearInterval(pulse);
    }, [meetingStatus?.is_recording]);



    useEffect(() => {
        if (!visibleResults.length) {
            setSelectedResult(null);
            return;
        }

        setSelectedResult((previous) => {
            if (!previous) {
                return visibleResults[0];
            }

            const stillVisible = visibleResults.find((item) => item.id === previous.id);
            return stillVisible ?? visibleResults[0];
        });
    }, [visibleResults]);

    const handleMemoryDeleted = (memoryId: string) => {
        setDeletedMemoryIds((previous) => {
            const next = new Set(previous);
            next.add(memoryId);
            return next;
        });
    };

    const handleDeleteMemory = async (memoryId: string) => {
        try {
            const deleted = await deleteMemory(memoryId);
            if (!deleted) {
                return;
            }
            handleMemoryDeleted(memoryId);
        } catch (err) {
            console.error("Failed to delete memory:", err);
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

            {!EVAL_UI && meetingStatus?.is_recording && (
                <div className={`recording-consent-banner ${meetingStatus.consent_state}`}>
                    <strong>Recording Active</strong>
                    <span>{meetingStatus.current_title ?? "Detected Meeting"}</span>
                    <span>{consentReminders[consentPulseTick % consentReminders.length]}</span>
                    <span>{consentHint}</span>
                </div>
            )}

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
                        <div className="sidebar-label">Features</div>
                        <button
                            className={`ui-action-btn todo-toggle-btn ${showTodoPanel ? "active" : ""}`}
                            onClick={() => setShowTodoPanel((open) => !open)}
                        >
                            Todo
                        </button>
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
                        <button
                            className={`ui-action-btn memory-cards-toggle-btn ${showMemoryCardsPanel ? "active" : ""}`}
                            onClick={() => setShowMemoryCardsPanel((open) => !open)}
                        >
                            All Cards
                        </button>
                        <button
                            className={`ui-action-btn stats-toggle-btn ${showStatsPanel ? "active" : ""}`}
                            onClick={() => setShowStatsPanel((open) => !open)}
                        >
                            Stats
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
                        resultCount={visibleResults.length}
                        searchResults={visibleResults}
                        disabled={!searchAllowed}
                    />
                </section>

                {!isFocusMode && (
                    <div className="main-layout">
                        <section className="main-column">
                            {error && <div className="error-banner">{error}</div>}

                            <Timeline
                                results={visibleResults}
                                isLoading={isLoading}
                                query={query}
                                selectedResultId={selectedResult?.id ?? null}
                                onSelectResult={setSelectedResult}
                                onDeleteMemory={(memoryId) => void handleDeleteMemory(memoryId)}
                                evalUi={EVAL_UI}
                            />
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
                    <MeetingRecorderPanel
                        isVisible={showMeetingPanel}
                        onClose={() => setShowMeetingPanel(false)}
                        onOpenAgent={() => setShowAgentPanel(true)}
                    />
                    <GraphPanel
                        isVisible={showGraphPanel}
                        onClose={() => setShowGraphPanel(false)}
                    />
                    <MemoryCardsPanel
                        isVisible={showMemoryCardsPanel}
                        onClose={() => setShowMemoryCardsPanel(false)}
                        appNames={appNames}
                        onMemoryDeleted={handleMemoryDeleted}
                    />
                    <StatsPanel
                        isVisible={showStatsPanel}
                        onClose={() => setShowStatsPanel(false)}
                    />
                    <TodoPanel
                        isVisible={showTodoPanel}
                        onClose={() => setShowTodoPanel(false)}
                    />
                </>
            )}
        </div>
    );
}

export default App;
