import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { motion, useReducedMotion } from "framer-motion";
import { Liquid } from "liquid-gooey";
import { motionTokens } from "@/shared/motion";
import "./SegmentedControl.css";

export interface SegmentOption<T extends string> {
    value: T;
    label: ReactNode;
    /** Accessible name when the visible label carries extra content (counts). */
    ariaLabel?: string;
}

interface SegmentedControlProps<T extends string> {
    options: readonly SegmentOption<T>[];
    value: T;
    onChange: (value: T) => void;
    ariaLabel: string;
    className?: string;
}

/** Inset gap between the track edge and the thumb, matching the CSS padding. */
const TRACK_PADDING_PX = 3;

/**
 * macOS-style segmented control whose selection is a liquid thumb: it springs
 * to the chosen segment and the liquid surface stretches with it, so the
 * selection flows instead of jumping. Segments share the width equally so
 * only the thumb's x changes.
 */
export function SegmentedControl<T extends string>({
    options,
    value,
    onChange,
    ariaLabel,
    className,
}: SegmentedControlProps<T>) {
    const trackRef = useRef<HTMLDivElement>(null);
    const [segmentWidth, setSegmentWidth] = useState(0);
    const reducedMotion = useReducedMotion() ?? false;
    const selectedIndex = Math.max(0, options.findIndex((option) => option.value === value));

    useLayoutEffect(() => {
        const track = trackRef.current;
        if (!track) return;
        const measure = () =>
            setSegmentWidth((track.clientWidth - TRACK_PADDING_PX * 2) / Math.max(1, options.length));
        measure();
        const observer = new ResizeObserver(measure);
        observer.observe(track);
        return () => observer.disconnect();
    }, [options.length]);

    return (
        <div
            ref={trackRef}
            className={["fndr-segmented", className].filter(Boolean).join(" ")}
            role="group"
            aria-label={ariaLabel}
            style={{ gridTemplateColumns: `repeat(${options.length}, minmax(0, 1fr))` }}
        >
            {segmentWidth > 0 && (
                <div className="fndr-segmented-layer" aria-hidden="true">
                    <Liquid blur={5} fill="var(--seg-thumb)" shadow="var(--seg-thumb-shadow)">
                        {/* The move effect follows the thumb's rendered position, so the
                            thumb springs itself and the liquid stretches along behind it. */}
                        <Liquid.Item effect="move">
                            <motion.div
                                className="fndr-segmented-thumb"
                                style={{ width: segmentWidth }}
                                initial={false}
                                animate={{ x: selectedIndex * segmentWidth }}
                                transition={reducedMotion ? { duration: 0 } : motionTokens.spring.gentle}
                            />
                        </Liquid.Item>
                    </Liquid>
                </div>
            )}
            {options.map((option) => {
                const selected = option.value === value;
                return (
                    <button
                        key={option.value}
                        type="button"
                        className={`fndr-segment${selected ? " is-selected" : ""}`}
                        aria-pressed={selected}
                        aria-label={option.ariaLabel}
                        onClick={() => onChange(option.value)}
                    >
                        {option.label}
                    </button>
                );
            })}
        </div>
    );
}
