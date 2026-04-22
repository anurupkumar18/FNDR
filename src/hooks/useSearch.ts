import { useEffect, useRef, useState } from "react";
import { MemoryCard, searchMemoryCards } from "../api/tauri";

const BASE_SEARCH_TIMEOUT_MS = 6_000;
const SEARCH_RESULT_LIMIT = 12;

function getAdaptiveDebounceMs(query: string): number {
    if (!query.trim()) {
        return 0;
    }
    return 40;
}

function getAdaptiveTimeoutMs(query: string, attempt: number): number {
    const words = query.trim().split(/\s+/).filter(Boolean).length;
    const extraForLength = Math.min(6000, query.length * 20);
    const extraForWords = Math.min(6000, words * 450);
    const retryBonus = attempt > 0 ? 4000 : 0;
    return BASE_SEARCH_TIMEOUT_MS + extraForLength + extraForWords + retryBonus;
}

export function useSearch(
    query: string,
    timeFilter: string | null,
    appFilter: string | null
) {
    const [results, setResults] = useState<MemoryCard[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const requestIdRef = useRef(0);

    useEffect(() => {
        const trimmedQuery = query.trim();
        const requestId = ++requestIdRef.current;
        const debounceMs = getAdaptiveDebounceMs(trimmedQuery);

        if (!trimmedQuery) {
            setResults([]);
            setError(null);
            setIsLoading(false);
            return;
        }

        let cancelled = false;
        setIsLoading(true);
        setError(null);

        // Debounce search
        const timer = setTimeout(async () => {
            try {
                const timeoutMs = getAdaptiveTimeoutMs(trimmedQuery, 0);
                const timeoutPromise = new Promise<never>((_, reject) => {
                    setTimeout(() => reject(new Error("Search timed out")), timeoutMs);
                });
                const res = await Promise.race([
                    searchMemoryCards(
                        trimmedQuery,
                        timeFilter ?? undefined,
                        appFilter ?? undefined,
                        SEARCH_RESULT_LIMIT
                    ),
                    timeoutPromise,
                ]);

                if (cancelled || requestId !== requestIdRef.current) {
                    return;
                }
                setResults(res.slice(0, SEARCH_RESULT_LIMIT)); // Top-k results
            } catch (e) {
                if (cancelled || requestId !== requestIdRef.current) {
                    return;
                }
                const errorMessage = e instanceof Error ? e.message : "Search failed";
                setError(
                    errorMessage.toLowerCase().includes("timed out")
                        ? "Search timed out. Try a shorter query or remove filters."
                        : errorMessage
                );
                setResults([]);
            } finally {
                if (!cancelled && requestId === requestIdRef.current) {
                    setIsLoading(false);
                }
            }
        }, debounceMs);

        return () => {
            cancelled = true;
            clearTimeout(timer);
        };
    }, [query, timeFilter, appFilter]);

    return { results, isLoading, error };
}
