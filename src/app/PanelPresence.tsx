import { useState, type ReactNode } from "react";
import { motion, useReducedMotion, type Variants } from "framer-motion";
import { motionTokens } from "@/shared/motion";

/**
 * Full-screen panels materialize: scale, blur, and opacity move together so
 * the surface reads as arriving rather than cross-fading. Exit retraces the
 * same path. `filter` and `transform` are cleared once settled, because a
 * lingering `filter` would become the backdrop root for the panel's own
 * backdrop-filter glass.
 */
const materialize: Variants = {
    open: {
        opacity: 1,
        scale: 1,
        filter: "blur(0px)",
        transitionEnd: { filter: "none" },
    },
    closed: {
        opacity: 0,
        scale: 0.985,
        filter: "blur(8px)",
    },
};

const crossFade: Variants = {
    open: { opacity: 1 },
    closed: { opacity: 0 },
};

interface PanelPresenceProps {
    open: boolean;
    /** Receives whether the panel should render; stays true through its exit. */
    children: (present: boolean) => ReactNode;
}

/**
 * Keeps a panel mounted and rendered through its exit so open and close are
 * symmetric. Reopening mid-exit re-targets the same spring from the panel's
 * on-screen state instead of restarting. During exit the layer is inert, so
 * focus and clicks already belong to whatever is underneath.
 */
export function PanelPresence({ open, children }: PanelPresenceProps) {
    const reducedMotion = useReducedMotion() ?? false;
    const [present, setPresent] = useState(open);
    if (open && !present) setPresent(true);

    return (
        <motion.div
            className="panel-presence"
            initial="closed"
            animate={open ? "open" : "closed"}
            variants={reducedMotion ? crossFade : materialize}
            transition={reducedMotion ? { duration: 0.15 } : motionTokens.spring.gentle}
            onAnimationComplete={() => {
                if (!open) setPresent(false);
            }}
            style={{ pointerEvents: open ? "auto" : "none" }}
            {...(open ? {} : { inert: "" })}
        >
            {children(present)}
        </motion.div>
    );
}
