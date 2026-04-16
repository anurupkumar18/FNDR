import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getGraphData, GraphEdgeData } from "../api/tauri";
import "./GraphPanel.css";
import "./graph/GraphViews.css";
import {
    asTypedGraphNodes,
    activityFacetForNode,
    buildAdjacency,
    buildClusterInsights,
    buildEdgePairMap,
    buildFocusNeighborhood,
    clusterKeyForNode,
    deriveJourneyHopSemantics,
    deriveTimelineSegments,
    nodeMatchesActivityFacet,
    relationSignals,
    scoreBridgeStrength,
    scoreNodeImportance,
    shortestPath,
} from "./graph/graphInsights";
import {
    buildConstellationLayout,
    layoutClusterIslands,
    layoutFocusRings,
    layoutJourneyPath,
    radialPositions,
} from "./graph/graphLayouts";
import {
    ACTIVITY_FACETS,
    ACTIVITY_FACET_COLORS,
    VIEW_MODES,
} from "./graph/graphViewConstants";
import {
    ActivityFacet,
    ClusterLens,
    FocusMode,
    NodeInsight,
    TypedGraphNode,
    ViewMode,
} from "./graph/graphTypes";
import { ClusterMapView } from "./graph/views/ClusterMapView";
import { JourneyPathView } from "./graph/views/JourneyPathView";
import { FocusGraphView } from "./graph/views/FocusGraphView";
import { TimelineGraphView } from "./graph/views/TimelineGraphView";
import { ConstellationView } from "./graph/views/ConstellationView";

interface GraphPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

interface GraphNavigationSnapshot {
    viewMode: ViewMode;
    clusterLens: ClusterLens;
    activityFacet: ActivityFacet;
    selectedNodeId: string | null;
    selectedClusterKey: string | null;
    journeyStartId: string;
    journeyEndId: string;
    focusMode: FocusMode;
}

interface GraphNavigationState {
    past: GraphNavigationSnapshot[];
    future: GraphNavigationSnapshot[];
}

function updateNodeInsightsWithBridge(
    base: Map<string, NodeInsight>,
    bridgeScores: Map<string, number>
): Map<string, NodeInsight> {
    const out = new Map<string, NodeInsight>();

    base.forEach((insight, nodeId) => {
        const bridgeStrength = bridgeScores.get(nodeId) ?? 0;
        out.set(nodeId, {
            ...insight,
            bridgeStrength,
            isBridge: bridgeStrength >= 0.56,
            labelPriority: Math.max(0, Math.min(1, insight.labelPriority + bridgeStrength * 0.16)),
        });
    });

    return out;
}

function snapshotEquals(a: GraphNavigationSnapshot, b: GraphNavigationSnapshot): boolean {
    return (
        a.viewMode === b.viewMode &&
        a.clusterLens === b.clusterLens &&
        a.activityFacet === b.activityFacet &&
        a.selectedNodeId === b.selectedNodeId &&
        a.selectedClusterKey === b.selectedClusterKey &&
        a.journeyStartId === b.journeyStartId &&
        a.journeyEndId === b.journeyEndId &&
        a.focusMode === b.focusMode
    );
}

function pickConstellationNodes(
    sortedNodesByTime: TypedGraphNode[],
    clusterKeyByNode: Map<string, string>,
    nodeInsights: Map<string, NodeInsight>
): TypedGraphNode[] {
    if (sortedNodesByTime.length === 0) {
        return [];
    }

    const budget =
        sortedNodesByTime.length > 9000
            ? 460
            : sortedNodesByTime.length > 4000
                ? 420
                : sortedNodesByTime.length > 1800
                    ? 350
                    : 240;

    const grouped = new Map<string, TypedGraphNode[]>();
    sortedNodesByTime.forEach((node) => {
        const key = clusterKeyByNode.get(node.id) ?? "unassigned";
        if (!grouped.has(key)) {
            grouped.set(key, []);
        }
        grouped.get(key)?.push(node);
    });

    const now = Date.now();
    const scoreByNode = new Map<string, number>();
    sortedNodesByTime.forEach((node) => {
        const insight = nodeInsights.get(node.id);
        const recencyDays = (now - node.created_at) / (1000 * 60 * 60 * 24);
        const recencyBoost = Math.max(0, 1 - recencyDays / 14);
        const score =
            (insight?.importance ?? 0.18) * 0.58 +
            (insight?.bridgeStrength ?? 0) * 0.22 +
            (insight?.labelPriority ?? 0) * 0.14 +
            recencyBoost * 0.06;
        scoreByNode.set(node.id, score);
    });

    const selectedIds = new Set<string>();
    grouped.forEach((nodes) => {
        const ranked = [...nodes].sort((a, b) => (scoreByNode.get(b.id) ?? 0) - (scoreByNode.get(a.id) ?? 0));
        const quota = Math.max(3, Math.min(28, Math.round(Math.sqrt(nodes.length) * 1.9)));
        ranked.slice(0, quota).forEach((node) => selectedIds.add(node.id));
    });

    const rankedGlobal = [...sortedNodesByTime].sort((a, b) => (scoreByNode.get(b.id) ?? 0) - (scoreByNode.get(a.id) ?? 0));
    if (selectedIds.size < budget) {
        rankedGlobal.forEach((node) => {
            if (selectedIds.size >= budget) {
                return;
            }
            selectedIds.add(node.id);
        });
    }

    return rankedGlobal.filter((node) => selectedIds.has(node.id)).slice(0, budget);
}

export function GraphPanel({ isVisible, onClose }: GraphPanelProps) {
    const [rawNodes, setRawNodes] = useState<TypedGraphNode[]>([]);
    const [rawEdges, setRawEdges] = useState<GraphEdgeData[]>([]);
    const [loading, setLoading] = useState(true);

    const [viewMode, setViewMode] = useState<ViewMode>("cluster");
    const [clusterLens, setClusterLens] = useState<ClusterLens>("app");
    const [activityFacet, setActivityFacet] = useState<ActivityFacet>("all");
    const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
    const [selectedClusterKey, setSelectedClusterKey] = useState<string | null>(null);
    const [journeyStartId, setJourneyStartId] = useState<string>("");
    const [journeyEndId, setJourneyEndId] = useState<string>("");
    const [focusMode, setFocusMode] = useState<FocusMode>("semantic");
    const [navigation, setNavigation] = useState<GraphNavigationState>({ past: [], future: [] });
    const isRestoringNavigation = useRef(false);

    useEffect(() => {
        if (!isVisible) {
            return;
        }

        setLoading(true);
        getGraphData()
            .then((data) => {
                setRawNodes(asTypedGraphNodes(data.nodes));
                setRawEdges(data.edges);
            })
            .catch((error) => {
                console.error("Failed to load graph data:", error);
                setRawNodes([]);
                setRawEdges([]);
            })
            .finally(() => {
                setLoading(false);
            });
    }, [isVisible]);

    useEffect(() => {
        if (!isVisible) {
            return;
        }
        isRestoringNavigation.current = false;
        setNavigation({ past: [], future: [] });
    }, [isVisible]);

    const activityFacetCounts = useMemo(() => {
        const counts: Record<ActivityFacet, number> = {
            all: 0,
            productive: 0,
            unproductive: 0,
            learning: 0,
            communication: 0,
            research: 0,
            neutral: 0,
        };

        rawNodes.forEach((node) => {
            counts.all += 1;
            const derivedFacet = activityFacetForNode(node);
            counts[derivedFacet] += 1;
        });

        return counts;
    }, [rawNodes]);

    const filteredNodes = useMemo(
        () => rawNodes.filter((node) => nodeMatchesActivityFacet(node, activityFacet)),
        [rawNodes, activityFacet]
    );

    const filteredNodeMap = useMemo(() => {
        const map = new Map<string, TypedGraphNode>();
        filteredNodes.forEach((node) => map.set(node.id, node));
        return map;
    }, [filteredNodes]);

    const filteredEdges = useMemo(
        () => rawEdges.filter((edge) => filteredNodeMap.has(edge.source) && filteredNodeMap.has(edge.target)),
        [rawEdges, filteredNodeMap]
    );

    const adjacency = useMemo(
        () => buildAdjacency(filteredEdges, filteredNodeMap),
        [filteredEdges, filteredNodeMap]
    );

    const edgeByPair = useMemo(() => buildEdgePairMap(filteredEdges), [filteredEdges]);

    const sortedNodesByTime = useMemo(
        () => [...filteredNodes].sort((a, b) => b.created_at - a.created_at),
        [filteredNodes]
    );

    const memoryNodes = useMemo(
        () => sortedNodesByTime.filter((node) => node.node_type === "MemoryChunk"),
        [sortedNodesByTime]
    );

    const timelineMemoryNodes = useMemo(
        () => memoryNodes.slice(0, 1800),
        [memoryNodes]
    );

    useEffect(() => {
        if (!isVisible || sortedNodesByTime.length === 0) {
            return;
        }

        if (!selectedNodeId || !filteredNodeMap.has(selectedNodeId)) {
            setSelectedNodeId(sortedNodesByTime[0].id);
        }

        if (!journeyStartId || !filteredNodeMap.has(journeyStartId)) {
            setJourneyStartId(sortedNodesByTime[Math.min(6, sortedNodesByTime.length - 1)].id);
        }

        if (!journeyEndId || !filteredNodeMap.has(journeyEndId)) {
            setJourneyEndId(sortedNodesByTime[0].id);
        }
    }, [isVisible, sortedNodesByTime, selectedNodeId, journeyStartId, journeyEndId, filteredNodeMap]);

    const nodeInsightsBase = useMemo(
        () => scoreNodeImportance(filteredNodes, filteredEdges, adjacency),
        [filteredNodes, filteredEdges, adjacency]
    );

    const lensClusterKeyByNode = useMemo(() => {
        const map = new Map<string, string>();
        filteredNodes.forEach((node) => map.set(node.id, clusterKeyForNode(node, clusterLens) || "unassigned"));
        return map;
    }, [filteredNodes, clusterLens]);

    const bridgeScores = useMemo(
        () => scoreBridgeStrength(filteredNodes, adjacency, lensClusterKeyByNode),
        [filteredNodes, adjacency, lensClusterKeyByNode]
    );

    const nodeInsights = useMemo(
        () => updateNodeInsightsWithBridge(nodeInsightsBase, bridgeScores),
        [nodeInsightsBase, bridgeScores]
    );

    const clusterData = useMemo(
        () => buildClusterInsights(filteredNodes, filteredEdges, clusterLens, nodeInsights),
        [filteredNodes, filteredEdges, clusterLens, nodeInsights]
    );

    const topClusters = useMemo(() => clusterData.clusters.slice(0, 12), [clusterData.clusters]);

    const clusterPositions = useMemo(
        () => layoutClusterIslands(topClusters, 980, 430),
        [topClusters]
    );

    useEffect(() => {
        if (topClusters.length === 0) {
            setSelectedClusterKey(null);
            return;
        }

        if (!selectedClusterKey || !topClusters.some((cluster) => cluster.key === selectedClusterKey)) {
            setSelectedClusterKey(topClusters[0].key);
        }
    }, [topClusters, selectedClusterKey]);

    const focusCenterId = selectedNodeId && filteredNodeMap.has(selectedNodeId) ? selectedNodeId : sortedNodesByTime[0]?.id ?? null;

    const focusNeighborhood = useMemo(
        () =>
            focusCenterId
                ? buildFocusNeighborhood(
                    focusCenterId,
                    filteredNodeMap,
                    adjacency,
                    edgeByPair,
                    focusMode,
                    nodeInsights,
                    10,
                    14
                )
                : {
                    centerId: "",
                    direct: [],
                    secondary: [],
                    displayNodeIds: new Set<string>(),
                },
        [focusCenterId, filteredNodeMap, adjacency, edgeByPair, focusMode, nodeInsights]
    );

    const focusPositions = useMemo(
        () =>
            focusCenterId
                ? layoutFocusRings(
                    focusCenterId,
                    focusNeighborhood.direct.map((item) => item.nodeId),
                    focusNeighborhood.secondary.map((item) => item.nodeId),
                    980,
                    430
                )
                : new Map<string, { x: number; y: number; ring: 0 | 1 | 2 }>(),
        [focusCenterId, focusNeighborhood]
    );

    const focusEdges = useMemo(
        () =>
            filteredEdges
                .filter(
                    (edge) =>
                        focusNeighborhood.displayNodeIds.has(edge.source) &&
                        focusNeighborhood.displayNodeIds.has(edge.target)
                )
                .slice(0, 180),
        [filteredEdges, focusNeighborhood.displayNodeIds]
    );

    const relationSignalsByMemory = useMemo(() => {
        const map = new Map<string, string[]>();
        for (let i = 0; i < timelineMemoryNodes.length - 1; i++) {
            const current = timelineMemoryNodes[i];
            const previous = timelineMemoryNodes[i + 1];
            const signals = relationSignals(current, previous);
            if (signals.length > 0) {
                map.set(current.id, signals.slice(0, 2));
            }
        }
        return map;
    }, [timelineMemoryNodes]);

    const timelineInsight = useMemo(
        () => deriveTimelineSegments(timelineMemoryNodes, adjacency),
        [timelineMemoryNodes, adjacency]
    );

    const journeyOptions = useMemo(() => sortedNodesByTime.slice(0, 140), [sortedNodesByTime]);

    const journeyPath = useMemo(
        () => shortestPath(journeyStartId, journeyEndId, adjacency),
        [journeyStartId, journeyEndId, adjacency]
    );

    const journeySemantics = useMemo(
        () => deriveJourneyHopSemantics(journeyPath, filteredNodeMap, edgeByPair, bridgeScores),
        [journeyPath, filteredNodeMap, edgeByPair, bridgeScores]
    );

    const journeyLayout = useMemo(
        () => layoutJourneyPath(journeyPath.slice(0, 16), adjacency, 980, 250),
        [journeyPath, adjacency]
    );

    const timelineBridge = useMemo(() => {
        if (journeyPath.length > 0 || !journeyStartId || !journeyEndId) {
            return [] as TypedGraphNode[];
        }

        const start = filteredNodeMap.get(journeyStartId);
        const end = filteredNodeMap.get(journeyEndId);
        if (!start || !end) {
            return [];
        }

        const minTs = Math.min(start.created_at, end.created_at);
        const maxTs = Math.max(start.created_at, end.created_at);

        return memoryNodes
            .slice(0, 800)
            .filter((node) => node.created_at >= minTs && node.created_at <= maxTs)
            .slice(0, 8);
    }, [journeyPath.length, journeyStartId, journeyEndId, filteredNodeMap, memoryNodes]);

    const constellationClusterData = useMemo(
        () => buildClusterInsights(filteredNodes, filteredEdges, "app", nodeInsights),
        [filteredNodes, filteredEdges, nodeInsights]
    );

    const constellationNodes = useMemo(
        () =>
            pickConstellationNodes(
                sortedNodesByTime,
                constellationClusterData.clusterKeyByNode,
                nodeInsights
            ),
        [sortedNodesByTime, constellationClusterData.clusterKeyByNode, nodeInsights]
    );

    const constellationClusterKeys = useMemo(
        () => [...new Set(constellationNodes.map((node) => constellationClusterData.clusterKeyByNode.get(node.id) ?? "unassigned"))],
        [constellationNodes, constellationClusterData.clusterKeyByNode]
    );

    const constellationCenters = useMemo(() => {
        const topConstClusters = constellationClusterData.clusters
            .filter((cluster) => constellationClusterKeys.includes(cluster.key))
            .slice(0, 12);

        const map = layoutClusterIslands(topConstClusters, 980, 430);
        const missing = constellationClusterKeys.filter((key) => !map.has(key));
        const fallback = radialPositions(missing, 980, 430, { radius: 168, startAngle: -Math.PI / 2 });
        fallback.forEach((point, key) => map.set(key, point));
        return map;
    }, [constellationClusterData.clusters, constellationClusterKeys]);

    const constellationPoints = useMemo(() => {
        return buildConstellationLayout(
            constellationNodes.map((node) => {
                const cluster = constellationClusterData.clusterKeyByNode.get(node.id) ?? "unassigned";
                const clusterRole = constellationClusterData.clusters.find((item) => item.key === cluster)?.role ?? "peripheral";
                const roleWeight =
                    clusterRole === "dominant"
                        ? 1.3
                        : clusterRole === "bridge"
                            ? 1.2
                            : clusterRole === "secondary"
                                ? 1
                                : 0.75;
                return {
                    id: node.id,
                    cluster,
                    importance: nodeInsights.get(node.id)?.importance ?? 0.2,
                    roleWeight,
                };
            }),
            constellationCenters
        );
    }, [constellationNodes, constellationClusterData, nodeInsights, constellationCenters]);

    const uniqueSessions = useMemo(() => {
        const sessionIds = new Set<string>();
        memoryNodes.forEach((node) => {
            const sessionId = String(node.metadata?.session_id ?? "").trim();
            if (sessionId) {
                sessionIds.add(sessionId);
            }
        });
        return sessionIds.size;
    }, [memoryNodes]);

    const selectNode = useCallback((nodeId: string) => {
        setSelectedNodeId(nodeId);
    }, []);

    const navigationSnapshot = useMemo<GraphNavigationSnapshot>(
        () => ({
            viewMode,
            clusterLens,
            activityFacet,
            selectedNodeId,
            selectedClusterKey,
            journeyStartId,
            journeyEndId,
            focusMode,
        }),
        [viewMode, clusterLens, activityFacet, selectedNodeId, selectedClusterKey, journeyStartId, journeyEndId, focusMode]
    );

    useEffect(() => {
        if (!isVisible || loading || isRestoringNavigation.current) {
            return;
        }

        setNavigation((current) => {
            const last = current.past[current.past.length - 1];
            if (last && snapshotEquals(last, navigationSnapshot)) {
                return current;
            }

            const nextPast = [...current.past, navigationSnapshot];
            if (nextPast.length > 80) {
                nextPast.shift();
            }
            return { past: nextPast, future: [] };
        });
    }, [isVisible, loading, navigationSnapshot]);

    const activeViewLabel = VIEW_MODES.find((mode) => mode.key === viewMode)?.label ?? "Graph";

    if (!isVisible) {
        return null;
    }

    return (
        <div className="graph-panel">
            <div className="graph-header">
                <div className="graph-title-wrap">
                    <p className="graph-breadcrumb">Memory Base &gt; Projects &gt; FNDR &gt; {activeViewLabel}</p>
                    <h2>Knowledge Base Overview · Core Connections ({rawNodes.length.toLocaleString()} Memories)</h2>
                    <p>Scene-based graph navigation optimized for clarity and scale.</p>
                    <div className="graph-stats-row">
                        <span>Nodes {filteredNodes.length.toLocaleString()} / {rawNodes.length.toLocaleString()}</span>
                        <span>Edges {filteredEdges.length.toLocaleString()} / {rawEdges.length.toLocaleString()}</span>
                        <span>Sessions {uniqueSessions.toLocaleString()}</span>
                        <span>Scene Budget {constellationNodes.length.toLocaleString()}</span>
                        <span>History {navigation.past.length.toLocaleString()}</span>
                    </div>
                </div>
                <div className="graph-nav-controls" role="group" aria-label="Graph navigation">
                    <button
                        className="ui-action-btn"
                        onClick={goBack}
                        disabled={!canGoBack}
                        title="Navigate back"
                    >
                        ←
                    </button>
                    <button
                        className="ui-action-btn"
                        onClick={goForward}
                        disabled={!canGoForward}
                        title="Navigate forward"
                    >
                        →
                    </button>
                    <button className="ui-action-btn graph-close-btn" onClick={onClose}>
                        ✕ Close
                    </button>
                </div>
            </div>

            <div className="graph-top-controls">
                <div className="graph-control-block">
                    <span className="graph-control-label">View Lens</span>
                    <div className="graph-mode-tabs" role="tablist" aria-label="Graph Views">
                        {VIEW_MODES.map((mode) => (
                            <button
                                key={mode.key}
                                className={`ui-action-btn graph-mode-tab ${viewMode === mode.key ? "active" : ""}`}
                                onClick={() => setViewMode(mode.key)}
                                title={mode.description}
                            >
                                {mode.label}
                            </button>
                        ))}

                    </div>
                </div>

                <div className="graph-control-block">
                    <span className="graph-control-label">Activity Filter</span>
                    <div className="graph-type-filters">
                        {ACTIVITY_FACETS.map((facet) => (
                            <button
                                key={facet.key}
                                className={`graph-type-filter ${activityFacet === facet.key ? "active" : "inactive"}`}
                                onClick={() => setActivityFacet(facet.key)}
                            >
                                <span className="graph-type-dot" style={{ background: ACTIVITY_FACET_COLORS[facet.key] }} />
                                <span className="graph-type-label">{facet.label}</span>
                                <strong>{activityFacetCounts[facet.key]}</strong>
                            </button>
                        ))}
                    </div>
                </div>
            </div>

            <div className="graph-view-shell">
                {loading ? (
                    <div className="graph-loading">
                        <div className="thinking-loader thinking-loader-md" aria-hidden="true" />
                        Loading graph data...
                    </div>
                ) : filteredNodes.length === 0 ? (
                    <div className="graph-empty">
                        <p>No visible graph data for the selected activity filter.</p>
                    </div>
                ) : (
                    <>
                        {viewMode === "cluster" && (
                            <ClusterMapView
                                clusterLens={clusterLens}
                                onClusterLensChange={setClusterLens}
                                clusters={clusterData.clusters}
                                clusterPositions={clusterPositions}
                                connections={clusterData.connections}
                                nodeMap={filteredNodeMap}
                                selectedClusterKey={selectedClusterKey}
                                onSelectCluster={setSelectedClusterKey}
                                onOpenFocus={selectNode}
                            />
                        )}

                        {viewMode === "journey" && (
                            <JourneyPathView
                                journeyOptions={journeyOptions}
                                journeyStartId={journeyStartId}
                                journeyEndId={journeyEndId}
                                onJourneyStartChange={setJourneyStartId}
                                onJourneyEndChange={setJourneyEndId}
                                journeyPath={journeyPath}
                                journeySemantics={journeySemantics}
                                journeyLayout={journeyLayout}
                                nodeMap={filteredNodeMap}
                                edgeByPair={edgeByPair}
                                timelineBridge={timelineBridge}
                                onOpenFocus={selectNode}
                            />
                        )}

                        {viewMode === "focus" && (
                            <FocusGraphView
                                focusMode={focusMode}
                                onFocusModeChange={setFocusMode}
                                centerId={focusCenterId}
                                onCenterChange={setSelectedNodeId}
                                options={sortedNodesByTime.slice(0, 180)}
                                neighborhood={focusNeighborhood}
                                positions={focusPositions}
                                visibleEdges={focusEdges}
                                nodeMap={filteredNodeMap}
                                onOpenFocus={selectNode}
                            />
                        )}

                        {viewMode === "timeline" && (
                            <TimelineGraphView
                                timelineInsight={timelineInsight}
                                nodeMap={filteredNodeMap}
                                adjacency={adjacency}
                                relationSignalsByMemory={relationSignalsByMemory}
                                onOpenFocus={selectNode}
                            />
                        )}

                        {viewMode === "constellation" && (
                            <ConstellationView
                                nodes={constellationNodes}
                                edges={filteredEdges}
                                pointPositions={constellationPoints}
                                adjacency={adjacency}
                                clusterKeyByNode={constellationClusterData.clusterKeyByNode}
                                nodeInsights={nodeInsights}
                                bridgeScores={bridgeScores}
                                clusters={constellationClusterData.clusters}
                                selectedNodeId={selectedNodeId}
                                onSelectNodeId={setSelectedNodeId}
                                onOpenFocus={selectNode}
                            />
                        )}
                    </>
                )}
            </div>
        </div>
    );
}
