import { describe, expect, it } from "vitest";
import { DEMO_COMMAND_IDS, isDemoCommand } from "./CommandPalette";

describe("demo command palette", () => {
    it("exposes only the demonstrated memory, context, metrics, and capture controls", () => {
        expect(DEMO_COMMAND_IDS).toEqual([
            "memory-cards",
            "engine-metrics",
            "local-context",
            "pause-capture",
            "resume-capture",
        ]);
        expect(isDemoCommand("memory-cards")).toBe(true);
        expect(isDemoCommand("screen-guide")).toBe(false);
        expect(isDemoCommand("automation")).toBe(false);
    });
});
