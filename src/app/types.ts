import type { PanelKey } from "@/domains/command-palette/CommandPalette";

export interface AppToast {
    id: string;
    title: string;
    body: string;
    kind: string;
    actionLabel?: string;
    targetPanel?: PanelKey;
}
