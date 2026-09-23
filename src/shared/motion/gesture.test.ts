import { describe, expect, it } from "vitest";
import { nearestSnapPoint, project, rubberband } from "./gesture";

describe("project", () => {
    it("throws a flick well past its release point", () => {
        expect(project(1000)).toBeCloseTo(499, 0);
        expect(project(-1000)).toBeCloseTo(-499, 0);
        expect(project(0)).toBe(0);
    });

    it("lands sooner with a lower deceleration rate", () => {
        expect(Math.abs(project(1000, 0.99))).toBeLessThan(Math.abs(project(1000)));
    });
});

describe("rubberband", () => {
    it("resists progressively and never exceeds the dimension", () => {
        const small = rubberband(20, 244);
        const large = rubberband(2000, 244);
        expect(small).toBeLessThan(20);
        expect(large).toBeLessThan(244);
        expect(large / 2000).toBeLessThan(small / 20);
    });

    it("keeps the overshoot direction", () => {
        expect(rubberband(-50, 244)).toBeLessThan(0);
    });
});

describe("nearestSnapPoint", () => {
    it("picks the snap point closest to the projected rest position", () => {
        expect(nearestSnapPoint(-200, [-244, 0])).toBe(-244);
        expect(nearestSnapPoint(-40, [-244, 0])).toBe(0);
    });
});
