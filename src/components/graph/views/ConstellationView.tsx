import { useMemo, useState } from "react";
import { GraphEdgeData } from "../../../api/tauri";
import { buildClusterHullPath, curvedConnectionPath } from "../graphLayouts";
import { ClusterInsight, NodeInsight, PositionedNode, TypedGraphNode } from "../graphTypes";
import { NODE_TYPE_META, shorten } from "../graphViewConstants";
import { scoreEdgeProminence } from "../graphInsights";
import { useViewport } from "../useViewport";

interface ConstellationViewProps {
    nodes: TypedGraphNode[];
    edges: GraphEdgeData[];
    pointPositions: Map<string, PositionedNode>;
    adjacency: Map<string, Set<string>>;
    clusterKeyByNode: Map<string, string>;
    nodeInsights: Map<string, NodeInsight>;
    bridgeScores: Map<string, number>;
    clusters: ClusterInsight[];
    selectedNodeId: string | null;
    onSelectNodeId: (id: string) => void;
    onOpenFocus: (id: string) => void;
}

function edgeColor(edgeType: string): string {
    if (edgeType === "REFERENCE_FOR_TASK") {
        return "#fb923c";
    }
    if (edgeType === "PART_OF_SESSION") {
        return "#60a5fa";
    }
    if (edgeType === "OCCURRED_AT") {
        return "#5dd9b4";
    }
    return "#94a3b8";
}

export function ConstellationView({
    nodes,
    edges,
    pointPositions,
    adjacency,
    clusterKeyByNode,
    nodeInsights,
    bridgeScores,
    clusters,
    selectedNodeId,
    onSelectNodeId,
    onOpenFocus,
}: ConstellationViewProps) {
    const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);
    const [isolatedCluster, setIsolatedCluster] = useState<string | null>(null);
    const viewport = useViewport({ minZoom: 0.55, maxZoom: 2.6, step: 0.12 });

    const selectedNodeCluster = selectedNodeId ? clusterKeyByNode.get(selectedNodeId) ?? null : null;

    const highlightedIds = useMemo(() => {
        const set = new Set<string>();
        if (hoveredNodeId) {
            set.add(hoveredNodeId);
            (adjacency.get(hoveredNodeId) ?? new Set()).forEach((neighbor) => set.add(neighbor));
        }

        if (selectedNodeId) {
            set.add(selectedNodeId);
            const first = adjacency.get(selectedNodeId) ?? new Set();
            first.forEach((neighbor) => {
                set.add(neighbor);
                (adjacency.get(neighbor) ?? new Set()).forEach((secondNeighbor) => set.add(secondNeighbor));
            });
        }

        return set;
    }, [hoveredNodeId, selectedNodeId, adjacency]);

    const visibleNodes = useMemo(() => {
        return nodes.filter((node) => {
            if (!isolatedCluster) {
                return true;
            }
            return clusterKeyByNode.get(node.id) === isolatedCluster;
        });
    }, [nodes, isolatedCluster, clusterKeyByNode]);

    const visibleNodeSet = new Set(visibleNodes.map((node) => node.id));

    const shellPaths = useMemo(() => {
        const byCluster = new Map<string, PositionedNode[]>();
        visibleNodes.forEach((node) => {
            const clusterKey = clusterKeyByNode.get(node.id) ?? "unassigned";
            if (!byCluster.has(clusterKey)) {
                byCluster.set(clusterKey, []);
            }
            const point = pointPositions.get(node.id);
            if (point) {
                byCluster.get(clusterKey)?.push(point);
            }
        });

        return [...byCluster.entries()]
            .map(([clusterKey, points]) => ({
                clusterKey,
                path: buildClusterHullPath(points, 16),
            }))
            .filter((entry) => entry.path.length > 0);
    }, [visibleNodes, pointPositions, clusterKeyByNode]);

    const visibleEdges = useMemo(() => {
        return edges.filter((edge) => visibleNodeSet.has(edge.source) && visibleNodeSet.has(edge.target)).slice(0, 420);
    }, [edges, visibleNodeSet]);

    const roleByCluster = new Map(clusters.map((cluster) => [cluster.key, cluster.role]));

    return (
        <div className="graph-view constellation-view">
            <div className="view-header row">
                <div>
                    <h3>Constellation</h3>
                    <p>{isolatedCluster ? `Isolated: ${shorten(isolatedCluster, 18)}` : "Scale-ready constellation with adaptive labels."}</p>
                </div>
                <div className="graph-canvas-controls" role="group" aria-label="Constellation controls">
                    <button className="ui-action-btn" onClick={viewport.zoomOut} aria-label="Zoom out">
                        -
                    </button>
                    <button className="ui-action-btn" onClick={viewport.zoomIn} aria-label="Zoom in">
                        +
                    </button>
                    <button className="ui-action-btn" onClick={viewport.reset}>
                        Reset
                    </button>
                    <span className="graph-canvas-zoom">{Math.round(viewport.zoom * 100)}%</span>
                </div>
            </div>

            <div
                className="constellation-map-surface"
                onWheel={viewport.onWheel}
            >
                <svg
                    viewBox="0 0 980 430"
                    className={`constellation-map-svg ${viewport.isDragging ? "dragging" : ""}`}
                    onMouseDown={viewport.onMouseDown}
                    onMouseMove={viewport.onMouseMove}
                    onMouseUp={viewport.onMouseUp}
                    onMouseLeave={viewport.onMouseLeave}
                >
                    <g transform={viewport.transform}>
                        {shellPaths.map((entry) => {
                            const role = roleByCluster.get(entry.clusterKey);
                            const opacity = role === "dominant" ? 0.16 : role === "bridge" ? 0.12 : 0.08;
                            return (
                                <path
                                    key={`constellation-hull-${entry.clusterKey}`}
                                    className="constellation-hull"
                                    d={entry.path}
                                    style={{ opacity }}
                                />
                            );
                        })}

                        {visibleEdges.map((edge) => {
                            const source = pointPositions.get(edge.source);
                            const target = pointPositions.get(edge.target);
                            if (!source || !target) {
                                return null;
                            }

                            const prominence = scoreEdgeProminence(edge, nodeInsights, bridgeScores, {
                                selectedNodeId,
                                hoveredNodeId,
                                highlightedIds,
                            });

                            if (prominence < 0.07) {
                                return null;
                            }

                            return (
                                <path
                                    key={`const-edge-${edge.id}`}
                                    className="constellation-edge"
                                    d={curvedConnectionPath(source, target, 0.11)}
                                    stroke={edgeColor(edge.edge_type)}
                                    strokeWidth={0.45 + prominence * 1.4}
                                    opacity={0.04 + prominence * 0.72}
                                />
                            );
                        })}

                        {[...new Set(visibleNodes.map((node) => clusterKeyByNode.get(node.id) ?? "unassigned"))].map((clusterKey) => {
                            const points = visibleNodes
                                .filter((node) => (clusterKeyByNode.get(node.id) ?? "unassigned") === clusterKey)
                                .map((node) => pointPositions.get(node.id))
                                .filter((point): point is PositionedNode => Boolean(point));

                            if (points.length === 0) {
                                return null;
                            }

                            const centerX = points.reduce((sum, point) => sum + point.x, 0) / points.length;
                            const centerY = points.reduce((sum, point) => sum + point.y, 0) / points.length;
                            const role = roleByCluster.get(clusterKey);
                            if (role !== "dominant" && role !== "bridge" && clusterKey !== selectedNodeCluster) {
                                return null;
                            }

                            return (
                                <text
                                    key={`const-label-${clusterKey}`}
                                    className="constellation-cluster-label"
                                    x={centerX}
                                    y={centerY - 25}
                                    textAnchor="middle"
                                >
                                    {shorten(clusterKey, 18)}
                                </text>
                            );
                        })}

                        {visibleNodes.map((node) => {
                            const point = pointPositions.get(node.id);
                            if (!point) {
                                return null;
                            }

                            const insight = nodeInsights.get(node.id);
                            const bridge = bridgeScores.get(node.id) ?? 0;
                            const importance = insight?.importance ?? 0.2;
                            const clusterRole = roleByCluster.get(clusterKeyByNode.get(node.id) ?? "") ?? "peripheral";

                            const radius =
                                node.node_type === "MemoryChunk"
                                    ? 2.4 + importance * 4.8
                                    : node.node_type === "Task"
                                        ? 3.4 + importance * 4
                                        : node.node_type === "Entity"
                                            ? 3.8 + importance * 4.4
                                            : 2.8 + importance * 3.8;

                            const selected = node.id === selectedNodeId;
                            const hovered = node.id === hoveredNodeId;
                            const highlighted = highlightedIds.has(node.id);
                            const fadePeripheral = clusterRole === "peripheral" && !selected && !hovered && !highlighted;

                            const showLabel =
                                selected ||
                                hovered ||
                                highlighted ||
                                (insight?.labelPriority ?? 0) > 0.82 ||
                                bridge > 0.64;

                            const meta = NODE_TYPE_META[node.node_type];

                            return (
                                <g
                                    key={`constellation-node-${node.id}`}
                                    className={`constellation-node ${fadePeripheral ? "peripheral" : ""} ${selected ? "selected" : ""}`}
                                    onMouseEnter={() => setHoveredNodeId(node.id)}
                                    onMouseLeave={() => setHoveredNodeId(null)}
                                    onClick={(event) => {
                                        if (event.altKey) {
                                            const clusterKey = clusterKeyByNode.get(node.id) ?? null;
                                            setIsolatedCluster((current) => (current === clusterKey ? null : clusterKey));
                                            return;
                                        }
                                        onSelectNodeId(node.id);
                                    }}
                                    onDoubleClick={() => onOpenFocus(node.id)}
                                >
                                    <circle
                                        cx={point.x}
                                        cy={point.y}
                                        r={radius}
                                        fill={node.node_type === "MemoryChunk" ? `${meta.color}40` : `${meta.color}22`}
                                        stroke={meta.color}
                                        strokeWidth={selected ? 1.7 : hovered ? 1.25 : 0.95}
                                    />
                                    {showLabel && (
                                        <text className="constellation-node-label" x={point.x} y={point.y - radius - 5} textAnchor="middle">
                                            {shorten(node.label, 20)}
                                        </text>
                                    )}
                                    <title>{node.label}</title>
                                </g>
                            );
                        })}
                    </g>
                </svg>
            </div>

            <p className="constellation-inspector-note">
                Pan with drag. Alt-click any node to isolate its cluster. Labels render only for high-signal nodes.
            </p>
        </div>
    );
}
