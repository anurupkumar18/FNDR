import { describe, expect, it } from "vitest";
import { DEMO_COMMAND_IDS, isDemoCommand } from "./CommandPalette";

describe("demo command palette", () => {
    it("exposes exactly the alpha demo destinations and capture controls", () => {
        expect(DEMO_COMMAND_IDS).toEqual([
            "go-home",
            "memory-cards",
            "ask-fndr",
            "daily-summary",
            "stats",
            "todo",
            "wrapped",
            "hermes-agent",
            "screen-guide",
            "engine-metrics",
            "privacy-proof",
            "pause-capture",
            "resume-capture",
        ]);
        expect(isDemoCommand("engine-metrics")).toBe(true);
        expect(isDemoCommand("privacy-proof")).toBe(true);
        expect(isDemoCommand("local-context")).toBe(false);
        expect(isDemoCommand("automation")).toBe(false);
    });
});
