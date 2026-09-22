import { describe, expect, it } from "vitest";

// The backend module itself, as text — the one source of truth for the window
// envelope these metrics have to fit inside.
import notchRustSource from "../../../src-tauri/src/ipc/commands/notch.rs?raw";

import {
    bodySizeFor,
    bottomRadiusFor,
    flareAllowance,
    notchMetrics,
    openPanelHeight,
    spring,
    stageSpring,
    topRadiusFor,
} from "./notchMetrics";

/**
 * The window is sized in Rust and the panel is drawn in TypeScript; if the two
 * disagree the panel either clips against the window edge or leaves a dead
 * transparent band over the menu bar.
 */
function rustConstant(name: string): number {
    const match = notchRustSource.match(new RegExp(`const ${name}: f64 = (-?[\\d.]+);`));
    if (!match) {
        throw new Error(`${name} is no longer declared in notch.rs`);
    }
    return Number(match[1]);
}

describe("notch metrics", () => {
    it("matches the window envelope the backend builds", () => {
        expect(notchMetrics.panelMaxWidth).toBe(rustConstant("PANEL_MAX_WIDTH"));
        expect(notchMetrics.panelMaxHeight).toBe(rustConstant("PANEL_MAX_HEIGHT"));
        expect(notchMetrics.shadowPadding).toBe(rustConstant("SHADOW_PADDING"));
        expect(notchMetrics.hoverBump).toBe(rustConstant("HOVER_BUMP"));
        expect(notchMetrics.pillTopGap).toBe(rustConstant("PILL_TOP_GAP"));
        expect(notchMetrics.pillClosedWidth).toBe(rustConstant("PILL_CLOSED_WIDTH"));
        expect(notchMetrics.pillClosedHeight).toBe(rustConstant("PILL_CLOSED_HEIGHT"));
    });

    it("never draws a panel larger than that envelope", () => {
        const closed = { width: 220, height: 38 };
        for (const style of ["notch", "pill"] as const) {
            for (const stage of ["closed", "peek", "open"] as const) {
                // Content taller than the window can ever show, to exercise the clamp.
                const body = bodySizeFor(stage, style, closed, notchMetrics.panelMaxHeight * 2);
                const topRadius = topRadiusFor(stage, style, closed.height);
                // The hover bump only applies before the panel opens.
                const bump = stage === "open" ? 0 : notchMetrics.hoverBump;
                const width = body.width + flareAllowance(topRadius, style) + bump;
                expect(width).toBeLessThanOrEqual(notchMetrics.panelMaxWidth);
                expect(body.height + bump).toBeLessThanOrEqual(notchMetrics.panelMaxHeight);
            }
        }
    });

    it("tracks the content it is showing, between the input row and the cap", () => {
        // An empty panel is still the height of its input row.
        expect(openPanelHeight(0)).toBe(notchMetrics.openHeaderHeight);
        expect(openPanelHeight(notchMetrics.openHeaderHeight + 120)).toBe(
            notchMetrics.openHeaderHeight + 120
        );
        // A long answer scrolls instead of pushing the panel off the window.
        expect(openPanelHeight(notchMetrics.panelMaxHeight * 3)).toBe(notchMetrics.panelMaxHeight);
    });

    it("keeps the pill a true capsule while collapsed", () => {
        expect(topRadiusFor("closed", "pill", 28)).toBe(14);
        expect(bottomRadiusFor("closed", "pill", 28)).toBe(14);
    });

    it("only the notch pays a flare allowance", () => {
        expect(flareAllowance(16, "notch")).toBe(32);
        expect(flareAllowance(16, "pill")).toBe(0);
    });

    it("converts SwiftUI spring parameters to an equivalent Framer spring", () => {
        // response 0.45s / damping 1.0 is critically damped: c = 2*sqrt(k*m).
        const critical = spring(0.45, 1);
        expect(critical.damping).toBeCloseTo(2 * Math.sqrt(critical.stiffness), 6);
        // Opening deliberately overshoots, closing does not.
        expect(stageSpring("closed", "open").damping).toBeLessThan(
            2 * Math.sqrt(stageSpring("closed", "open").stiffness)
        );
    });
});
