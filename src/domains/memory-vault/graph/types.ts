import type { InsightGraphEdge, InsightGraphNode } from "@/shared/ipc/tauri";

/** Display-ready view of an insight node. */
export interface GraphNodeView {
    id: string;
    raw: InsightGraphNode;
    /** Truncated label for tooltip/side-panel use. NEVER drawn on the node circle. */
    label: string;
    nodeType: string;
    /** Louvain community id, when available. */
    community: number | null;
    /** Number of edges incident to this node (computed). */
    connectionCount: number;
    /** Pixel radius for canvas render. */
    size: number;
    /** Computed 0..1 importance for sort/legend tier. */
    importance: number;
}

/** Display-ready view of an insight edge. */
export interface GraphEdgeView {
    id: string;
    raw: InsightGraphEdge;
    sourceId: string;
    targetId: string;
    edgeType: string;
    confidence: number;
    /** Render bucket; drives stroke style. */
    kind: EdgeKind;
    reasons: RelationshipReason[];
}

export type EdgeKind = "structural" | "semantic" | "reference" | "temporal" | "conflict";

export interface GraphCluster {
    id: number;
    nodeIds: string[];
    /** Optional human label (server-supplied cluster_0_name when available). */
    label: string | null;
}

export interface RelationshipReason {
    text: string;
    tone: "neutral" | "amber" | "alarm";
}

export interface GraphLegendRow {
    kind: "community" | "node-type" | "edge-kind" | "encoding";
    label: string;
    swatch: LegendSwatch;
}

export interface LegendSwatch {
    color: string;
    shape: "dot" | "ring" | "dash" | "dot-dot" | "arrow";
}

/** Result of graphDataBuilder.build(). */
export interface GraphView {
    nodes: GraphNodeView[];
    edges: GraphEdgeView[];
    clusters: GraphCluster[];
    /** Map from community id to display color, deterministic per session. */
    communityColors: Record<number, string>;
}
