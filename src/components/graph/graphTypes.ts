import { GraphEdgeData, GraphNodeData } from "../../api/tauri";

export type GraphNodeType = "MemoryChunk" | "Entity" | "Task" | "Url";
export type ViewMode = "constellation" | "timeline" | "cluster" | "focus" | "journey";
export type ClusterLens = "app" | "memoryType" | "domain" | "session";

export type ActivityFacet =
    | "all"
    | "productive"
    | "unproductive"
    | "learning"
    | "communication"
    | "research"
    | "neutral";

export type ClusterRole = "dominant" | "secondary" | "bridge" | "peripheral";
export type PathQuality = "direct" | "weak" | "cross-context" | "semantic";
export type HopRole = "anchor" | "bridge" | "pivot" | "endpoint";
export type FocusMode = "structural" | "semantic" | "causal";

export interface TypedGraphNode extends GraphNodeData {
    node_type: GraphNodeType;
}

export interface Point {
    x: number;
    y: number;
}

export interface PositionedNode extends Point {
    id: string;
}

export interface NodeInsight {
    id: string;
    importance: number;
    recency: number;
    weightedDegree: number;
    bridgeStrength: number;
    labelPriority: number;
    isBridge: boolean;
}

export interface EdgeInsight {
    id: string;
    source: string;
    target: string;
    prominence: number;
    reason: string;
    isBridge: boolean;
}

export interface ClusterInsight {
    key: string;
    label: string;
    nodeIds: string[];
    memoryIds: string[];
    typeCounts: Record<GraphNodeType, number>;
    role: ClusterRole;
    strengthScore: number;
    bridgeScore: number;
    internalDensity: number;
    crossConnectivity: number;
    topTags: string[];
    summary: string;
    exemplarMemoryIds: string[];
    subthemes: Array<{ key: string; count: number }>;
    sparkline: number[];
}

export interface TimelineSegment {
    id: string;
    label: "exploration" | "implementation" | "refinement" | "validation" | "drift" | "return";
    memoryIds: string[];
    startTs: number;
    endTs: number;
    intensity: number;
    confidence: number;
    pivotCount: number;
}

export interface TimelineMemoryGroup {
    id: string;
    memoryIds: string[];
    representativeId: string;
    isCollapsedByDefault: boolean;
    similarityReason: string;
}

export interface JourneyHop {
    nodeId: string;
    index: number;
    role: HopRole;
    reason: string;
    quality: PathQuality;
    isEssential: boolean;
    phaseGroup: number;
}

export interface FocusNeighbor {
    nodeId: string;
    score: number;
    degree: 1 | 2;
    reasons: string[];
    edgeType?: string | null;
    viaNodeId?: string | null;
}

export interface FocusNeighborhood {
    centerId: string;
    direct: FocusNeighbor[];
    secondary: FocusNeighbor[];
    displayNodeIds: Set<string>;
}

export interface JourneySemantics {
    hops: JourneyHop[];
    summary: string;
    overallQuality: PathQuality;
}

export interface ClusterConnection {
    pair: string;
    leftKey: string;
    rightKey: string;
    count: number;
    reasons: string[];
    primaryReason: string;
    strength: number;
}

export interface TimelinePivot {
    memoryId: string;
    reason: string;
}

export interface TimelineInsight {
    segments: TimelineSegment[];
    pivots: TimelinePivot[];
    groupedMemories: TimelineMemoryGroup[];
    densityByBucket: number[];
}

export interface NodeTypeMeta {
    label: string;
    color: string;
    short: string;
    softColor: string;
}

export interface InteractionState {
    selectedNodeId?: string | null;
    hoveredNodeId?: string | null;
    highlightedIds?: Set<string>;
}

export interface EdgePairIndex {
    [pair: string]: GraphEdgeData;
}
