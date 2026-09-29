import { useEffect, useRef, useState } from "react";
import { searchMemoryCards, type MemoryCard } from "@/shared/ipc/tauri";
import { SEARCH_LIMITS } from "@/shared/utils/config";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
} from "@/shared/activity/activityTrace";

function getAdaptiveDebounceMs(query: string): number {
    if (!query.trim()) {
        return 0;
    }
    return SEARCH_LIMITS.typingDebounceMs;
}

function getAdaptiveTimeoutMs(query: string, attempt: number): number {
    const words = query.trim().split(/\s+/).filter(Boolean).length;
    const extraForLength = Math.min(
        SEARCH_LIMITS.timeoutBonusCapMs,
        query.length * SEARCH_LIMITS.perCharBonusMs
    );
    const extraForWords = Math.min(
        SEARCH_LIMITS.timeoutBonusCapMs,
        words * SEARCH_LIMITS.perWordBonusMs
    );
    const retryBonus = attempt > 0 ? SEARCH_LIMITS.retryBonusMs : 0;
    return SEARCH_LIMITS.baseTimeoutMs + extraForLength + extraForWords + retryBonus;
}

export function useSearch(query: string, timeFilter: string | null, appFilter: string | null) {
    const [results, setResults] = useState<MemoryCard[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [activityTrace, setActivityTrace] = useState<ActivityTraceSnapshot | null>(null);
    const requestIdRef = useRef(0);

    useEffect(() => {
        const trimmedQuery = query.trim();
        const requestId = ++requestIdRef.current;
        const debounceMs = getAdaptiveDebounceMs(trimmedQuery);

        if (!trimmedQuery) {
            setResults([]);
            setError(null);
            setIsLoading(false);
            setActivityTrace(null);
            return;
        }

        let cancelled = false;
        const requestedAtMs = Date.now();
        let trace = beginActivityTrace({
            id: `search-${requestId}`,
            title: "Memory search activity",
            startedAtMs: requestedAtMs,
        });
        trace = recordActivityStep(trace, {
            id: "queued",
            label: debounceMs > 0 ? "Waiting for typing to settle" : "Search requested",
            actor: "FNDR search",
            status: debounceMs > 0 ? "waiting" : "running",
            evidence: "frontend-event",
            atMs: requestedAtMs,
        });
        setActivityTrace(trace);
        setIsLoading(true);
        setError(null);

        const timer = setTimeout(async () => {
            const retrievalStartedAtMs = Date.now();
            setActivityTrace((current) => {
                if (!current || current.id !== `search-${requestId}`) return current;
                const withSettledInput = recordActivityStep(current, {
                    id: "queued",
                    label: debounceMs > 0 ? "Typing settled" : "Search requested",
                    actor: "FNDR search",
                    status: "completed",
                    evidence: "frontend-event",
                    atMs: retrievalStartedAtMs,
                    durationMs: Math.max(0, retrievalStartedAtMs - requestedAtMs),
                });
                return recordActivityStep(withSettledInput, {
                    id: "retrieval",
                    label: "Requesting memory search",
                    actor: "FNDR search service",
                    status: "running",
                    evidence: "ipc-boundary",
                    atMs: retrievalStartedAtMs,
                });
            });
            let timeoutHandle: ReturnType<typeof setTimeout> | null = null;
            try {
                const timeoutMs = getAdaptiveTimeoutMs(trimmedQuery, 0);
                const timeoutPromise = new Promise<never>((_, reject) => {
                    timeoutHandle = setTimeout(() => reject(new Error("Search timed out")), timeoutMs);
                });

                const searchPromise = searchMemoryCards(
                    trimmedQuery,
                    timeFilter ?? undefined,
                    appFilter ?? undefined,
                    SEARCH_LIMITS.resultLimit
                );

                const res = await Promise.race([searchPromise, timeoutPromise]);

                if (cancelled || requestId !== requestIdRef.current) {
                    return;
                }
                const nextResults = res.slice(0, SEARCH_LIMITS.resultLimit);
                const completedAtMs = Date.now();
                const routes = Array.from(new Set(
                    nextResults.flatMap((card) => card.matched_routes ?? []),
                )).slice(0, 5);
                const count = nextResults.length;
                const detail = [
                    `${count} ${count === 1 ? "memory" : "memories"}`,
                    routes.length > 0 ? routes.join(" + ") : null,
                ].filter(Boolean).join(" · ");
                setResults(nextResults);
                setActivityTrace((current) => {
                    if (!current || current.id !== `search-${requestId}`) return current;
                    const withRetrieval = recordActivityStep(current, {
                        id: "retrieval",
                        label: "Memory search request completed",
                        actor: "FNDR search service",
                        status: "completed",
                        evidence: "ipc-boundary",
                        atMs: completedAtMs,
                        durationMs: Math.max(0, completedAtMs - retrievalStartedAtMs),
                    });
                    return recordActivityStep(withRetrieval, {
                        id: "result",
                        label: "Search completed",
                        actor: "FNDR search service",
                        status: "completed",
                        evidence: "result-metadata",
                        atMs: completedAtMs,
                        detail,
                    });
                });
            } catch (e) {
                if (cancelled || requestId !== requestIdRef.current) {
                    return;
                }
                const errorMessage = e instanceof Error ? e.message : "Search failed";
                const timedOut = errorMessage.toLowerCase().includes("timed out");
                setError(timedOut
                    ? "Search timed out. Try a shorter query or remove filters."
                    : errorMessage);
                setResults([]);
                const failedAtMs = Date.now();
                setActivityTrace((current) => {
                    if (!current || current.id !== `search-${requestId}`) return current;
                    return recordActivityStep(current, {
                        id: "retrieval",
                        label: timedOut ? "Search timed out" : "Search failed",
                        actor: "FNDR search service",
                        status: "failed",
                        evidence: "ipc-boundary",
                        atMs: failedAtMs,
                        durationMs: Math.max(0, failedAtMs - retrievalStartedAtMs),
                        detail: timedOut ? "Client timeout" : "Backend request failed",
                    });
                });
            } finally {
                if (timeoutHandle !== null) clearTimeout(timeoutHandle);
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

    return { results, isLoading, error, activityTrace };
}
