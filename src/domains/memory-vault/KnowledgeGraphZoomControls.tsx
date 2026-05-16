import type { RefObject } from "react";
import type { KnowledgeGraphCanvasHandle } from "./KnowledgeGraphCanvas";

export interface KnowledgeGraphZoomControlsProps {
    handle: RefObject<KnowledgeGraphCanvasHandle | null>;
}

export function KnowledgeGraphZoomControls({ handle }: KnowledgeGraphZoomControlsProps) {
    return (
        <div className="kg-zoom-controls" aria-label="Graph zoom controls">
            <button
                type="button"
                className="kg-zoom-btn"
                onClick={() => handle.current?.zoomIn()}
                aria-label="Zoom in"
                title="Zoom in (+)"
            >
                +
            </button>
            <button
                type="button"
                className="kg-zoom-btn"
                onClick={() => handle.current?.zoomOut()}
                aria-label="Zoom out"
                title="Zoom out (−)"
            >
                −
            </button>
            <button
                type="button"
                className="kg-zoom-btn"
                onClick={() => handle.current?.fit()}
                aria-label="Fit to graph"
                title="Fit to graph (f)"
            >
                ⊕
            </button>
            <button
                type="button"
                className="kg-zoom-btn"
                onClick={() => handle.current?.reset()}
                aria-label="Reset zoom"
                title="Reset (0)"
            >
                ⌂
            </button>
        </div>
    );
}
