/**
 * The notch silhouette as an SVG path, ported from the macOS original.
 *
 * Inverted (concave) top corners flare the panel outward into the menu bar so
 * it merges with the physical cutout; that flare has no CSS equivalent, so it
 * is a hand-built quadratic curve per side. The bottom corners are continuous
 * ("squircle") corners: the web has no access to Apple's real `.continuous`
 * curve, so they are sampled from a superellipse of exponent
 * `CONTINUOUS_CORNER_EXPONENT`, which tracks it closely enough to read as
 * system UI beside the real notch.
 *
 * Paths are generated per frame from animating width/height/radii, so
 * everything here is pure and allocation-light.
 */

import type { NotchStyle } from "./notchMetrics";

/** n = 2 is a plain circular corner; ~5 matches Apple's continuous corner. */
const CONTINUOUS_CORNER_EXPONENT = 5;
/** Segments per corner. 18 is smooth at the 60pt radius the open panel uses. */
const CORNER_STEPS = 18;

export interface NotchPathOptions {
    width: number;
    height: number;
    topRadius: number;
    bottomRadius: number;
    style: NotchStyle;
}

function round(value: number): number {
    return Math.round(value * 1000) / 1000;
}

function point(x: number, y: number): string {
    return `${round(x)} ${round(y)}`;
}

/**
 * One quadrant of a superellipse, as `L` segments. `(cx, cy)` is the centre of
 * the corner's radius box; `sx`/`sy` steer which quadrant is drawn, and the
 * sweep always starts on the edge that `sx` points along.
 */
function continuousCorner(
    cx: number,
    cy: number,
    radius: number,
    sx: number,
    sy: number,
    swap: boolean
): string {
    if (radius <= 0) {
        return "";
    }
    const exponent = 2 / CONTINUOUS_CORNER_EXPONENT;
    const segments: string[] = [];
    for (let step = 1; step <= CORNER_STEPS; step += 1) {
        const theta = (Math.PI / 2) * (step / CORNER_STEPS);
        const u = Math.cos(theta) ** exponent;
        const v = Math.sin(theta) ** exponent;
        const dx = swap ? v : u;
        const dy = swap ? u : v;
        segments.push(`L ${point(cx + sx * radius * dx, cy + sy * radius * dy)}`);
    }
    return segments.join(" ");
}

/**
 * The notch: flares out to the screen edge at the top, continuous corners at
 * the bottom. The body is inset by `topRadius` per side to make room for the
 * flare — callers add that allowance to `width` (see `flareAllowance`).
 */
function notchPath({ width, height, topRadius, bottomRadius }: NotchPathOptions): string {
    // Mid-collapse the panel is briefly smaller than its own open radii, so
    // both are clamped against the box they are drawn in — otherwise the flare
    // dives past the bottom edge and the silhouette self-intersects.
    const top = Math.max(0, Math.min(topRadius, width / 2, height / 2));
    const bottom = Math.max(
        0,
        Math.min(bottomRadius, Math.max(0, width / 2 - top), Math.max(0, height - top))
    );

    const left = top;
    const right = width - top;

    return [
        `M ${point(0, 0)}`,
        // Top-left: flare outward to meet the screen edge.
        `Q ${point(top, 0)} ${point(top, top)}`,
        `L ${point(left, height - bottom)}`,
        continuousCorner(left + bottom, height - bottom, bottom, -1, 1, false),
        `L ${point(right - bottom, height)}`,
        continuousCorner(right - bottom, height - bottom, bottom, 1, 1, true),
        `L ${point(right, top)}`,
        // Top-right flare.
        `Q ${point(right, 0)} ${point(width, 0)}`,
        "Z",
    ]
        .filter(Boolean)
        .join(" ");
}

/** The detached island: a rounded rectangle with continuous corners all round. */
function pillPath({ width, height, topRadius, bottomRadius }: NotchPathOptions): string {
    const limit = Math.min(width, height) / 2;
    const top = Math.max(0, Math.min(topRadius, limit));
    const bottom = Math.max(0, Math.min(bottomRadius, limit));

    return [
        `M ${point(top, 0)}`,
        `L ${point(width - top, 0)}`,
        continuousCorner(width - top, top, top, 1, -1, true),
        `L ${point(width, height - bottom)}`,
        continuousCorner(width - bottom, height - bottom, bottom, 1, 1, false),
        `L ${point(bottom, height)}`,
        continuousCorner(bottom, height - bottom, bottom, -1, 1, true),
        `L ${point(0, top)}`,
        continuousCorner(top, top, top, -1, -1, false),
        "Z",
    ]
        .filter(Boolean)
        .join(" ");
}

export function notchShapePath(options: NotchPathOptions): string {
    return options.style === "notch" ? notchPath(options) : pillPath(options);
}

/** Ready for `style.clipPath` — regenerated every frame while animating. */
export function notchClipPath(options: NotchPathOptions): string {
    return `path("${notchShapePath(options)}")`;
}
