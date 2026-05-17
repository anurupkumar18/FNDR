import { useEffect, useState } from "react";
import { fndrGetMemorySubgraph } from "../../shared/ipc/tauri";

interface Props {
    seedIds: string[];
    maxHops?: number;
    /** Defensive cap so a wide seed set can't blow up the canvas. */
    maxNodes?: number;
}

/**
 * Phase 5 — query-scoped subgraph view. Thin wrapper around the MCP
 * `fndr.get_memory_subgraph` descriptor; renders an empty-state until the
 * typed-graph persistence (Phase 6) lands.
 */
export function QueryScopedGraph({ seedIds, maxHops = 2, maxNodes = 25 }: Props) {
    const [summary, setSummary] = useState<{ node_count: number; edge_count: number } | null>(
        null,
    );
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        let cancelled = false;
        if (seedIds.length === 0) {
            setSummary({ node_count: 0, edge_count: 0 });
            return;
        }
        fndrGetMemorySubgraph(seedIds, maxHops)
            .then((sub) => {
                if (!cancelled) {
                    setSummary({
                        node_count: Math.min(sub.node_count, maxNodes),
                        edge_count: sub.edge_count,
                    });
                }
            })
            .catch((err) => {
                if (!cancelled) setError(String(err));
            });
        return () => {
            cancelled = true;
        };
    }, [seedIds.join(","), maxHops, maxNodes]);

    return (
        <div
            data-testid="fndr-query-graph"
            style={{
                padding: 16,
                background: "#FAF9F6",
                color: "#3E2723",
                borderRadius: 8,
                fontSize: 13,
            }}
        >
            <div style={{ opacity: 0.6, fontSize: 11, marginBottom: 4 }}>
                Query-scoped graph · {seedIds.length} seed{seedIds.length === 1 ? "" : "s"}
            </div>
            {error ? (
                <div style={{ color: "#E65100" }}>{error}</div>
            ) : summary ? (
                <div>
                    {summary.node_count} nodes · {summary.edge_count} edges
                    {summary.node_count === 0 && (
                        <div style={{ opacity: 0.6, marginTop: 4 }}>
                            Typed insight-graph persistence is still pending; nothing to draw yet.
                        </div>
                    )}
                </div>
            ) : (
                <div>Loading subgraph…</div>
            )}
        </div>
    );
}
