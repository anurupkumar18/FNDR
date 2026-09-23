import type { MemoryCard } from "@/shared/ipc/tauri";
import { AskPanel } from "@/domains/ask/AskPanel";
import { CommandPalette, type PanelKey } from "@/domains/command-palette/CommandPalette";
import { MemoryCardsPanel } from "@/domains/memory-vault/MemoryCardsPanel";
import { ScreenGuidePanel } from "@/domains/screen-guide/ScreenGuidePanel";
import { DailySummaryPanel } from "@/domains/workspace/DailySummaryPanel";
import { FndrWrappedPanel } from "@/domains/workspace/FndrWrappedPanel";
import { StatsPanel } from "@/domains/workspace/StatsPanel";
import { TodoPanel } from "@/domains/workspace/TodoPanel";
import { EngineMetricsPanel } from "@/domains/workspace/EngineMetricsPanel";
import { PrivacyProofPanel } from "@/domains/privacy-proof/PrivacyProof";
import { AppToasts } from "./AppToasts";
import { PanelPresence } from "./PanelPresence";
import { PanelErrorBoundary } from "./PanelErrorBoundary";
import type { AppToast } from "./types";
import type { MountedPanelKey } from "./panels";

interface AppPanelsProps {
    activePanel: MountedPanelKey | null;
    appNames: string[];
    appToasts: AppToast[];
    isCapturePaused: boolean;
    query: string;
    selectedResult: MemoryCard | null;
    showCommandPalette: boolean;
    memoryVaultFocusId: string | null;
    onClearSearch: () => void;
    onCloseCommandPalette: () => void;
    onClosePanel: () => void;
    onDeleteMemory: (memoryId: string) => void;
    onDismissToast: (toastId: string) => void;
    onGoHome: () => void;
    onMemoryDeleted: (memoryId: string) => void;
    onOpenPanel: (panel: PanelKey) => void;
    onRunQuery: (query: string) => void;
    onSearchApp: (appName: string) => void;
    onToastAction: (toast: AppToast) => void;
    onOpenMemoryById: (memoryId: string) => void;
}

/** Alpha demo surface: Vault, Search & Ask, daily reflection, Stats, To-dos,
 *  Wrapped, Screen Guide, and Engine diagnostics.
 *  Hidden rank-2 panels stay compiled under src/domains but are not mounted. */
export function AppPanels({
    activePanel,
    appNames,
    appToasts,
    isCapturePaused,
    query,
    selectedResult,
    showCommandPalette,
    memoryVaultFocusId,
    onClearSearch,
    onCloseCommandPalette,
    onClosePanel,
    onDeleteMemory,
    onDismissToast,
    onGoHome,
    onMemoryDeleted,
    onOpenPanel,
    onRunQuery,
    onSearchApp,
    onToastAction,
    onOpenMemoryById,
}: AppPanelsProps) {
    return (
        <>
            <PanelPresence open={activePanel === "ask"}>
                {(present) => (
                    <PanelErrorBoundary panelName="Ask FNDR" onClose={onClosePanel}>
                        <AskPanel isVisible={present} onClose={onClosePanel} onOpenMemoryById={onOpenMemoryById} />
                    </PanelErrorBoundary>
                )}
            </PanelPresence>
            <PanelPresence open={activePanel === "memoryCards"}>
                {(present) => (
                    <MemoryCardsPanel
                        isVisible={present}
                        onClose={onClosePanel}
                        appNames={appNames}
                        onMemoryDeleted={onMemoryDeleted}
                        feature="vault"
                        focusMemoryId={memoryVaultFocusId}
                        onOpenMemoryById={onOpenMemoryById}
                    />
                )}
            </PanelPresence>
            <PanelPresence open={activePanel === "dailySummary"}>
                {(present) => (
                    <PanelErrorBoundary panelName="Daily Summary" onClose={onClosePanel}>
                        <DailySummaryPanel
                            isVisible={present}
                            onClose={onClosePanel}
                            onOpenMemoryById={onOpenMemoryById}
                        />
                    </PanelErrorBoundary>
                )}
            </PanelPresence>
            <PanelPresence open={activePanel === "stats"}>
                {(present) => (
                    <PanelErrorBoundary panelName="Stats" onClose={onClosePanel}>
                        <StatsPanel isVisible={present} onClose={onClosePanel} />
                    </PanelErrorBoundary>
                )}
            </PanelPresence>
            <PanelPresence open={activePanel === "todo"}>
                {(present) => (
                    <PanelErrorBoundary panelName="To-dos" onClose={onClosePanel}>
                        <TodoPanel isVisible={present} onClose={onClosePanel} />
                    </PanelErrorBoundary>
                )}
            </PanelPresence>
            <PanelPresence open={activePanel === "wrapped"}>
                {(present) => (
                    <PanelErrorBoundary panelName="FNDR Wrapped" onClose={onClosePanel}>
                        <FndrWrappedPanel isVisible={present} onClose={onClosePanel} />
                    </PanelErrorBoundary>
                )}
            </PanelPresence>
            <PanelPresence open={activePanel === "screenGuide"}>
                {(present) => (
                    <PanelErrorBoundary panelName="Screen Guide" onClose={onClosePanel}>
                        <ScreenGuidePanel isVisible={present} onClose={onClosePanel} />
                    </PanelErrorBoundary>
                )}
            </PanelPresence>
            <PanelPresence open={activePanel === "engineMetrics"}>
                {(present) => (
                    <PanelErrorBoundary panelName="Engine diagnostics" onClose={onClosePanel}>
                        <EngineMetricsPanel isVisible={present} onClose={onClosePanel} />
                    </PanelErrorBoundary>
                )}
            </PanelPresence>
            <PanelPresence open={activePanel === "privacyProof"}>
                {(present) => (
                    <PanelErrorBoundary panelName="Privacy Activity" onClose={onClosePanel}>
                        <PrivacyProofPanel isVisible={present} onClose={onClosePanel} />
                    </PanelErrorBoundary>
                )}
            </PanelPresence>
            <CommandPalette
                isOpen={showCommandPalette}
                onClose={onCloseCommandPalette}
                selectedMemory={selectedResult}
                demoOnly
                context={{
                    query,
                    onOpenPanel,
                    onGoHome,
                    onSearch: onRunQuery,
                    onSearchApp,
                    onClearSearch,
                    onDeleteMemory,
                    isCapturePaused,
                }}
            />
            <AppToasts toasts={appToasts} onAction={onToastAction} onDismiss={onDismissToast} />
        </>
    );
}
