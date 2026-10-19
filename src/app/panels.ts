import type { PanelKey } from "@/domains/command-palette/CommandPalette";

/** Panels mounted by the five-destination shell. Historical panels may stay
 * compiled, but callers must pass this boundary before changing the visible
 * foreground. */
export const MOUNTED_PANEL_KEYS = [
    "ask",
    "memoryCards",
    "dailySummary",
    "outputs",
    "stats",
    "todo",
    "wrapped",
    "privacyProof",
    "screenGuide",
    "agent",
    "engineMetrics",
] as const satisfies readonly PanelKey[];

export type MountedPanelKey = (typeof MOUNTED_PANEL_KEYS)[number];

/** Existing panels that support the five everyday destinations. Home and
 * Trust & Settings are shell actions rather than foreground panels. */
export const PRIMARY_DESTINATION_PANEL_KEYS = [
    "ask",
    "memoryCards",
    "dailySummary",
    "stats",
    "todo",
    "wrapped",
    "privacyProof",
] as const satisfies readonly MountedPanelKey[];

/** Experimental surfaces remain available, but are not primary navigation. */
export const LAB_PANEL_KEYS = ["screenGuide", "agent", "engineMetrics"] as const satisfies readonly MountedPanelKey[];

export function isMountedPanelKey(panel: PanelKey): panel is MountedPanelKey {
    return (MOUNTED_PANEL_KEYS as readonly PanelKey[]).includes(panel);
}
