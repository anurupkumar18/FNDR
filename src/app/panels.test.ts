import { describe, expect, it } from "vitest";
import { MOUNTED_PANEL_KEYS, isMountedPanelKey } from "./panels";

describe("Alpha foreground panel registry", () => {
    it("contains every and only the destinations mounted by AppPanels", () => {
        expect(MOUNTED_PANEL_KEYS).toEqual([
            "memoryCards",
            "ask",
            "dailySummary",
            "stats",
            "todo",
            "wrapped",
            "screenGuide",
            "engineMetrics",
            "privacyProof",
            "agent",
        ]);
        expect(isMountedPanelKey("memoryCards")).toBe(true);
        expect(isMountedPanelKey("meeting")).toBe(false);
        expect(isMountedPanelKey("knowledgeGraph")).toBe(false);
        expect(isMountedPanelKey("focusMode")).toBe(false);
    });
});
