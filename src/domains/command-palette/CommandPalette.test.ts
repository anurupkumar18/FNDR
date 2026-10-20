import { describe, expect, it } from "vitest";
import { DEMO_COMMAND_IDS, isDemoCommand } from "./CommandPalette";

describe("demo command palette", () => {
    it("exposes exactly the alpha demo destinations and capture controls", () => {
        expect(DEMO_COMMAND_IDS).toEqual([
            "go-home",
            "ask-fndr",
            "memory-cards",
            "daily-brief",
            "daily-outputs",
            "trust-settings",
            "screen-guide",
            "hermes-agent",
            "stats",
            "todo",
            "wrapped",
            "privacy-proof",
            "engine-metrics",
            "pause-capture",
            "resume-capture",
        ]);
        expect(isDemoCommand("engine-metrics")).toBe(true);
        expect(isDemoCommand("trust-settings")).toBe(true);
        expect(isDemoCommand("local-context")).toBe(false);
        expect(isDemoCommand("automation")).toBe(false);
    });
});
