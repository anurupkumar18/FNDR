import { useEffect, useMemo, useState, useCallback, useRef } from "react";
import { SearchBar } from "./components/SearchBar";
import { Timeline } from "./components/Timeline";
import { ControlPanel } from "./components/ControlPanel";
import { TodoPanel } from "./components/TodoPanel";
import { AgentPanel } from "./components/AgentPanel";
import { MeetingRecorderPanel } from "./components/MeetingRecorderPanel";
import { MemoryCardsPanel } from "./components/MemoryCardsPanel";
import { StatsPanel } from "./components/StatsPanel";
import { DailySummaryPanel } from "./components/DailySummaryPanel";
import { PipelineInspectorPanel } from "./components/PipelineInspectorPanel";
import { ModelDownloadBanner } from "./components/ModelDownloadBanner";
import { Onboarding } from "./components/Onboarding";
import { SearchHistoryPanel, appendToSearchHistory } from "./components/SearchHistoryPanel";
import { QuickSkillsPanel } from "./components/QuickSkillsPanel";
import { FocusSessionPanel } from "./components/FocusSessionPanel";
import { CommandPalette, PanelKey } from "./components/CommandPalette";
import { AutomationPanel, useAutomationScheduler } from "./components/AutomationPanel";
import { ResearchPanel } from "./components/ResearchPanel";
import { TimeTrackingPanel } from "./components/TimeTrackingPanel";
import { FocusModePanel } from "./components/FocusModePanel";
import "./components/FocusModePanel.css";

import { useSearch } from "./hooks/useSearch";
import {
    CaptureStatus,
    MeetingRecorderStatus,
    MemoryCard,
    deleteMemory,
    getAppNames,
    getMeetingStatus,
    onMeetingStatus,
    onProactiveSuggestion,
    getStatus,
    getFunGreeting,
} from "./api/tauri";
import { getOnboardingState, requestBiometricAuth, saveOnboardingState, type OnboardingState } from "./api/onboarding";
import { EVAL_UI } from "./evalUi";
import "./styles/App.css";

function formatHomeDate(now: Date): string {
    const weekday = now.toLocaleDateString(undefined, { weekday: "long" }).toUpperCase();
    const month = now.toLocaleDateString(undefined, { month: "long" }).toUpperCase();
    const day = now.toLocaleDateString(undefined, { day: "numeric" });
    return `${weekday} · ${month} ${day}`;
}

// ── Biometric Lock Screen ─────────────────────────────────────────────────
function BiometricLockScreen({
    onUnlock,
    onDisableBiometricLock,
}: {
    onUnlock: () => void;
    onDisableBiometricLock: () => Promise<void>;
}) {
    const [error, setError] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);
    const [disabling, setDisabling] = useState(false);
    const [attemptCount, setAttemptCount] = useState(0);
    const autoPromptedRef = useRef(false);

    const authenticate = useCallback(async () => {
        setLoading(true);
        setError(null);
        try {
            const ok = await requestBiometricAuth("Unlock FNDR — your private screen history");
            if (ok) {
                onUnlock();
            } else {
                setError("Authentication failed. Tap to try again.");
                setAttemptCount((count) => count + 1);
            }
        } catch {
            setError("Touch ID is unavailable right now. You can retry or continue without lock.");
            setAttemptCount((count) => count + 1);
        } finally {
            setLoading(false);
        }
    }, [onUnlock]);

    // Auto-trigger on mount
    useEffect(() => {
        // React Strict Mode intentionally mounts effects twice in development.
        // Gate auto-auth so users do not get duplicate biometric prompts.
        if (autoPromptedRef.current) {
            return;
        }
        autoPromptedRef.current = true;
        void authenticate();
    }, [authenticate]);

    return (
        <div className="biometric-lock-overlay">
            <div className="biometric-lock-card">
                <div className="biometric-lock-icon">FNDR</div>
                <h1 className="biometric-lock-title">FNDR is Locked</h1>
                <p className="biometric-lock-subtitle">
                    Authenticate with Touch ID or your system password to access your memories.
                </p>
                {error && <div className="biometric-lock-error">{error}</div>}
                <button
                    className="biometric-lock-btn"
                    onClick={() => void authenticate()}
                    disabled={loading}
                >
                    {loading ? "Authenticating…" : "Unlock with Touch ID"}
                </button>
                {attemptCount > 0 && (
                    <button
                        className="biometric-lock-btn"
                        onClick={() => {
                            setDisabling(true);
                            void onDisableBiometricLock().finally(() => setDisabling(false));
                        }}
                        disabled={loading || disabling}
                    >
                        {disabling ? "Unlocking…" : "Continue without biometric lock"}
                    </button>
                )}
            </div>
        </div>
    );
}

function App() {
    const [queryDraft, setQueryDraft] = useState("");
    const [query, setQuery] = useState("");
    const [timeFilter, setTimeFilter] = useState<string | null>(null);
    const [appFilter, setAppFilter] = useState<string | null>(null);
    const [appNames, setAppNames] = useState<string[]>([]);
    const [status, setStatus] = useState<CaptureStatus | null>(null);
    const [meetingStatus, setMeetingStatus] = useState<MeetingRecorderStatus | null>(null);
    // Single active-panel state — only one full-screen panel can be open at a time.
    // CommandPalette is kept separate because it layers on top of the current panel.
    const [activePanel, setActivePanel] = useState<PanelKey | null>(null);
    const [researchSeedMemory, setResearchSeedMemory] = useState<MemoryCard | null>(null);
    const [showCommandPalette, setShowCommandPalette] = useState(false);
    const [focusDriftToast, setFocusDriftToast] = useState<string | null>(null);

    // Background automation scheduler — fires Tauri calls on configured schedules
    useAutomationScheduler();
    const [onboardingDone, setOnboardingDone] = useState<boolean | null>(null);
    const [biometricRequired, setBiometricRequired] = useState<boolean | null>(null);
    const [biometricUnlocked, setBiometricUnlocked] = useState(false);
    const [selectedResult, setSelectedResult] = useState<MemoryCard | null>(null);
    const [isSidebarOpen, setIsSidebarOpen] = useState(false);
    const [deletedMemoryIds, setDeletedMemoryIds] = useState<Set<string>>(new Set());
    const [displayName, setDisplayName] = useState<string | null>(null);
    const [now, setNow] = useState(() => new Date());
    const handleUnlock = useCallback(() => setBiometricUnlocked(true), []);
    const handleDisableBiometricLock = useCallback(async () => {
        try {
            const current = await getOnboardingState();
            await saveOnboardingState({
                ...current,
                biometric_enabled: false,
            });
        } catch (err) {
            console.error("Failed to disable biometric lock:", err);
        } finally {
            setBiometricRequired(false);
            setBiometricUnlocked(true);
        }
    }, []);

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
                setBiometricRequired(s.biometric_enabled === true);
            })
            .catch(() => {
                setOnboardingDone(false);
                setDisplayName(null);
                setBiometricRequired(false);
            });
    }, []);

    const isFocusMode = !query.trim();
    const homeDateLabel = useMemo(() => formatHomeDate(now), [now]);

    const [homeGreeting, setHomeGreeting] = useState("Loading...");
    const homeGreetingLine1 = useMemo(() => {
        const suffix = "Let's dive into your memories.";
        const trimmed = homeGreeting.trim();
        const withoutSuffix = trimmed.endsWith(suffix)
            ? trimmed.slice(0, -suffix.length).trim()
            : trimmed;
        const exclamationIndex = withoutSuffix.indexOf("!");
        if (exclamationIndex >= 0) {
            return withoutSuffix.slice(0, exclamationIndex + 1).trim();
        }
        return withoutSuffix;
    }, [homeGreeting]);

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
        if (normalized) appendToSearchHistory(normalized);
        if (typeof nextValue === "string") {
            setQueryDraft(nextValue);
        }
    };

    // Run a Quick Skill: set query + optional time filter, then submit
    const handleRunSkill = (skillQuery: string, timeFilter?: string) => {
        if (timeFilter) setTimeFilter(timeFilter);
        setQueryDraft(skillQuery);
        setQuery(skillQuery);
        if (skillQuery) appendToSearchHistory(skillQuery);
    };

    // Run a search for a specific app (from Focus Session panel)
    const handleSearchApp = (appName: string) => {
        setAppFilter(appName);
        setQueryDraft("");
        setQuery(" "); // trigger search with only app filter
    };

    // Command Palette panel dispatcher — opens any panel by key
    const handleOpenPanel = useCallback((panel: PanelKey) => {
        setShowCommandPalette(false);
        setActivePanel(panel);
    }, []);

    // Research trigger — opens Research panel seeded with a memory
    const handleResearchMemory = useCallback((memory: MemoryCard) => {
        setResearchSeedMemory(memory);
        setActivePanel("research");
        setShowCommandPalette(false);
    }, []);

    // Global Cmd+K / Ctrl+K listener
    useEffect(() => {
        const handler = (e: KeyboardEvent) => {
            if ((e.metaKey || e.ctrlKey) && e.key === "k") {
                e.preventDefault();
                setShowCommandPalette((prev) => !prev);
            }
        };
        window.addEventListener("keydown", handler);
        return () => window.removeEventListener("keydown", handler);
    }, []);

    // Proactive suggestion listener — surfaces focus drift alerts as a toast.
    // Uses async inner function so the unlisten handle is guaranteed to be
    // assigned before any cleanup can run, avoiding a listener leak.
    useEffect(() => {
        let unlisten: (() => void) | null = null;
        let mounted = true;

        const subscribe = async () => {
            try {
                const fn = await onProactiveSuggestion((suggestion) => {
                    if (suggestion.memory_id === "focus_drift") {
                        setFocusDriftToast(suggestion.snippet);
                        setTimeout(() => setFocusDriftToast(null), 8_000);
                    }
                });
                if (mounted) {
                    unlisten = fn;
                } else {
                    fn(); // component already unmounted — immediately release
                }
            } catch {
                // non-fatal; proactive surface is best-effort
            }
        };

        void subscribe();
        return () => {
            mounted = false;
            if (unlisten) unlisten();
        };
    }, []);



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

    if (onboardingDone === null || biometricRequired === null) {
        return null;
    }

    if (!onboardingDone) {
        return (
            <Onboarding
                onComplete={(next: OnboardingState) => {
                    setOnboardingDone(true);
                    setDisplayName(next.display_name ?? null);
                    setBiometricRequired(next.biometric_enabled === true);
                    // If they just enabled biometrics, mark as already unlocked for this session
                    if (next.biometric_enabled) {
                        setBiometricUnlocked(true);
                    }
                }}
            />
        );
    }

    if (biometricRequired && !biometricUnlocked) {
        return (
            <BiometricLockScreen
                onUnlock={handleUnlock}
                onDisableBiometricLock={handleDisableBiometricLock}
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
                    {isSidebarOpen ? "Close" : "Menu"}
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
                    {(
                        [
                            { label: "Features", items: [
                                { key: "memoryCards" as PanelKey, text: "Memory Cards" },
                                { key: "stats" as PanelKey, text: "Stats" },
                                { key: "todo" as PanelKey, text: "To Do" },
                                { key: "meeting" as PanelKey, text: "Meetings" },
                                { key: "dailySummary" as PanelKey, text: "Daily Summary" },
                                { key: "agent" as PanelKey, text: "Agent" },
                                { key: "pipeline" as PanelKey, text: "Pipeline Inspector" },
                            ]},
                            { label: "Smart", items: [
                                { key: "focusSession" as PanelKey, text: "Focus Session" },
                                { key: "quickSkills" as PanelKey, text: "Quick Skills" },
                                { key: "searchHistory" as PanelKey, text: "Search History" },
                                { key: "automation" as PanelKey, text: "Automation" },
                                { key: "research" as PanelKey, text: "Research" },
                                { key: "timeTracking" as PanelKey, text: "Time Tracking" },
                                { key: "focusMode" as PanelKey, text: "Focus Mode" },
                            ]},
                        ] as const
                    ).map((group) => (
                        <div key={group.label} className="sidebar-group sidebar-actions">
                            <div className="sidebar-label">{group.label}</div>
                            {group.items.map(({ key, text }) => (
                                <button
                                    key={key}
                                    className={`ui-action-btn ${activePanel === key ? "active" : ""}`}
                                    onClick={() => {
                                        if (key === "research") setResearchSeedMemory(null);
                                        setActivePanel(activePanel === key ? null : key);
                                        setIsSidebarOpen(false);
                                    }}
                                >
                                    {text}
                                </button>
                            ))}
                        </div>
                    ))}

                    <div className="sidebar-group sidebar-actions">
                        <div className="sidebar-label">Commands</div>
                        <button
                            className="ui-action-btn"
                            onClick={() => { setShowCommandPalette(true); setIsSidebarOpen(false); }}
                        >
                            Cmd+K Palette
                        </button>
                    </div>
                </aside>
            )}

            <main className={`app-main ${isFocusMode ? "search-centered" : ""}`}>
                {isFocusMode && (
                    <div className="home-focus-header">
                        <div className="home-date-context">{homeDateLabel}</div>
                        <div className="home-greeting">
                            <div>{homeGreetingLine1}</div>
                            <div>Let&apos;s dive into your memories.</div>
                        </div>
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
                        onSetMeetingPanelOpen={(open) => setActivePanel(open ? "meeting" : null)}
                        onSetMemoryCardsPanelOpen={(open) => setActivePanel(open ? "memoryCards" : null)}
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
                        isVisible={activePanel === "agent"}
                        onClose={() => setActivePanel(null)}
                    />
                    <MeetingRecorderPanel
                        isVisible={activePanel === "meeting"}
                        onClose={() => setActivePanel(null)}
                    />
                    <MemoryCardsPanel
                        isVisible={activePanel === "memoryCards"}
                        onClose={() => setActivePanel(null)}
                        appNames={appNames}
                        onMemoryDeleted={handleMemoryDeleted}
                    />
                    <StatsPanel
                        isVisible={activePanel === "stats"}
                        onClose={() => setActivePanel(null)}
                    />
                    <TodoPanel
                        isVisible={activePanel === "todo"}
                        onClose={() => setActivePanel(null)}
                    />
                    <DailySummaryPanel
                        isVisible={activePanel === "dailySummary"}
                        onClose={() => setActivePanel(null)}
                    />
                    <PipelineInspectorPanel
                        isVisible={activePanel === "pipeline"}
                        onClose={() => setActivePanel(null)}
                        currentQuery={query}
                        timeFilter={timeFilter}
                        appFilter={appFilter}
                    />
                    <SearchHistoryPanel
                        isVisible={activePanel === "searchHistory"}
                        onClose={() => setActivePanel(null)}
                        onRunQuery={handleSearchSubmit}
                    />
                    <QuickSkillsPanel
                        isVisible={activePanel === "quickSkills"}
                        onClose={() => setActivePanel(null)}
                        onRunSkill={handleRunSkill}
                    />
                    <FocusSessionPanel
                        isVisible={activePanel === "focusSession"}
                        onClose={() => setActivePanel(null)}
                        onSearchApp={handleSearchApp}
                    />
                    <AutomationPanel
                        isVisible={activePanel === "automation"}
                        onClose={() => setActivePanel(null)}
                    />
                    <ResearchPanel
                        isVisible={activePanel === "research"}
                        onClose={() => setActivePanel(null)}
                        seedMemory={researchSeedMemory}
                    />
                    <TimeTrackingPanel
                        isVisible={activePanel === "timeTracking"}
                        onClose={() => setActivePanel(null)}
                        onSearchApp={handleSearchApp}
                    />
                    <FocusModePanel
                        isVisible={activePanel === "focusMode"}
                        onClose={() => setActivePanel(null)}
                    />
                    <CommandPalette
                        isOpen={showCommandPalette}
                        onClose={() => setShowCommandPalette(false)}
                        selectedMemory={selectedResult}
                        context={{
                            query,
                            onOpenPanel: handleOpenPanel,
                            onSearch: (q) => handleSearchSubmit(q),
                            onSearchApp: handleSearchApp,
                            onClearSearch: () => { setQuery(""); setQueryDraft(""); setTimeFilter(null); setAppFilter(null); },
                            onDeleteMemory: handleMemoryDeleted,
                            onResearch: handleResearchMemory,
                            isCapturing: status?.is_capturing ?? false,
                        }}
                    />
                    {/* Focus drift toast */}
                    {focusDriftToast && (
                        <div className="fm-drift-toast">
                            <div className="fm-toast-header">
                                <span className="fm-toast-title">Focus Drift Detected</span>
                                <button
                                    className="fm-toast-dismiss"
                                    onClick={() => setFocusDriftToast(null)}
                                >
                                    ×
                                </button>
                            </div>
                            <p className="fm-toast-body">{focusDriftToast}</p>
                            <button
                                className="fm-toast-dismiss"
                                style={{ alignSelf: "flex-start", fontSize: "0.76rem", padding: "4px 8px", border: "1px solid rgba(230,150,60,0.25)", borderRadius: "6px", opacity: 1, color: "rgba(230,165,80,0.85)" }}
                                onClick={() => { setFocusDriftToast(null); setActivePanel("focusMode"); }}
                            >
                                View Focus Mode
                            </button>
                        </div>
                    )}
                </>
            )}
        </div>
    );
}

export default App;
