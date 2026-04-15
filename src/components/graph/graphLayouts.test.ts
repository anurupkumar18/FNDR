import { describe, expect, it } from "vitest";
import {
    buildClusterHullPath,
    buildConstellationLayout,
    buildTimelineWaveform,
    curvedConnectionPath,
    layoutFocusRings,
    layoutJourneyPath,
} from "./graphLayouts";
import { TimelineSegment } from "./graphTypes";

describe("graphLayouts", () => {
    it("creates curved connection paths", () => {
        const path = curvedConnectionPath({ x: 10, y: 10 }, { x: 100, y: 80 }, 0.2);
        expect(path.startsWith("M")).toBe(true);
        expect(path.includes("Q")).toBe(true);
    });

    it("builds a hull path for clustered points", () => {
        const hull = buildClusterHullPath([
            { x: 100, y: 100 },
            { x: 160, y: 110 },
            { x: 150, y: 170 },
            { x: 90, y: 150 },
        ]);

        expect(hull.length).toBeGreaterThan(0);
    });

    it("lays out focus rings around center", () => {
        const positions = layoutFocusRings("center", ["a", "b", "c"], ["d", "e"], 980, 430);
        expect(positions.get("center")?.ring).toBe(0);
        expect(positions.get("a")?.ring).toBe(1);
        expect(positions.get("d")?.ring).toBe(2);
    });

    it("builds timeline waveform path", () => {
        const segments: TimelineSegment[] = [
            {
                id: "s1",
                label: "exploration",
                memoryIds: ["m1"],
                startTs: 100,
                endTs: 130,
                intensity: 0.2,
                confidence: 0.6,
                pivotCount: 0,
            },
            {
                id: "s2",
                label: "implementation",
                memoryIds: ["m2"],
                startTs: 200,
                endTs: 260,
                intensity: 0.8,
                confidence: 0.7,
                pivotCount: 1,
            },
        ];

        const waveform = buildTimelineWaveform(segments, 980, 180);
        expect(waveform.points.length).toBe(2);
        expect(waveform.linePath.length).toBeGreaterThan(0);
        expect(waveform.areaPath.length).toBeGreaterThan(0);
    });

    it("lays out journey path with branch candidates", () => {
        const adjacency = new Map<string, Set<string>>([
            ["a", new Set(["b", "x"])],
            ["b", new Set(["a", "c", "y"])],
            ["c", new Set(["b", "z"])],
        ]);

        const layout = layoutJourneyPath(["a", "b", "c"], adjacency, 980, 250);
        expect(layout.points.length).toBeGreaterThanOrEqual(3);
        expect(layout.edges.length).toBeGreaterThanOrEqual(2);
        expect(layout.branchCandidatesByPathId.size).toBeGreaterThan(0);
    });

    it("builds constellation layout points by cluster", () => {
        const centers = new Map([
            ["alpha", { x: 200, y: 200 }],
            ["beta", { x: 700, y: 200 }],
        ]);

        const layout = buildConstellationLayout(
            [
                { id: "n1", cluster: "alpha", importance: 0.8, roleWeight: 1.2 },
                { id: "n2", cluster: "alpha", importance: 0.4, roleWeight: 1 },
                { id: "n3", cluster: "beta", importance: 0.7, roleWeight: 1.1 },
            ],
            centers
        );

        expect(layout.size).toBe(3);
        expect(layout.get("n1")?.x).toBeDefined();
    });
});
