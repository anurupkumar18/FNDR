import type { MemoryCard } from "../api/tauri";
import { AgentPanel } from "../components/AgentPanel";
import { AutomationPanel } from "../components/AutomationPanel";
import { CommandPalette, type PanelKey } from "../components/CommandPalette";
import { DailySummaryPanel } from "../components/DailySummaryPanel";
import { FocusModePanel } from "../components/FocusModePanel";
import { FocusSessionPanel } from "../components/FocusSessionPanel";
import { MeetingRecorderPanel } from "../components/MeetingRecorderPanel";
import { MemoryCardsPanel } from "../components/MemoryCardsPanel";
import { PipelineInspectorPanel } from "../components/PipelineInspectorPanel";
import { QuickSkillsPanel } from "../components/QuickSkillsPanel";
import { ResearchPanel } from "../components/ResearchPanel";
import { SearchHistoryPanel } from "../components/SearchHistoryPanel";
import { StatsPanel } from "../components/StatsPanel";
import { TimeTrackingPanel } from "../components/TimeTrackingPanel";
import { TodoPanel } from "../components/TodoPanel";
import { AppToasts } from "./AppToasts";
import { PanelErrorBoundary } from "./PanelErrorBoundary";
import type { AppToast } from "./types";

interface AppPanelsProps {
    activePanel: PanelKey | null;
    appFilter: string | null;
    appNames: string[];
    appToasts: AppToast[];
    isCapturing: boolean;
    query: string;
    researchSeedMemory: MemoryCard | null;
    selectedResult: MemoryCard | null;
    showCommandPalette: boolean;
    timeFilter: string | null;
    onClearSearch: () => void;
    onCloseCommandPalette: () => void;
    onClosePanel: () => void;
    onDeleteMemory: (memoryId: string) => void;
    onDismissToast: (toastId: string) => void;
    onMemoryDeleted: (memoryId: string) => void;
    onOpenPanel: (panel: PanelKey) => void;
    onResearchMemory: (memory: MemoryCard) => void;
    onRunQuery: (query: string) => void;
    onRunSkill: (skillQuery: string, timeFilter?: string) => void;
    onSearchApp: (appName: string) => void;
    onToastAction: (toast: AppToast) => void;
}

export function AppPanels({
    activePanel,
    appFilter,
    appNames,
    appToasts,
    isCapturing,
    query,
    researchSeedMemory,
    selectedResult,
    showCommandPalette,
    timeFilter,
    onClearSearch,
    onCloseCommandPalette,
    onClosePanel,
    onDeleteMemory,
    onDismissToast,
    onMemoryDeleted,
    onOpenPanel,
    onResearchMemory,
    onRunQuery,
    onRunSkill,
    onSearchApp,
    onToastAction,
}: AppPanelsProps) {
    return (
        <>
            <PanelErrorBoundary panelName="Agent">
                <AgentPanel isVisible={activePanel === "agent"} onClose={onClosePanel} />
            </PanelErrorBoundary>
            <MeetingRecorderPanel isVisible={activePanel === "meeting"} onClose={onClosePanel} />
            <MemoryCardsPanel
                isVisible={activePanel === "memoryCards"}
                onClose={onClosePanel}
                appNames={appNames}
                onMemoryDeleted={onMemoryDeleted}
            />
            <StatsPanel isVisible={activePanel === "stats"} onClose={onClosePanel} />
            <TodoPanel isVisible={activePanel === "todo"} onClose={onClosePanel} />
            <DailySummaryPanel isVisible={activePanel === "dailySummary"} onClose={onClosePanel} />
            <PipelineInspectorPanel
                isVisible={activePanel === "pipeline"}
                onClose={onClosePanel}
                currentQuery={query}
                timeFilter={timeFilter}
                appFilter={appFilter}
            />
            <SearchHistoryPanel
                isVisible={activePanel === "searchHistory"}
                onClose={onClosePanel}
                onRunQuery={onRunQuery}
            />
            <QuickSkillsPanel
                isVisible={activePanel === "quickSkills"}
                onClose={onClosePanel}
                onRunSkill={onRunSkill}
            />
            <FocusSessionPanel
                isVisible={activePanel === "focusSession"}
                onClose={onClosePanel}
                onSearchApp={onSearchApp}
            />
            <AutomationPanel isVisible={activePanel === "automation"} onClose={onClosePanel} />
            <ResearchPanel
                isVisible={activePanel === "research"}
                onClose={onClosePanel}
                seedMemory={researchSeedMemory}
            />
            <TimeTrackingPanel
                isVisible={activePanel === "timeTracking"}
                onClose={onClosePanel}
                onSearchApp={onSearchApp}
            />
            <FocusModePanel isVisible={activePanel === "focusMode"} onClose={onClosePanel} />
            <CommandPalette
                isOpen={showCommandPalette}
                onClose={onCloseCommandPalette}
                selectedMemory={selectedResult}
                context={{
                    query,
                    onOpenPanel,
                    onSearch: onRunQuery,
                    onSearchApp,
                    onClearSearch,
                    onDeleteMemory,
                    onResearch: onResearchMemory,
                    isCapturing,
                }}
            />
            <AppToasts toasts={appToasts} onAction={onToastAction} onDismiss={onDismissToast} />
        </>
    );
}
