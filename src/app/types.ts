import type { PanelKey } from "../components/CommandPalette";

export interface AppToast {
    id: string;
    title: string;
    body: string;
    kind: string;
    actionLabel?: string;
    targetPanel?: PanelKey;
}
