import { useEffect, useRef, useState } from "react";
import { filesSearch, type FileHit } from "@/shared/ipc/files";

/** The meaning search waits for typing to pause; word matches show at once. */
const PAUSE_MS = 250;

/**
 * "words": instant word matches while typing. "meaning": the full search.
 * "words_only": the full search ran on words alone (no model yet).
 */
export type FileSearchPhase = "words" | "meaning" | "words_only";

/**
 * Files search beside a memory search, the way DigUp answers: word matches
 * on every keystroke, then the full search once typing pauses. Its own
 * sequence, and quiet failure: Files search is optional, so an error or a
 * service that is off leaves no files and never an error in the surface.
 */
export function useFileSearch(query: string, enabled: boolean, limit: number) {
    const [hits, setHits] = useState<FileHit[]>([]);
    const [strong, setStrong] = useState(0);
    const [searching, setSearching] = useState(false);
    const [phase, setPhase] = useState<FileSearchPhase | null>(null);
    const seq = useRef(0);

    useEffect(() => {
        const trimmed = query.trim();
        const current = ++seq.current;
        let final = false;
        if (!enabled || !trimmed) {
            setHits([]);
            setStrong(0);
            setSearching(false);
            setPhase(null);
            return;
        }
        setSearching(true);
        filesSearch(trimmed, undefined, limit, true)
            .then((result) => {
                if (seq.current !== current || final) return;
                setHits(result.hits);
                setStrong(result.strong);
                setPhase("words");
            })
            .catch(() => undefined);
        const timer = window.setTimeout(() => {
            filesSearch(trimmed, undefined, limit)
                .then((result) => {
                    if (seq.current !== current) return;
                    final = true;
                    setHits(result.hits);
                    setStrong(result.strong);
                    setPhase(result.meaning ? "meaning" : "words_only");
                    setSearching(false);
                })
                .catch(() => {
                    if (seq.current !== current) return;
                    final = true;
                    setHits([]);
                    setStrong(0);
                    setPhase(null);
                    setSearching(false);
                });
        }, PAUSE_MS);
        return () => window.clearTimeout(timer);
    }, [query, enabled, limit]);

    return { hits, strong, searching, phase };
}
