import { useState, useEffect } from "react";
import { search, SearchResult } from "../api/tauri";

export function useSearch(
    query: string,
    timeFilter: string | null,
    appFilter: string | null
) {
    const [results, setResults] = useState<SearchResult[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        // Debounce search
        const timer = setTimeout(async () => {
            if (!query.trim()) {
                setResults([]);
                return;
            }

            setIsLoading(true);
            setError(null);

            try {
                const res = await search(query, timeFilter, appFilter);
                setResults(res);
            } catch (e) {
                setError(e instanceof Error ? e.message : "Search failed");
                setResults([]);
            } finally {
                setIsLoading(false);
            }
        }, 300);

        return () => clearTimeout(timer);
    }, [query, timeFilter, appFilter]);

    return { results, isLoading, error };
}
