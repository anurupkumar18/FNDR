import { useEffect, useRef, useState } from "react";
import { filesThumbnails } from "@/shared/ipc/files";

/**
 * Quick Look's thumbnails for the shown rows, asked for in one batch per new
 * set of paths and kept for the life of the surface. A file Quick Look can't
 * draw is asked once and then shows its kind.
 */
export function useThumbnails(paths: string[], pixels?: number): Record<string, string> {
    const [thumbnails, setThumbnails] = useState<Record<string, string>>({});
    const asked = useRef(new Set<string>());
    const key = paths.join("\n");
    useEffect(() => {
        const wanted = paths.filter((path) => !asked.current.has(path));
        if (!wanted.length) return;
        wanted.forEach((path) => asked.current.add(path));
        // Kept even when the rows change before they arrive: the paths are asked only once.
        filesThumbnails(wanted, pixels)
            .then((found) => setThumbnails((current) => ({ ...current, ...found })))
            .catch(() => wanted.forEach((path) => asked.current.delete(path)));
    }, [key]);
    return thumbnails;
}
