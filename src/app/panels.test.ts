import { describe, expect, it } from "vitest";
import {
    LAB_PANEL_KEYS,
    MOUNTED_PANEL_KEYS,
    PRIMARY_DESTINATION_PANEL_KEYS,
    isMountedPanelKey,
} from "./panels";

describe("foreground panel registry", () => {
    it("keeps the five-destination panels and Labs panels mounted", () => {
        expect(MOUNTED_PANEL_KEYS).toEqual([
            "ask",
            "memoryCards",
            "dailySummary",
            "stats",
            "todo",
            "wrapped",
            "privacyProof",
            "screenGuide",
            "agent",
            "engineMetrics",
        ]);
        expect(PRIMARY_DESTINATION_PANEL_KEYS).toEqual([
            "ask",
            "memoryCards",
            "dailySummary",
            "stats",
            "todo",
            "wrapped",
            "privacyProof",
        ]);
        expect(LAB_PANEL_KEYS).toEqual(["screenGuide", "agent", "engineMetrics"]);
        expect(isMountedPanelKey("memoryCards")).toBe(true);
        expect(isMountedPanelKey("meeting")).toBe(false);
        expect(isMountedPanelKey("knowledgeGraph")).toBe(false);
        expect(isMountedPanelKey("focusMode")).toBe(false);
    });
});
