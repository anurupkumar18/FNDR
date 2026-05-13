import { useCallback, useState } from "react";
import {
    findGraphPath,
    getFullGraph,
    getGodNodes,
    getGraphForProject,
    getNodeDetail,
    searchGraph,
    type InsightGraphSubgraph,
} from "@/shared/ipc/tauri";

/** Force-directed layout tick cap (product spec). */
export const GRAPH_SIM_MAX_TICKS = 300;

export function useGraph() {
    const [subgraph, setSubgraph] = useState<InsightGraphSubgraph | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const load = useCallback(async (opts: { mode: "full" | "project"; projectLabel?: string }) => {
        setLoading(true);
        setError(null);
        try {
            const data =
                opts.mode === "project" && opts.projectLabel?.trim()
                    ? await getGraphForProject(opts.projectLabel.trim())
                    : await getFullGraph();
            setSubgraph(data);
        } catch (e) {
            setError(e instanceof Error ? e.message : "Graph load failed");
            setSubgraph(null);
        } finally {
            setLoading(false);
        }
    }, []);

    const fetchNodeDetail = useCallback(async (id: string) => getNodeDetail(id), []);

    const fetchPath = useCallback(
        async (from: string, to: string) => findGraphPath(from, to),
        []
    );

    const fetchGodNodes = useCallback(async (k: number) => getGodNodes(k), []);

    const runSemanticSearch = useCallback(
        async (queryEmbedding: number[], k: number) => searchGraph(queryEmbedding, k),
        []
    );

    return {
        subgraph,
        loading,
        error,
        load,
        fetchNodeDetail,
        fetchPath,
        fetchGodNodes,
        runSemanticSearch,
    };
}
