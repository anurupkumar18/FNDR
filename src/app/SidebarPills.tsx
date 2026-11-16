import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { motion, useReducedMotion, type Transition } from "framer-motion";

interface Frame {
    top: number;
    left: number;
    width: number;
    height: number;
}

interface SidebarHighlightProps {
    /** `data-sidebar-key` of the row to sit behind. */
    targetKey: string | null;
    visible: boolean;
    className: string;
    transition: Transition;
}

/**
 * One highlight per role (selection, hover) that travels between sidebar
 * rows on a spring. It measures the row's offset inside the drawer rather
 * than its on-screen box, so the drawer sliding or being dragged never reads
 * as the row moving and the highlight stays glued to it.
 */
export function SidebarHighlight({ targetKey, visible, className, transition }: SidebarHighlightProps) {
    const ref = useRef<HTMLSpanElement>(null);
    const reducedMotion = useReducedMotion() ?? false;
    const [frame, setFrame] = useState<Frame | null>(null);
    const placedRef = useRef(false);

    useLayoutEffect(() => {
        const container = ref.current?.parentElement;
        if (!container || !targetKey) return;
        const measure = () => {
            const row = container.querySelector<HTMLElement>(`[data-sidebar-key="${CSS.escape(targetKey)}"]`);
            if (!row) return;
            setFrame((previous) =>
                previous
                && previous.top === row.offsetTop
                && previous.left === row.offsetLeft
                && previous.width === row.offsetWidth
                && previous.height === row.offsetHeight
                    ? previous
                    : { top: row.offsetTop, left: row.offsetLeft, width: row.offsetWidth, height: row.offsetHeight },
            );
        };
        measure();
        if (typeof ResizeObserver === "undefined") return;
        const observer = new ResizeObserver(measure);
        observer.observe(container);
        return () => observer.disconnect();
    }, [targetKey]);

    // The first placement jumps into position; only later moves travel.
    // Marked after commit so StrictMode's double render cannot skip it.
    const instant = reducedMotion || !placedRef.current;
    useEffect(() => {
        if (frame) placedRef.current = true;
    }, [frame]);
    const shown = visible && Boolean(targetKey && frame);

    return (
        <motion.span
            ref={ref}
            className={className}
            aria-hidden="true"
            initial={false}
            animate={{
                x: frame?.left ?? 0,
                y: frame?.top ?? 0,
                width: frame?.width ?? 0,
                height: frame?.height ?? 0,
                opacity: shown ? 1 : 0,
            }}
            transition={instant ? { duration: 0 } : { default: transition, opacity: { duration: 0.16 } }}
        />
    );
}
