import { describe, expect, it } from "vitest";
import { GraphEdgeData } from "../../api/tauri";
import {
    buildAdjacency,
    buildClusterInsights,
    buildEdgePairMap,
    buildFocusNeighborhood,
    deriveJourneyHopSemantics,
    deriveTimelineSegments,
    rankRepresentativeMemories,
    scoreBridgeStrength,
    scoreNodeImportance,
} from "./graphInsights";
import { TypedGraphNode } from "./graphTypes";

function node(id: string, node_type: TypedGraphNode["node_type"], created_at: number, label: string, metadata: Record<string, unknown> = {}): TypedGraphNode {
    return {
        id,
        node_type,
        created_at,
        label,
        metadata,
    };
}

function edge(id: string, source: string, target: string, edge_type: string): GraphEdgeData {
    return {
        id,
        source,
        target,
        edge_type,
        timestamp: Date.now(),
        metadata: {},
    };
}

const nodes: TypedGraphNode[] = [
    node("m1", "MemoryChunk", 1000, "Implemented graph panel", { app_name: "VS Code", session_id: "s1", memory_type: "development" }),
    node("m2", "MemoryChunk", 1100, "Refined timeline cards", { app_name: "VS Code", session_id: "s1", memory_type: "development" }),
    node("m3", "MemoryChunk", 2200, "Reviewed docs in Chrome", { app_name: "Google Chrome", session_id: "s2", memory_type: "web", url: "https://example.com" }),
    node("t1", "Task", 1150, "Refactor graph modes", { source_app: "VS Code", task_type: "Todo" }),
    node("e1", "Entity", 900, "Session anchor", { entity_type: "session", session_id: "s1" }),
    node("u1", "Url", 2150, "https://example.com/graph", { host: "example.com" }),
];

const edges: GraphEdgeData[] = [
    edge("e-1", "m1", "m2", "PART_OF_SESSION"),
    edge("e-2", "t1", "m1", "REFERENCE_FOR_TASK"),
    edge("e-3", "m3", "u1", "OCCURRED_AT"),
    edge("e-4", "m1", "e1", "PART_OF_SESSION"),
    edge("e-5", "m2", "e1", "PART_OF_SESSION"),
    edge("e-6", "t1", "m3", "REFERENCE_FOR_TASK"),
];

describe("graphInsights", () => {
    it("scores node importance and bridge strength", () => {
        const nodeMap = new Map(nodes.map((item) => [item.id, item]));
        const adjacency = buildAdjacency(edges, nodeMap);
        const importance = scoreNodeImportance(nodes, edges, adjacency);

        expect(importance.get("t1")?.importance).toBeGreaterThan(0);
        expect(importance.get("m1")?.weightedDegree).toBeGreaterThan(0);

        const clusterData = buildClusterInsights(nodes, edges, "app", importance);
        const bridges = scoreBridgeStrength(nodes, adjacency, clusterData.clusterKeyByNode);
        expect((bridges.get("t1") ?? 0) > 0).toBe(true);
    });

    it("ranks representative memories with diversity", () => {
        const nodeMap = new Map(nodes.map((item) => [item.id, item]));
        const adjacency = buildAdjacency(edges, nodeMap);
        const importance = scoreNodeImportance(nodes, edges, adjacency);
        const picked = rankRepresentativeMemories(["m1", "m2", "m3"], nodeMap, importance, 2);

        expect(picked.length).toBe(2);
        expect(new Set(picked).size).toBe(2);
    });

    it("derives timeline segments and duplicate grouping", () => {
        const nodeMap = new Map(nodes.map((item) => [item.id, item]));
        const adjacency = buildAdjacency(edges, nodeMap);

        const timeline = deriveTimelineSegments(nodes.filter((item) => item.node_type === "MemoryChunk"), adjacency);
        expect(timeline.segments.length).toBeGreaterThan(0);
        expect(timeline.groupedMemories.length).toBeGreaterThan(0);
    });

    it("derives journey hop semantics", () => {
        const path = ["m1", "t1", "m3"];
        const nodeMap = new Map(nodes.map((item) => [item.id, item]));
        const adjacency = buildAdjacency(edges, nodeMap);
        const base = scoreNodeImportance(nodes, edges, adjacency);
        const clusterData = buildClusterInsights(nodes, edges, "app", base);
        const bridges = scoreBridgeStrength(nodes, adjacency, clusterData.clusterKeyByNode);
        const edgeByPair = buildEdgePairMap(edges);

        const semantics = deriveJourneyHopSemantics(path, nodeMap, edgeByPair, bridges);
        expect(semantics.hops.length).toBe(path.length);
        expect(["direct", "weak", "cross-context", "semantic"]).toContain(semantics.overallQuality);
    });

    it("builds focus neighborhood by semantic mode", () => {
        const nodeMap = new Map(nodes.map((item) => [item.id, item]));
        const adjacency = buildAdjacency(edges, nodeMap);
        const base = scoreNodeImportance(nodes, edges, adjacency);
        const edgeByPair = buildEdgePairMap(edges);

        const neighborhood = buildFocusNeighborhood("m1", nodeMap, adjacency, edgeByPair, "semantic", base);
        expect(neighborhood.centerId).toBe("m1");
        expect(neighborhood.direct.length).toBeGreaterThan(0);
    });
});
