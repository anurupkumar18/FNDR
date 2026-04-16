import { useEffect, useRef, useState } from "react";
import { MemoryCard, searchMemoryCards } from "../api/tauri";

const BASE_SEARCH_TIMEOUT_MS = 8000;

function getAdaptiveDebounceMs(query: string): number {
    const trimmedLength = query.trim().length;
    return Math.min(1100, 280 + Math.floor(trimmedLength * 6));
}

function getAdaptiveTimeoutMs(query: string, attempt: number): number {
    const words = query.trim().split(/\s+/).filter(Boolean).length;
    const extraForLength = Math.min(6000, query.length * 20);
    const extraForWords = Math.min(6000, words * 450);
    const retryBonus = attempt > 0 ? 4000 : 0;
    return BASE_SEARCH_TIMEOUT_MS + extraForLength + extraForWords + retryBonus;
}

function isTimeoutError(error: unknown): boolean {
    if (!(error instanceof Error)) {
        return false;
    }
    return error.message.toLowerCase().includes("timed out");
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
                let res: MemoryCard[] = [];

                for (let attempt = 0; attempt < 2; attempt += 1) {
                    try {
                        const timeoutMs = getAdaptiveTimeoutMs(trimmedQuery, attempt);
                        const timeoutPromise = new Promise<never>((_, reject) => {
                            setTimeout(() => reject(new Error("Search timed out")), timeoutMs);
                        });
                        res = await Promise.race([
                            searchMemoryCards(
                                trimmedQuery,
                                timeFilter ?? undefined,
                                appFilter ?? undefined,
                                10
                            ),
                            timeoutPromise,
                        ]);
                        break;
                    } catch (attemptError) {
                        const shouldRetry = attempt === 0 && isTimeoutError(attemptError);
                        if (!shouldRetry) {
                            throw attemptError;
                        }
                    }
                }

                if (cancelled || requestId !== requestIdRef.current) {
                    return;
                }
                setResults(res.slice(0, 10)); // Top-k results
            } catch (e) {
                if (cancelled || requestId !== requestIdRef.current) {
                    return;
                }
                const errorMessage = e instanceof Error ? e.message : "Search failed";
                setError(
                    errorMessage.toLowerCase().includes("timed out")
                        ? "Search is taking longer than expected. Pause typing for a moment and try again."
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
