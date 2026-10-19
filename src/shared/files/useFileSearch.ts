import { useEffect, useRef, useState } from "react";
import { filesSearch, type FileHit } from "@/shared/ipc/files";

const DEBOUNCE_MS = 250;

/**
 * Files search beside a memory search: the same query, its own debounce and
 * sequence, and quiet failure. Files search is optional, so an error or a
 * service that is off leaves no files and never an error in the surface.
 */
export function useFileSearch(query: string, enabled: boolean, limit: number) {
    const [hits, setHits] = useState<FileHit[]>([]);
    const [strong, setStrong] = useState(0);
    const [searching, setSearching] = useState(false);
    const seq = useRef(0);

    useEffect(() => {
        const trimmed = query.trim();
        const current = ++seq.current;
        if (!enabled || !trimmed) {
            setHits([]);
            setStrong(0);
            setSearching(false);
            return;
        }
        setSearching(true);
        const timer = window.setTimeout(() => {
            filesSearch(trimmed, undefined, limit)
                .then((result) => {
                    if (seq.current !== current) return;
                    setHits(result.hits);
                    setStrong(result.strong);
                    setSearching(false);
                })
                .catch(() => {
                    if (seq.current !== current) return;
                    setHits([]);
                    setStrong(0);
                    setSearching(false);
                });
        }, DEBOUNCE_MS);
        return () => window.clearTimeout(timer);
    }, [query, enabled, limit]);

    return { hits, strong, searching };
}
