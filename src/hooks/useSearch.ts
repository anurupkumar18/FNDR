import { useEffect, useRef, useState } from "react";
import { search, SearchResult } from "../api/tauri";

export function useSearch(
    query: string,
    timeFilter: string | null,
    appFilter: string | null
) {
    const [results, setResults] = useState<SearchResult[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const requestIdRef = useRef(0);

    useEffect(() => {
        const trimmedQuery = query.trim();
        const requestId = ++requestIdRef.current;

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
                const res = await search(
                    trimmedQuery,
                    timeFilter ?? undefined,
                    appFilter ?? undefined,
                    10
                );
                if (cancelled || requestId !== requestIdRef.current) {
                    return;
                }
                setResults(res.slice(0, 10)); // Top-k results
            } catch (e) {
                if (cancelled || requestId !== requestIdRef.current) {
                    return;
                }
                setError(e instanceof Error ? e.message : "Search failed");
                setResults([]);
            } finally {
                if (!cancelled && requestId === requestIdRef.current) {
                    setIsLoading(false);
                }
            }
        }, 300);

        return () => {
            cancelled = true;
            clearTimeout(timer);
        };
    }, [query, timeFilter, appFilter]);

    return { results, isLoading, error };
}
