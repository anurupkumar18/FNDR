import type { PanelKey } from "@/domains/command-palette/CommandPalette";

/** Panels mounted by the Alpha shell. Historical panels may stay compiled,
 * but callers must pass this boundary before changing the visible foreground. */
export const MOUNTED_PANEL_KEYS = [
    "memoryCards",
    "ask",
    "dailySummary",
    "stats",
    "todo",
    "wrapped",
    "screenGuide",
    "engineMetrics",
    "privacyProof",
] as const satisfies readonly PanelKey[];

export type MountedPanelKey = (typeof MOUNTED_PANEL_KEYS)[number];

export function isMountedPanelKey(panel: PanelKey): panel is MountedPanelKey {
    return (MOUNTED_PANEL_KEYS as readonly PanelKey[]).includes(panel);
}
