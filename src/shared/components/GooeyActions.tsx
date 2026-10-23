import type { ReactNode } from "react";
import { Liquid } from "liquid-gooey";
import { useReducedMotionSafe } from "@/shared/motion/useReducedMotionSafe";
import "./GooeyActions.css";

/**
 * The one place that decides how FNDR's liquid action pairs look: goo softness,
 * edge tightness, spring and theme colors. Callers pass buttons, never numbers.
 */
export const GOOEY = {
    blur: 5,
    contrast: 18,
    transition: "bouncy",
    fill: "var(--gooey-fill)",
    shadow: "var(--gooey-shadow)",
} as const;

interface GooeyActionsProps {
    /** The control that is always there (Speak). */
    primary: ReactNode;
    /** The control that melts out of `primary` while `secondaryVisible` (Submit). */
    secondary: ReactNode;
    secondaryVisible: boolean;
    /** Width of the secondary control plus the gap, in px: how far the primary slides to make room. */
    slotPx: number;
    className?: string;
}

/**
 * Two buttons on one liquid surface. Under reduced motion there is no goo: the
 * secondary control is simply mounted or unmounted.
 */
export function GooeyActions({ primary, secondary, secondaryVisible, slotPx, className }: GooeyActionsProps) {
    const { reduced } = useReducedMotionSafe();
    const classes = ["gooey-actions", className].filter(Boolean).join(" ");

    if (reduced) {
        return (
            <div className={`${classes} gooey-actions--flat`} data-gooey="off">
                {primary}
                {secondaryVisible ? secondary : null}
            </div>
        );
    }

    // The collapsed control stays mounted so it can swell back out; keep it out of reach until then.
    const parked = secondaryVisible ? {} : { "aria-hidden": true, inert: "" };

    return (
        <Liquid
            className={classes}
            data-gooey="on"
            blur={GOOEY.blur}
            contrast={GOOEY.contrast}
            fill={GOOEY.fill}
            shadow={GOOEY.shadow}
        >
            <Liquid.Item
                className="gooey-actions__item gooey-actions__item--primary"
                x={secondaryVisible ? 0 : slotPx}
                transition={GOOEY.transition}
            >
                {primary}
            </Liquid.Item>
            <Liquid.Item
                className="gooey-actions__item"
                x={secondaryVisible ? 0 : -slotPx / 2}
                scale={secondaryVisible ? 1 : 0.4}
                transition={GOOEY.transition}
            >
                <div className="gooey-actions__parked" {...parked}>
                    {secondary}
                </div>
            </Liquid.Item>
        </Liquid>
    );
}
