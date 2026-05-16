import { describe, it, expect } from "vitest";
import { colorForCommunity, assignCommunityColors } from "../graphPalette";

describe("graphPalette", () => {
    it("returns the muted accent fallback for null community", () => {
        expect(colorForCommunity(null)).toBe("var(--cp-accent-muted)");
    });

    it("returns a deterministic HSL string for a given community id", () => {
        const a = colorForCommunity(0);
        const b = colorForCommunity(0);
        expect(a).toBe(b);
        expect(a).toMatch(/^hsl\(\d+ \d+% \d+%\)$/);
    });

    it("hue varies between distinct community ids", () => {
        const a = colorForCommunity(0);
        const b = colorForCommunity(1);
        expect(a).not.toBe(b);
    });

    it("assignCommunityColors maps every supplied community id", () => {
        const map = assignCommunityColors([0, 1, 2]);
        expect(Object.keys(map).sort()).toEqual(["0", "1", "2"]);
        for (const v of Object.values(map)) {
            expect(v).toMatch(/^hsl\(\d+ \d+% \d+%\)$/);
        }
    });

    it("assignCommunityColors is stable across calls with the same input", () => {
        const a = assignCommunityColors([3, 7, 11]);
        const b = assignCommunityColors([3, 7, 11]);
        expect(a).toEqual(b);
    });
});
