import type React from "react";
import { filesDrag } from "@/shared/ipc/files";

/** How far the pointer moves with the button down before a press becomes a drag. */
const DRAG_DISTANCE = 4;

/** One pointer, so one press at a time; kept out of React so a re-render (hover selects the row) keeps it. */
let press: { path: string; x: number; y: number } | null = null;
let draggedPath: string | null = null;

/**
 * Mouse handlers that drag a found file out of FNDR (to Finder, Mail, a
 * chat), as DigUp's rows do. A press that becomes a drag never also opens
 * the file: the click after it is swallowed.
 */
export function dragOutHandlers(path: string, icon?: string) {
    return {
        draggable: false,
        onMouseDown: (event: React.MouseEvent) => {
            if (event.button !== 0) return;
            press = { path, x: event.clientX, y: event.clientY };
            draggedPath = null;
        },
        onMouseMove: (event: React.MouseEvent) => {
            if (!press || press.path !== path || (event.buttons & 1) === 0) return;
            if (Math.hypot(event.clientX - press.x, event.clientY - press.y) < DRAG_DISTANCE) return;
            press = null;
            draggedPath = path;
            void filesDrag(path, icon?.startsWith("data:image/png;base64,") ? icon : undefined).catch(() => undefined);
        },
        onMouseUp: () => {
            press = null;
        },
        onClickCapture: (event: React.MouseEvent) => {
            if (draggedPath === path) {
                draggedPath = null;
                event.stopPropagation();
                event.preventDefault();
            }
        },
    };
}
