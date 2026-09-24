import { describe, expect, it } from "vitest";

import { notchClipPath, notchShapePath } from "./notchShape";

/** Every coordinate pair the path emits, in order. */
function points(path: string): Array<[number, number]> {
    return Array.from(path.matchAll(/(-?\d+(?:\.\d+)?) (-?\d+(?:\.\d+)?)/g)).map((match) => [
        Number(match[1]),
        Number(match[2]),
    ]);
}

describe("notch silhouette", () => {
    const closed = {
        width: 216,
        height: 38,
        topRadius: 8,
        bottomRadius: 12,
        style: "notch" as const,
    };

    it("flares out to both top edges of the window", () => {
        const path = notchShapePath(closed);
        expect(path.startsWith("M 0 0")).toBe(true);
        // The last drawn point before the close is the far top edge, so the
        // silhouette merges with the menu bar instead of floating below it.
        expect(path).toContain(`${closed.width} 0`);
        expect(path.endsWith("Z")).toBe(true);
    });

    it("keeps every point inside the box it is drawn in", () => {
        for (const style of ["notch", "pill"] as const) {
            const path = notchShapePath({ ...closed, style, width: 560, height: 400 });
            for (const [x, y] of points(path)) {
                expect(Number.isFinite(x) && Number.isFinite(y)).toBe(true);
                expect(x).toBeGreaterThanOrEqual(0);
                expect(x).toBeLessThanOrEqual(560);
                expect(y).toBeGreaterThanOrEqual(0);
                expect(y).toBeLessThanOrEqual(400);
            }
        }
    });

    it("clamps radii that exceed the silhouette they are drawn in", () => {
        // Mid-collapse the panel is briefly much smaller than the open radii.
        const path = notchShapePath({
            width: 30,
            height: 10,
            topRadius: 16,
            bottomRadius: 60,
            style: "notch",
        });
        for (const [x, y] of points(path)) {
            expect(Number.isFinite(x) && Number.isFinite(y)).toBe(true);
            expect(y).toBeLessThanOrEqual(10);
            expect(x).toBeLessThanOrEqual(30);
        }
    });

    it("drops the corner sweep when a radius animates to zero", () => {
        const square = notchShapePath({
            width: 200,
            height: 60,
            topRadius: 0,
            bottomRadius: 0,
            style: "pill",
        });
        // Four corners plus the closing point, no sampled curve between them.
        expect(points(square)).toHaveLength(5);
    });

    it("draws the pill as a closed capsule at half-height radii", () => {
        const capsule = notchShapePath({
            width: 132,
            height: 28,
            topRadius: 14,
            bottomRadius: 14,
            style: "pill",
        });
        const drawn = points(capsule);
        expect(drawn[0]).toEqual([14, 0]);
        // Ends where it started, so the clip path has no seam.
        expect(drawn[drawn.length - 1]).toEqual([14, 0]);
    });

    it("wraps the path for `clip-path`", () => {
        expect(notchClipPath(closed)).toMatch(/^path\("M 0 0 .*Z"\)$/);
    });
});
