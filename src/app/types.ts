import type { MountedPanelKey } from "./panels";

export interface AppToast {
    id: string;
    title: string;
    body: string;
    kind: string;
    actionLabel?: string;
    targetPanel?: MountedPanelKey;
    /** When set with targetPanel "memoryCards", the vault opens focused on this memory. */
    memoryId?: string;
}
