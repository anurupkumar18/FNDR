import { useReducedMotion } from "framer-motion";
import { ThinkingOrb, type OrbState } from "thinking-orbs";
import { useActiveCinematicPalette } from "@/shared/hooks/useActiveCinematicPalette";

/** Orb sizes are tuned per scale: 20 sits inline with text, 32 fills a
 *  loading state, 64 stands in for the assistant while it works. */
const ORB_SIZE = { sm: 20, md: 32, lg: 64 } as const;

interface ThinkingIndicatorProps {
    /** What the work looks like: recall is `searching`, writing is `composing`. */
    state: OrbState;
    size?: keyof typeof ORB_SIZE;
    className?: string;
}

/** Decorative progress for model and retrieval work. Pair it with visible
 *  status text; the orb itself is hidden from assistive tech. Under reduced
 *  motion it holds a still frame instead of animating. */
export function ThinkingIndicator({ state, size = "md", className }: ThinkingIndicatorProps) {
    const { mode } = useActiveCinematicPalette();
    const reducedMotion = useReducedMotion() ?? false;

    return (
        <ThinkingOrb
            state={state}
            size={ORB_SIZE[size]}
            theme={mode}
            paused={reducedMotion}
            className={["thinking-indicator", className].filter(Boolean).join(" ")}
            aria-hidden="true"
        />
    );
}
