import type { MemoryCard } from "@/shared/ipc/tauri";
import { AgentPanel } from "@/domains/workspace/AgentPanel";
import { CommandPalette, type PanelKey } from "@/domains/command-palette/CommandPalette";
import { MemoryCardsPanel } from "@/domains/memory-vault/MemoryCardsPanel";
import { EngineMetricsPanel } from "@/domains/workspace/EngineMetricsPanel";
import { AppToasts } from "./AppToasts";
import { PanelErrorBoundary } from "./PanelErrorBoundary";
import type { AppToast } from "./types";

interface AppPanelsProps {
    activePanel: PanelKey | null;
    appNames: string[];
    appToasts: AppToast[];
    isCapturing: boolean;
    query: string;
    selectedResult: MemoryCard | null;
    showCommandPalette: boolean;
    memoryVaultFocusId: string | null;
    onClearSearch: () => void;
    onCloseCommandPalette: () => void;
    onClosePanel: () => void;
    onDeleteMemory: (memoryId: string) => void;
    onDismissToast: (toastId: string) => void;
    onMemoryDeleted: (memoryId: string) => void;
    onOpenPanel: (panel: PanelKey) => void;
    onRunQuery: (query: string) => void;
    onSearchApp: (appName: string) => void;
    onToastAction: (toast: AppToast) => void;
    onOpenMemoryById: (memoryId: string) => void;
}

export function AppPanels({
    activePanel,
    appNames,
    appToasts,
    isCapturing,
    query,
    selectedResult,
    showCommandPalette,
    memoryVaultFocusId,
    onClearSearch,
    onCloseCommandPalette,
    onClosePanel,
    onDeleteMemory,
    onDismissToast,
    onMemoryDeleted,
    onOpenPanel,
    onRunQuery,
    onSearchApp,
    onToastAction,
    onOpenMemoryById,
}: AppPanelsProps) {
    return (
        <>
            <PanelErrorBoundary panelName="Agent">
                <AgentPanel isVisible={activePanel === "agent"} onClose={onClosePanel} mode="context" />
            </PanelErrorBoundary>
            <MemoryCardsPanel
                isVisible={activePanel === "memoryCards"}
                onClose={onClosePanel}
                appNames={appNames}
                onMemoryDeleted={onMemoryDeleted}
                feature="vault"
                focusMemoryId={memoryVaultFocusId}
                onOpenMemoryById={onOpenMemoryById}
            />
            <EngineMetricsPanel
                isVisible={activePanel === "engineMetrics"}
                onClose={onClosePanel}
            />
            <CommandPalette
                isOpen={showCommandPalette}
                onClose={onCloseCommandPalette}
                selectedMemory={selectedResult}
                demoOnly
                context={{
                    query,
                    onOpenPanel,
                    onSearch: onRunQuery,
                    onSearchApp,
                    onClearSearch,
                    onDeleteMemory,
                    onResearch: () => undefined,
                    isCapturing,
                }}
            />
            <AppToasts toasts={appToasts} onAction={onToastAction} onDismiss={onDismissToast} />
        </>
    );
}
