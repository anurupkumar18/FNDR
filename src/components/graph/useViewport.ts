import { useCallback, useMemo, useState, type MouseEvent, type WheelEvent } from "react";

interface ViewportConfig {
    minZoom?: number;
    maxZoom?: number;
    step?: number;
}

interface ViewportState {
    zoom: number;
    offsetX: number;
    offsetY: number;
    isDragging: boolean;
}

export interface ViewportController {
    zoom: number;
    offset: { x: number; y: number };
    isDragging: boolean;
    transform: string;
    zoomIn: () => void;
    zoomOut: () => void;
    reset: () => void;
    onWheel: (event: WheelEvent<Element>) => void;
    onMouseDown: (event: MouseEvent<Element>) => void;
    onMouseMove: (event: MouseEvent<Element>) => void;
    onMouseUp: () => void;
    onMouseLeave: () => void;
}

interface DragSnapshot {
    x: number;
    y: number;
    startOffsetX: number;
    startOffsetY: number;
}

function clamp(value: number, min: number, max: number): number {
    return Math.max(min, Math.min(max, value));
}

export function useViewport(config?: ViewportConfig): ViewportController {
    const minZoom = config?.minZoom ?? 0.55;
    const maxZoom = config?.maxZoom ?? 2.4;
    const step = config?.step ?? 0.12;

    const [state, setState] = useState<ViewportState>({
        zoom: 1,
        offsetX: 0,
        offsetY: 0,
        isDragging: false,
    });
    const [dragSnapshot, setDragSnapshot] = useState<DragSnapshot | null>(null);

    const setZoom = useCallback(
        (delta: number) => {
            setState((current) => ({
                ...current,
                zoom: clamp(Number((current.zoom + delta).toFixed(2)), minZoom, maxZoom),
            }));
        },
        [maxZoom, minZoom]
    );

    const zoomIn = useCallback(() => setZoom(step), [setZoom, step]);
    const zoomOut = useCallback(() => setZoom(-step), [setZoom, step]);

    const reset = useCallback(() => {
        setDragSnapshot(null);
        setState({ zoom: 1, offsetX: 0, offsetY: 0, isDragging: false });
    }, []);

    const onWheel = useCallback(
        (event: WheelEvent<Element>) => {
            event.preventDefault();
            const direction = event.deltaY > 0 ? -1 : 1;
            setZoom(direction * step * 0.75);
        },
        [setZoom, step]
    );

    const onMouseDown = useCallback((event: MouseEvent<Element>) => {
        setDragSnapshot({
            x: event.clientX,
            y: event.clientY,
            startOffsetX: state.offsetX,
            startOffsetY: state.offsetY,
        });
        setState((current) => ({ ...current, isDragging: true }));
    }, [state.offsetX, state.offsetY]);

    const onMouseMove = useCallback(
        (event: MouseEvent<Element>) => {
            if (!dragSnapshot) {
                return;
            }
            const dx = event.clientX - dragSnapshot.x;
            const dy = event.clientY - dragSnapshot.y;
            setState((current) => ({
                ...current,
                offsetX: dragSnapshot.startOffsetX + dx,
                offsetY: dragSnapshot.startOffsetY + dy,
                isDragging: true,
            }));
        },
        [dragSnapshot]
    );

    const stopDragging = useCallback(() => {
        setDragSnapshot(null);
        setState((current) => ({ ...current, isDragging: false }));
    }, []);

    const transform = useMemo(
        () => `translate(${state.offsetX} ${state.offsetY}) scale(${state.zoom})`,
        [state.offsetX, state.offsetY, state.zoom]
    );

    return {
        zoom: state.zoom,
        offset: { x: state.offsetX, y: state.offsetY },
        isDragging: state.isDragging,
        transform,
        zoomIn,
        zoomOut,
        reset,
        onWheel,
        onMouseDown,
        onMouseMove,
        onMouseUp: stopDragging,
        onMouseLeave: stopDragging,
    };
}
