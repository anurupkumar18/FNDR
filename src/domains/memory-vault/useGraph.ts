import { useCallback, useRef, useState } from "react";
import {
    findGraphPath,
    getFullGraph,
    getGodNodes,
    getGraphForProject,
    getNodeDetail,
    searchGraph,
    type InsightGraphSubgraph,
} from "@/shared/ipc/tauri";
import { graphCache } from "./graph/graphCache";

/** Force-directed layout tick cap (product spec). */
export const GRAPH_SIM_MAX_TICKS = 300;

interface LoadOpts {
    mode: "full" | "project";
    projectLabel?: string;
}

function cacheKey(opts: LoadOpts): string {
    if (opts.mode === "project" && opts.projectLabel?.trim()) {
        return `project:${opts.projectLabel.trim()}`;
    }
    return "full";
}

export function useGraph() {
    const [subgraph, setSubgraph] = useState<InsightGraphSubgraph | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const lastOptsRef = useRef<LoadOpts | null>(null);

    const load = useCallback(async (opts: LoadOpts) => {
        lastOptsRef.current = opts;
        setLoading(true);
        setError(null);
        try {
            const key = cacheKey(opts);
            const data = await graphCache.get(key, () =>
                opts.mode === "project" && opts.projectLabel?.trim()
                    ? getGraphForProject(opts.projectLabel.trim())
                    : getFullGraph(),
            );
            setSubgraph(data);
        } catch (e) {
            setError(e instanceof Error ? e.message : "Graph load failed");
            setSubgraph(null);
        } finally {
            setLoading(false);
        }
    }, []);

    const refresh = useCallback(async () => {
        const opts = lastOptsRef.current;
        if (!opts) return;
        graphCache.invalidate(cacheKey(opts));
        await load(opts);
    }, [load]);

    const fetchNodeDetail = useCallback(async (id: string) => getNodeDetail(id), []);

    const fetchPath = useCallback(
        async (from: string, to: string) => findGraphPath(from, to),
        [],
    );

    const fetchGodNodes = useCallback(async (k: number) => getGodNodes(k), []);

    const runSemanticSearch = useCallback(
        async (queryEmbedding: number[], k: number) => searchGraph(queryEmbedding, k),
        [],
    );

    return {
        subgraph,
        loading,
        error,
        load,
        refresh,
        fetchNodeDetail,
        fetchPath,
        fetchGodNodes,
        runSemanticSearch,
    };
}
