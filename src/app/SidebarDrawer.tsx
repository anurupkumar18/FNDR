import { useCallback, useLayoutEffect, useRef, useState, type PointerEvent, type ReactNode } from "react";
import { animate, motion, useMotionValue, useReducedMotion, useTransform } from "framer-motion";
import { motionTokens, nearestSnapPoint, project, rubberband } from "@/shared/motion";

/** Movement before a press becomes a drag, so taps on nav rows still click. */
const DRAG_HYSTERESIS_PX = 10;

interface SidebarDrawerProps {
    isOpen: boolean;
    onClose: () => void;
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
 * Primary navigation drawer. Position lives in one motion value, so the
 * toggle button, a drag, and a flick all animate from wherever the drawer
 * currently is and can interrupt each other mid-flight.
 */
export function SidebarDrawer({ isOpen, onClose, children }: SidebarDrawerProps) {
    const navRef = useRef<HTMLElement>(null);
    const widthRef = useRef(244);
    const dragRef = useRef<DragState | null>(null);
    const suppressClickRef = useRef(false);
    const reducedMotion = useReducedMotion() ?? false;
    const x = useMotionValue(-widthRef.current);
    const scrimOpacity = useTransform(x, (value) => Math.min(1, Math.max(0, 1 + value / widthRef.current)));
    const [isSettledClosed, setIsSettledClosed] = useState(!isOpen);

    const settle = useCallback(
        (open: boolean, velocity?: number) => {
            const target = open ? 0 : -widthRef.current;
            if (open) setIsSettledClosed(false);
            const transition = reducedMotion
                ? { duration: 0 }
                : velocity === undefined
                    ? motionTokens.spring.snappy
                    : { ...motionTokens.spring.momentum, velocity };
            animate(x, target, transition).then(() => {
                if (!open && x.get() === target) setIsSettledClosed(true);
            });
        },
        [reducedMotion, x],
    );

    useLayoutEffect(() => {
        const width = navRef.current?.offsetWidth;
        if (width) widthRef.current = width;
        if (dragRef.current?.active) return;
        settle(isOpen);
    }, [isOpen, settle]);

    const handlePointerDown = (event: PointerEvent<HTMLElement>) => {
        suppressClickRef.current = false;
        if (!isOpen || event.button !== 0) return;
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
        x.set(raw > 0 ? rubberband(raw, width) : Math.max(raw, -width));
    };

    const handlePointerEnd = (event: PointerEvent<HTMLElement>) => {
        const drag = dragRef.current;
        dragRef.current = null;
        if (!drag?.active || drag.pointerId !== event.pointerId) return;

        suppressClickRef.current = true;
        const velocity = x.getVelocity();
        const target = nearestSnapPoint(x.get() + project(velocity), [-widthRef.current, 0]);
        const open = target === 0;
        settle(open, velocity);
        if (!open) onClose();
    };

    const hidden = !isOpen && isSettledClosed;

    return (
        <>
            <motion.button
                type="button"
                className="sidebar-scrim"
                onClick={onClose}
                aria-label="Close sidebar overlay"
                aria-hidden={!isOpen}
                tabIndex={isOpen ? 0 : -1}
                style={{
                    opacity: scrimOpacity,
                    pointerEvents: isOpen ? "auto" : "none",
                    display: hidden ? "none" : undefined,
                }}
            />
            <motion.nav
                ref={navRef}
                id="primary-navigation"
                className={`left-sidebar ${isOpen ? "open" : ""}`}
                aria-label="Primary navigation"
                {...(isOpen ? {} : { inert: "" })}
                style={{ x, visibility: hidden ? "hidden" : "visible" }}
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
            </motion.nav>
        </>
    );
}
