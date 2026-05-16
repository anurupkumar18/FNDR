import { useEffect, useMemo, useState } from "react";
import type { InsightGraphEdge, InsightGraphNode } from "@/shared/ipc/tauri";
import { buildGraphView } from "./graph/graphDataBuilder";
import type { GraphNodeView } from "./graph/types";
import { KnowledgeGraphCanvas } from "./KnowledgeGraphCanvas";
import { KnowledgeGraphSidePanel } from "./KnowledgeGraphSidePanel";
import { GRAPH_SIM_MAX_TICKS } from "./useGraph";
import "./KnowledgeGraph.css";

export interface KnowledgeGraphProps {
    nodes: InsightGraphNode[];
    edges: InsightGraphEdge[];
    height?: number;
    onNodeClick?: (node: InsightGraphNode) => void;
    selectedNodeId?: string | null;
    pathNodeIds?: readonly string[] | null;
    highlightNodeIds?: readonly string[] | null;
    /** Optional Louvain map from caller (back-compat with existing MemoryCardsPanel callsites). */
    louvainByNodeId?: Record<string, number> | null;
    maxSimulationTicks?: number;
    /** Hierarchical layout is no longer supported; this prop is accepted for back-compat and ignored. */
    layoutMode?: "hierarchical" | "force";
    /** When true, mount the vertical right-side memory card. Default: true. */
    showSidePanel?: boolean;
}

export function KnowledgeGraph({
    nodes,
    edges,
    height = 480,
    onNodeClick,
    selectedNodeId = null,
    pathNodeIds = null,
    highlightNodeIds = null,
    louvainByNodeId = null,
    maxSimulationTicks = GRAPH_SIM_MAX_TICKS,
    showSidePanel = true,
}: KnowledgeGraphProps) {
    const view = useMemo(
        () =>
            buildGraphView({
                nodes,
                edges,
                louvain: louvainByNodeId ?? undefined,
            }),
        [nodes, edges, louvainByNodeId],
    );

    const nodeIndex = useMemo(() => new Map(view.nodes.map((n) => [n.id, n])), [view.nodes]);

    const [hoveredId, setHoveredId] = useState<string | null>(null);
    const [internalSelectedId, setInternalSelectedId] = useState<string | null>(null);
    const effectiveSelectedId = selectedNodeId ?? internalSelectedId;

    // Drop internal selection when an external selection change wipes it.
    useEffect(() => {
        if (selectedNodeId !== undefined && selectedNodeId !== null) {
            setInternalSelectedId(null);
        }
    }, [selectedNodeId]);

    const neighborhoodIds = useMemo(() => {
        const focus = hoveredId ?? effectiveSelectedId;
        if (!focus) return new Set<string>();
        const out = new Set<string>([focus]);
        for (const e of view.edges) {
            if (e.sourceId === focus) out.add(e.targetId);
            if (e.targetId === focus) out.add(e.sourceId);
        }
        return out;
    }, [hoveredId, effectiveSelectedId, view.edges]);

    const pathSet = useMemo(() => new Set(pathNodeIds ?? []), [pathNodeIds]);
    const hubSet = useMemo(() => new Set(highlightNodeIds ?? []), [highlightNodeIds]);

    const selectedNode = effectiveSelectedId ? nodeIndex.get(effectiveSelectedId) ?? null : null;
    const incidentEdges = useMemo(() => {
        if (!effectiveSelectedId) return [];
        return view.edges.filter(
            (e) => e.sourceId === effectiveSelectedId || e.targetId === effectiveSelectedId,
        );
    }, [view.edges, effectiveSelectedId]);

    const handleSelect = (n: GraphNodeView) => {
        setInternalSelectedId(n.id);
        onNodeClick?.(n.raw);
    };

    return (
        <div
            className={`knowledge-graph-shell${showSidePanel ? "" : " knowledge-graph-shell-bare"}`}
            data-empty={view.nodes.length === 0 ? "true" : undefined}
            style={{ height }}
        >
            <div className="knowledge-graph-canvas-wrap film-grain">
                <KnowledgeGraphCanvas
                    view={view}
                    width={0}
                    height={height}
                    selectedId={effectiveSelectedId}
                    hoveredId={hoveredId}
                    neighborhoodIds={neighborhoodIds}
                    pathNodeIds={pathSet}
                    hubNodeIds={hubSet}
                    maxTicks={maxSimulationTicks}
                    onHover={setHoveredId}
                    onSelect={handleSelect}
                />
            </div>
            {showSidePanel && (
                <KnowledgeGraphSidePanel
                    selected={selectedNode}
                    incidentEdges={incidentEdges}
                    nodeIndex={nodeIndex}
                    onSelectNode={handleSelect}
                />
            )}
        </div>
    );
}
