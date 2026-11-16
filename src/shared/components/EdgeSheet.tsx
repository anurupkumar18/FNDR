import { useCallback, useLayoutEffect, useRef, useState, type PointerEvent, type ReactNode, type Ref } from "react";
import { animate, motion, useMotionValue, useReducedMotion, useTransform } from "framer-motion";
import { motionTokens, nearestSnapPoint, project, rubberband } from "@/shared/motion";

/** Movement before a press becomes a drag, so taps on controls still click. */
const DRAG_HYSTERESIS_PX = 10;

/** Presses that start here keep their own horizontal gestures. */
const OWN_GESTURE_SELECTOR = 'input, textarea, select, pre, code, [contenteditable="true"], [role="slider"], [data-sheet-no-drag]';

interface EdgeSheetProps {
    isOpen: boolean;
    onClose: () => void;
    sheetRef?: Ref<HTMLElement>;
    id?: string;
    className?: string;
    backdropClassName?: string;
    ariaLabelledBy?: string;
    children: ReactNode;
}

interface DragState {
    pointerId: number;
    startX: number;
    startY: number;
    grabOffset: number;
    active: boolean;
}

/**
 * A modal sheet on the right edge. Its position is one motion value, so
 * opening, closing, a drag and a flick all start from wherever the sheet is
 * and can interrupt each other. Dragging right follows the pointer, dragging
 * past the open edge rubber-bands, and release snaps to where the gesture is
 * heading. It stays mounted, inert, through its exit so close mirrors open.
 */
export function EdgeSheet({ isOpen, onClose, sheetRef, id, className, backdropClassName, ariaLabelledBy, children }: EdgeSheetProps) {
    const localRef = useRef<HTMLElement | null>(null);
    const widthRef = useRef(440);
    const dragRef = useRef<DragState | null>(null);
    const suppressClickRef = useRef(false);
    const reducedMotion = useReducedMotion() ?? false;
    const x = useMotionValue(widthRef.current);
    const backdropOpacity = useTransform(x, (value) => Math.min(1, Math.max(0, 1 - value / widthRef.current)));
    const [present, setPresent] = useState(isOpen);
    if (isOpen && !present) setPresent(true);

    const setRefs = useCallback(
        (node: HTMLElement | null) => {
            localRef.current = node;
            if (typeof sheetRef === "function") sheetRef(node);
            else if (sheetRef) (sheetRef as { current: HTMLElement | null }).current = node;
        },
        [sheetRef],
    );

    const settle = useCallback(
        (open: boolean, velocity?: number) => {
            const target = open ? 0 : widthRef.current;
            const transition = reducedMotion
                ? { duration: 0 }
                : velocity === undefined
                    ? motionTokens.spring.snappy
                    : { ...motionTokens.spring.momentum, velocity };
            animate(x, target, transition).then(() => {
                if (!open && x.get() === target) setPresent(false);
            });
        },
        [reducedMotion, x],
    );

    useLayoutEffect(() => {
        if (!present) return;
        const width = localRef.current?.offsetWidth;
        if (width) widthRef.current = width;
        if (dragRef.current?.active) return;
        settle(isOpen);
    }, [isOpen, present, settle]);

    if (!present) return null;

    const handlePointerDown = (event: PointerEvent<HTMLElement>) => {
        suppressClickRef.current = false;
        if (!isOpen || event.button !== 0) return;
        if ((event.target as Element).closest(OWN_GESTURE_SELECTOR)) return;
        dragRef.current = {
            pointerId: event.pointerId,
            startX: event.clientX,
            startY: event.clientY,
            grabOffset: x.get() - event.clientX,
            active: false,
        };
    };

    const handlePointerMove = (event: PointerEvent<HTMLElement>) => {
        const drag = dragRef.current;
        if (!drag || drag.pointerId !== event.pointerId) return;

        if (!drag.active) {
            const dx = event.clientX - drag.startX;
            const dy = event.clientY - drag.startY;
            if (Math.abs(dx) < DRAG_HYSTERESIS_PX) return;
            if (Math.abs(dy) > Math.abs(dx)) {
                dragRef.current = null;
                return;
            }
            drag.active = true;
            x.stop();
            drag.grabOffset = x.get() - event.clientX;
            event.currentTarget.setPointerCapture(event.pointerId);
        }

        const width = widthRef.current;
        const raw = event.clientX + drag.grabOffset;
        x.set(raw < 0 ? rubberband(raw, width) : Math.min(raw, width));
    };

    const handlePointerEnd = (event: PointerEvent<HTMLElement>) => {
        const drag = dragRef.current;
        dragRef.current = null;
        if (!drag?.active || drag.pointerId !== event.pointerId) return;

        suppressClickRef.current = true;
        const velocity = x.getVelocity();
        const target = nearestSnapPoint(x.get() + project(velocity), [0, widthRef.current]);
        const open = target === 0;
        settle(open, velocity);
        if (!open) onClose();
    };

    return (
        <>
            <motion.div
                className={backdropClassName}
                onClick={onClose}
                aria-hidden="true"
                style={{ opacity: backdropOpacity, pointerEvents: isOpen ? "auto" : "none" }}
            />
            <motion.aside
                ref={setRefs}
                id={id}
                className={className}
                role="dialog"
                aria-modal={isOpen ? "true" : undefined}
                aria-labelledby={ariaLabelledBy}
                {...(isOpen ? {} : { inert: "" })}
                style={{ x, pointerEvents: isOpen ? "auto" : "none" }}
                onPointerDown={handlePointerDown}
                onPointerMove={handlePointerMove}
                onPointerUp={handlePointerEnd}
                onPointerCancel={handlePointerEnd}
                onClickCapture={(event) => {
                    if (!suppressClickRef.current) return;
                    suppressClickRef.current = false;
                    event.preventDefault();
                    event.stopPropagation();
                }}
            >
                {children}
            </motion.aside>
        </>
    );
}
