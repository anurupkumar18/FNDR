import { useEffect, useMemo, useState } from "react";
import { SearchBar } from "./components/SearchBar";
import { Timeline } from "./components/Timeline";
import { ControlPanel } from "./components/ControlPanel";
import { TodoPanel } from "./components/TodoPanel";
import { AgentPanel } from "./components/AgentPanel";
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
    onMeetingStatus,
    getStatus,
    getFunGreeting,
} from "./api/tauri";
import { getOnboardingState } from "./api/onboarding";
import { EVAL_UI } from "./evalUi";
import "./styles/App.css";

function formatHomeDate(now: Date): string {
    const weekday = now.toLocaleDateString(undefined, { weekday: "long" }).toUpperCase();
    const month = now.toLocaleDateString(undefined, { month: "long" }).toUpperCase();
    const day = now.toLocaleDateString(undefined, { day: "numeric" });
    return `${weekday} · ${month} ${day}`;
}



function App() {
    const [queryDraft, setQueryDraft] = useState("");
    const [query, setQuery] = useState("");
    const [timeFilter, setTimeFilter] = useState<string | null>(null);
    const [appFilter, setAppFilter] = useState<string | null>(null);
    const [appNames, setAppNames] = useState<string[]>([]);
    const [status, setStatus] = useState<CaptureStatus | null>(null);
    const [meetingStatus, setMeetingStatus] = useState<MeetingRecorderStatus | null>(null);
    const [showAgentPanel, setShowAgentPanel] = useState(false);
    const [showMeetingPanel, setShowMeetingPanel] = useState(false);
    const [showMemoryCardsPanel, setShowMemoryCardsPanel] = useState(false);
    const [showStatsPanel, setShowStatsPanel] = useState(false);
    const [showTodoPanel, setShowTodoPanel] = useState(false);
    const [onboardingDone, setOnboardingDone] = useState<boolean | null>(null);
    const [selectedResult, setSelectedResult] = useState<MemoryCard | null>(null);
    const [isSidebarOpen, setIsSidebarOpen] = useState(false);
    const [deletedMemoryIds, setDeletedMemoryIds] = useState<Set<string>>(new Set());
    const [displayName, setDisplayName] = useState<string | null>(null);
    const [now, setNow] = useState(() => new Date());

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
            .then((s) => {
                setOnboardingDone(s.step === "complete" && s.model_downloaded);
                setDisplayName(s.display_name ?? null);
            })
            .catch(() => {
                setOnboardingDone(false);
                setDisplayName(null);
            });
    }, []);

    const isFocusMode = !query.trim();
    const homeDateLabel = useMemo(() => formatHomeDate(now), [now]);
    
    const [homeGreeting, setHomeGreeting] = useState("Loading...");

    // Fetch the fun animated greeting anytime they log in or the name changes
    useEffect(() => {
        getFunGreeting(displayName).then(setHomeGreeting).catch(() => {
            setHomeGreeting("Welcome back to FNDR.");
        });
    }, [displayName]);

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
        let mounted = true;
        let unlisten: (() => void) | null = null;

        const fetchMeeting = async () => {
            try {
                const nextStatus = await getMeetingStatus();
                if (!mounted) return;
                setMeetingStatus(nextStatus);
            } catch {
                // Ignore transient meeting status failures while runtime starts.
            }
        };

        const subscribe = async () => {
            try {
                unlisten = await onMeetingStatus((nextStatus) => {
                    if (!mounted) return;
                    setMeetingStatus(nextStatus);
                });
            } catch {
                // Ignore listener registration errors; manual refresh paths remain available.
            }
        };

        void fetchMeeting();
        void subscribe();

        return () => {
            mounted = false;
            if (unlisten) {
                unlisten();
            }
        };
    }, []);

    useEffect(() => {
        const timer = window.setInterval(() => {
            setNow(new Date());
        }, 60_000);
        return () => window.clearInterval(timer);
    }, []);

    useEffect(() => {
        const handleProfileUpdated = (event: Event) => {
            const customEvent = event as CustomEvent<{ displayName: string | null }>;
            setDisplayName(customEvent.detail?.displayName ?? null);
        };
        window.addEventListener("fndr-profile-updated", handleProfileUpdated as EventListener);
        return () =>
            window.removeEventListener("fndr-profile-updated", handleProfileUpdated as EventListener);
    }, []);

    useEffect(() => {
        if (query.trim().length > 0) {
            return;
        }
        setTimeFilter(null);
        setAppFilter(null);
    }, [query]);

    const handleSearchSubmit = (nextValue?: string) => {
        const source = nextValue ?? queryDraft;
        const normalized = source
            .replace(/\r?\n/g, " ")
            .replace(/\s+/g, " ")
            .trim();
        setQuery(normalized);
        if (typeof nextValue === "string") {
            setQueryDraft(nextValue);
        }
    };



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
        return (
            <Onboarding
                onComplete={(next) => {
                    setOnboardingDone(true);
                    setDisplayName(next.display_name ?? null);
                }}
            />
        );
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
                <div className="recording-consent-banner pending">
                    <strong>Recording Active</strong>
                    <span>{meetingStatus.current_title ?? "Meeting"}</span>
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
                            className={`ui-action-btn memory-cards-toggle-btn ${showMemoryCardsPanel ? "active" : ""}`}
                            onClick={() => setShowMemoryCardsPanel((open) => !open)}
                        >
                            Memory Cards
                        </button>
                        <button
                            className={`ui-action-btn stats-toggle-btn ${showStatsPanel ? "active" : ""}`}
                            onClick={() => setShowStatsPanel((open) => !open)}
                        >
                            Stats
                        </button>
                        <button
                            className={`ui-action-btn todo-toggle-btn ${showTodoPanel ? "active" : ""}`}
                            onClick={() => setShowTodoPanel((open) => !open)}
                        >
                            To do
                        </button>
                        <button
                            className={`ui-action-btn meeting-toggle-btn ${showMeetingPanel ? "active" : ""}`}
                            onClick={() => setShowMeetingPanel((open) => !open)}
                        >
                            Meetings
                        </button>
                    </div>
                </aside>
            )}

            <main className={`app-main ${isFocusMode ? "search-centered" : ""}`}>
                {isFocusMode && (
                    <div className="home-focus-header">
                        <div className="home-greeting">{homeGreeting}</div>
                        <div className="home-date-context">{homeDateLabel}</div>
                    </div>
                )}

                <section className={`search-shell ${query.trim() ? "is-active" : ""}`}>
                    <SearchBar
                        value={queryDraft}
                        submittedValue={query}
                        onChange={setQueryDraft}
                        onSubmit={handleSearchSubmit}
                        timeFilter={timeFilter}
                        onTimeFilterChange={setTimeFilter}
                        appFilter={appFilter}
                        onAppFilterChange={setAppFilter}
                        onSetMeetingPanelOpen={setShowMeetingPanel}
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
