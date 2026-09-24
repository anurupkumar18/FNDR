/**
 * Every tunable of the notch HUD's silhouette, in one place.
 *
 * The panel wears one of two silhouettes per display: `notch` sits on a real
 * camera housing with inverted top corners flaring into the menu bar, `pill` is
 * a detached Dynamic-Island-style capsule for displays without one. Nothing
 * here knows about React — `notchShape.ts` turns these numbers into a path and
 * `NotchHud.tsx` animates between the three stages.
 *
 * The envelope constants (panel max size, shadow padding, hover bump, pill top
 * gap) are duplicated in `src-tauri/src/ipc/commands/notch.rs`, which sizes the
 * fixed window. `notchMetrics.test.ts` fails if the two drift apart.
 */

export type NotchStyle = "notch" | "pill";

/** Closed → hovered → opened. The panel only ever grows in that order. */
export type NotchStage = "closed" | "peek" | "open";

export interface NotchSize {
    width: number;
    height: number;
}

export const notchMetrics = {
    /** Collapsed pill on displays without a camera housing. */
    pillClosedWidth: 132,
    pillClosedHeight: 28,
    /** The pill is detached from the screen edge; a real notch never is. */
    pillTopGap: 6,

    /** Largest footprint the panel ever takes — the window is sized for this. */
    panelMaxWidth: 560,
    panelMaxHeight: 400,
    /** Margin around the panel so its CSS shadow isn't clipped by the window. */
    shadowPadding: 28,
    /** Cosmetic growth while hovered, reserved so it can't clip. */
    hoverBump: 6,

    /** Resting silhouette. The top radius doubles as the width of each flare. */
    closedTopRadius: 8,
    closedBottomRadius: 12,
    /** Peek: more rounded than resting, same ratio the open panel uses. */
    peekTopRadius: 12,
    peekBottomRadius: 22,
    openTopRadius: 16,
    openBottomRadius: 60,
    /** Uniform on all four corners, unlike the notch. */
    pillPeekCornerRadius: 17,
    pillOpenCornerRadius: 32,

    /**
     * Peek widens the notch by this much per side — a physical notch can't
     * change width, so the growth reads as real black flanking the cutout.
     */
    peekFlankWidth: 54,
    /** The cutout's height can't change either, so this appears below it. */
    peekHeightBump: 14,
    /** Pill peek is free to be whatever reads best; it has no hardware to match. */
    pillPeekWidth: 260,
    pillPeekHeight: 34,

    /** Open panel: width is fixed, height tracks whatever the panel is showing. */
    openWidth: 520,
    /** The input row alone — the floor the panel never shrinks below when open. */
    openHeaderHeight: 62,
    openRowHeight: 54,
    openFooterHeight: 34,
    /** Search rows shown while browsing. */
    openMaxRows: 5,
    /** Memories cited under an answer. */
    answerCardLimit: 3,

    /** Content padding inside the open panel, past the flare. */
    openContentInset: 14,

    // Springs, as SwiftUI response/damping-fraction pairs (see `spring()`).
    // Opening overshoots slightly; closing is critically damped.
    openSpringResponse: 0.45,
    openSpringDamping: 0.7,
    closeSpringResponse: 0.45,
    closeSpringDamping: 1.0,
    /** Peek is a smaller move, so it gets a quicker spring of its own. */
    peekSpringResponse: 0.3,
    peekSpringDamping: 0.82,

    /**
     * Grace period before an un-hovered panel collapses, so crossing a gap in
     * the silhouette (or the pointer poll's own 16ms granularity) doesn't
     * flicker it shut.
     */
    hoverExitGraceMs: 260,
    /** How often the drawn panel's frame is reported back to the window. */
    hitRectThrottleMs: 60,
} as const;

/** Fallback geometry until the backend reports the real display metrics. */
export const fallbackGeometry = {
    closed_width: notchMetrics.pillClosedWidth,
    closed_height: notchMetrics.pillClosedHeight,
    is_physical_notch: false,
    window_width: notchMetrics.panelMaxWidth + notchMetrics.shadowPadding * 2 + notchMetrics.hoverBump,
    window_height:
        notchMetrics.panelMaxHeight +
        notchMetrics.shadowPadding +
        notchMetrics.hoverBump +
        notchMetrics.pillTopGap,
    screen_width: 1440,
    screen_height: 900,
};

/**
 * A shape drawn in a box of width `w` has a visible body of `w - 2 * topRadius`
 * in notch style, because the flare eats the outer edges. Zero for the pill,
 * which has no flare.
 */
export function flareAllowance(topRadius: number, style: NotchStyle): number {
    return style === "notch" ? topRadius * 2 : 0;
}

export function styleFor(isPhysicalNotch: boolean): NotchStyle {
    return isPhysicalNotch ? "notch" : "pill";
}

export function topRadiusFor(stage: NotchStage, style: NotchStyle, closedHeight: number): number {
    if (style === "pill") {
        switch (stage) {
            case "closed":
                return closedHeight / 2;
            case "peek":
                return notchMetrics.pillPeekCornerRadius;
            case "open":
                return notchMetrics.pillOpenCornerRadius;
        }
    }
    switch (stage) {
        case "closed":
            return notchMetrics.closedTopRadius;
        case "peek":
            return notchMetrics.peekTopRadius;
        case "open":
            return notchMetrics.openTopRadius;
    }
}

export function bottomRadiusFor(stage: NotchStage, style: NotchStyle, closedHeight: number): number {
    if (style === "pill") {
        return topRadiusFor(stage, style, closedHeight);
    }
    switch (stage) {
        case "closed":
            return notchMetrics.closedBottomRadius;
        case "peek":
            return notchMetrics.peekBottomRadius;
        case "open":
            return notchMetrics.openBottomRadius;
    }
}

/**
 * Height of the open panel for the content currently inside it. An answer is
 * as long as it is, so the panel is measured rather than computed from a row
 * count — clamped to the input row at one end and the window envelope at the
 * other, past which the transcript scrolls.
 */
export function openPanelHeight(contentHeight: number): number {
    return Math.min(
        Math.max(contentHeight, notchMetrics.openHeaderHeight),
        notchMetrics.panelMaxHeight
    );
}

/**
 * The silhouette's body size for a stage — before the flare allowance and the
 * hover bump, which `NotchHud` adds on top.
 */
export function bodySizeFor(
    stage: NotchStage,
    style: NotchStyle,
    closed: NotchSize,
    openContentHeight: number
): NotchSize {
    switch (stage) {
        case "closed":
            return closed;
        case "peek":
            return style === "notch"
                ? {
                      width: closed.width + notchMetrics.peekFlankWidth * 2,
                      height: closed.height + notchMetrics.peekHeightBump,
                  }
                : { width: notchMetrics.pillPeekWidth, height: notchMetrics.pillPeekHeight };
        case "open":
            return { width: notchMetrics.openWidth, height: openPanelHeight(openContentHeight) };
    }
}

/**
 * SwiftUI's `.spring(response:dampingFraction:)` in Framer Motion's terms, so
 * the numbers above read the same as the macOS original.
 */
export function spring(response: number, dampingFraction: number) {
    return {
        type: "spring" as const,
        stiffness: ((2 * Math.PI) / response) ** 2,
        damping: (4 * Math.PI * dampingFraction) / response,
        mass: 1,
        restDelta: 0.01,
    };
}

export function stageSpring(from: NotchStage, to: NotchStage) {
    if (to === "peek" || from === "peek") {
        return spring(notchMetrics.peekSpringResponse, notchMetrics.peekSpringDamping);
    }
    const entering = to === "open";
    return entering
        ? spring(notchMetrics.openSpringResponse, notchMetrics.openSpringDamping)
        : spring(notchMetrics.closeSpringResponse, notchMetrics.closeSpringDamping);
}
