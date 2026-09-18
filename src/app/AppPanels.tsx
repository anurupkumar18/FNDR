import type { MemoryCard } from "@/shared/ipc/tauri";
import { AskPanel } from "@/domains/ask/AskPanel";
import { CommandPalette, type PanelKey } from "@/domains/command-palette/CommandPalette";
import { MemoryCardsPanel } from "@/domains/memory-vault/MemoryCardsPanel";
import { ScreenGuidePanel } from "@/domains/screen-guide/ScreenGuidePanel";
import { DailySummaryPanel } from "@/domains/workspace/DailySummaryPanel";
import { FndrWrappedPanel } from "@/domains/workspace/FndrWrappedPanel";
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
    onGoHome: () => void;
    onMemoryDeleted: (memoryId: string) => void;
    onOpenPanel: (panel: PanelKey) => void;
    onRunQuery: (query: string) => void;
    onSearchApp: (appName: string) => void;
    onToastAction: (toast: AppToast) => void;
    onOpenMemoryById: (memoryId: string) => void;
}

/** Alpha demo surface: Vault, Ask FNDR, Daily Summary, Wrapped, Screen Guide.
 *  Hidden rank-2 panels stay compiled under src/domains but are not mounted. */
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
            <PanelErrorBoundary panelName="Ask FNDR">
                <AskPanel isVisible={activePanel === "ask"} onClose={onClosePanel} onOpenMemoryById={onOpenMemoryById} />
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
            <PanelErrorBoundary panelName="Daily Summary">
                <DailySummaryPanel
                    isVisible={activePanel === "dailySummary"}
                    onClose={onClosePanel}
                    onOpenMemoryById={onOpenMemoryById}
                />
            </PanelErrorBoundary>
            <PanelErrorBoundary panelName="FNDR Wrapped">
                <FndrWrappedPanel isVisible={activePanel === "wrapped"} onClose={onClosePanel} />
            </PanelErrorBoundary>
            <PanelErrorBoundary panelName="Screen Guide">
                <ScreenGuidePanel isVisible={activePanel === "screenGuide"} onClose={onClosePanel} />
            </PanelErrorBoundary>
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
                    isCapturing,
                }}
            />
            <AppToasts toasts={appToasts} onAction={onToastAction} onDismiss={onDismissToast} />
        </>
    );
}
