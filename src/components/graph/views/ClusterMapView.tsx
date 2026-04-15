import {
    ClusterConnection,
    ClusterInsight,
    ClusterLens,
    Point,
    TypedGraphNode,
} from "../graphTypes";
import {
    CLUSTER_LENSES,
    describeNode,
    shorten,
    typeCountBadge,
} from "../graphViewConstants";
import { useViewport } from "../useViewport";

interface ClusterMapViewProps {
    clusterLens: ClusterLens;
    onClusterLensChange: (lens: ClusterLens) => void;
    clusters: ClusterInsight[];
    clusterPositions: Map<string, Point>;
    connections: ClusterConnection[];
    nodeMap: Map<string, TypedGraphNode>;
    selectedClusterKey: string | null;
    onSelectCluster: (key: string) => void;
    onOpenFocus: (nodeId: string) => void;
}

function roleColor(role: ClusterInsight["role"]): string {
    if (role === "dominant") {
        return "rgba(104, 181, 255, 0.66)";
    }
    if (role === "bridge") {
        return "rgba(178, 139, 255, 0.6)";
    }
    if (role === "secondary") {
        return "rgba(148, 163, 184, 0.62)";
    }
    return "rgba(148, 163, 184, 0.4)";
}

function connectionStroke(reason: string): string {
    switch (reason) {
        case "shared task":
            return "rgba(255, 154, 95, 0.55)";
        case "shared session":
            return "rgba(104, 181, 255, 0.56)";
        case "shared memory type":
            return "rgba(178, 139, 255, 0.5)";
        default:
            return "rgba(148, 163, 184, 0.38)";
    }
}

function edgePattern(reason: string): string | undefined {
    if (reason === "shared transition") {
        return "2 4";
    }
    if (reason === "shared memory type") {
        return "5 4";
    }
    return undefined;
}

export function ClusterMapView({
    clusterLens,
    onClusterLensChange,
    clusters,
    clusterPositions,
    connections,
    nodeMap,
    selectedClusterKey,
    onSelectCluster,
    onOpenFocus,
}: ClusterMapViewProps) {
    const viewport = useViewport({ minZoom: 0.6, maxZoom: 2.4, step: 0.12 });
    const topClusters = clusters.slice(0, 12);
    const selectedCluster =
        topClusters.find((cluster) => cluster.key === selectedClusterKey) ?? topClusters[0] ?? null;

    const selectedConnections = selectedCluster
        ? connections.filter(
            (connection) =>
                connection.leftKey === selectedCluster.key || connection.rightKey === selectedCluster.key
        )
        : [];

    return (
        <div className="graph-view cluster-view">
            <div className="view-header row">
                <div>
                    <h3>Cluster Map</h3>
                    <p>Work islands by {clusterLens}.</p>
                </div>
                <div className="view-header-controls">
                    <div className="cluster-lens-tabs">
                        {CLUSTER_LENSES.map((lens) => (
                            <button
                                key={lens.key}
                                className={`ui-action-btn ${clusterLens === lens.key ? "active" : ""}`}
                                onClick={() => onClusterLensChange(lens.key)}
                            >
                                {lens.label}
                            </button>
                        ))}
                    </div>
                    <div className="graph-canvas-controls" role="group" aria-label="Cluster map controls">
                        <button className="ui-action-btn" onClick={viewport.zoomOut} aria-label="Zoom out">-</button>
                        <button className="ui-action-btn" onClick={viewport.zoomIn} aria-label="Zoom in">+</button>
                        <button className="ui-action-btn" onClick={viewport.reset}>Reset</button>
                        <span className="graph-canvas-zoom">{Math.round(viewport.zoom * 100)}%</span>
                    </div>
                </div>
            </div>

            <div className="cluster-map-layout">
                <div className="cluster-map-surface">
                    <svg
                        viewBox="0 0 980 430"
                        className={`cluster-map-svg ${viewport.isDragging ? "dragging" : ""}`}
                        onWheel={viewport.onWheel}
                        onMouseDown={viewport.onMouseDown}
                        onMouseMove={viewport.onMouseMove}
                        onMouseUp={viewport.onMouseUp}
                        onMouseLeave={viewport.onMouseLeave}
                    >
                        <g transform={viewport.transform}>
                            {connections.slice(0, 40).map((connection) => {
                                const left = clusterPositions.get(connection.leftKey);
                                const right = clusterPositions.get(connection.rightKey);
                                if (!left || !right) {
                                    return null;
                                }

                                return (
                                    <line
                                        key={`cluster-link-${connection.pair}`}
                                        className="cluster-link"
                                        x1={left.x}
                                        y1={left.y}
                                        x2={right.x}
                                        y2={right.y}
                                        stroke={connectionStroke(connection.primaryReason)}
                                        strokeWidth={0.9 + connection.strength * 2.2}
                                        opacity={0.2 + connection.strength * 0.52}
                                        strokeDasharray={edgePattern(connection.primaryReason)}
                                    >
                                        <title>{`${connection.primaryReason} • ${connection.count}`}</title>
                                    </line>
                                );
                            })}

                            {topClusters.map((cluster) => {
                                const pos = clusterPositions.get(cluster.key);
                                if (!pos) {
                                    return null;
                                }

                                const radius = 17 + cluster.strengthScore * 17;
                                const isActive = cluster.key === selectedCluster?.key;
                                const color = roleColor(cluster.role);

                                return (
                                    <g
                                        key={`cluster-node-${cluster.key}`}
                                        className={`cluster-island-node role-${cluster.role}`}
                                        onClick={() => onSelectCluster(cluster.key)}
                                        style={{
                                            transform: `translate(${pos.x}px, ${pos.y}px)`,
                                            transition: "transform 220ms ease-out",
                                        }}
                                    >
                                        <circle
                                            className="cluster-core"
                                            cx={0}
                                            cy={0}
                                            r={radius}
                                            fill={isActive ? "rgba(13, 20, 30, 0.98)" : "rgba(11, 15, 21, 0.95)"}
                                            stroke={color}
                                            strokeWidth={isActive ? 1.8 : 1}
                                        />
                                        <text x={0} y={2} textAnchor="middle" style={{ fill: "#e8edf6", fontSize: 11, fontWeight: 600 }}>
                                            {cluster.nodeIds.length}
                                        </text>
                                        <text className="cluster-label-title" x={0} y={radius + 13} textAnchor="middle">
                                            {shorten(cluster.label, 15)}
                                        </text>
                                    </g>
                                );
                            })}
                        </g>
                    </svg>
                </div>

                <aside className="cluster-inspector">
                    {selectedCluster ? (
                        <>
                            <div className="inspector-title-row">
                                <strong>{selectedCluster.label}</strong>
                                <span>{selectedCluster.role}</span>
                            </div>
                            <div className="inspector-meta">
                                <span>{selectedCluster.nodeIds.length} nodes</span>
                                <span>{selectedCluster.memoryIds.length} memories</span>
                            </div>

                            <section className="inspector-section">
                                <div className="inspector-section-label">Type Mix</div>
                                <div className="inspector-row">{typeCountBadge(selectedCluster.typeCounts)}</div>
                            </section>

                            <section className="inspector-section">
                                <div className="inspector-section-label">Connected Islands</div>
                                <div className="inspector-row-list">
                                    {selectedConnections.slice(0, 5).map((connection) => {
                                        const peer =
                                            connection.leftKey === selectedCluster.key
                                                ? connection.rightKey
                                                : connection.leftKey;
                                        return (
                                            <div key={`conn-${connection.pair}`} className="inspector-row">
                                                <strong>{shorten(peer, 20)}</strong>
                                                <span>{connection.primaryReason} • {connection.count}</span>
                                            </div>
                                        );
                                    })}
                                </div>
                            </section>

                            <section className="inspector-section">
                                <div className="inspector-section-label">Representative Memories</div>
                                <div className="inspector-row-list">
                                    {selectedCluster.exemplarMemoryIds.slice(0, 5).map((nodeId) => {
                                        const node = nodeMap.get(nodeId);
                                        if (!node) {
                                            return null;
                                        }

                                        return (
                                            <button
                                                key={`cluster-exemplar-${node.id}`}
                                                className="inspector-row-btn"
                                                onClick={() => onOpenFocus(node.id)}
                                            >
                                                <strong>{shorten(node.label, 86)}</strong>
                                                <small>{describeNode(node)}</small>
                                            </button>
                                        );
                                    })}
                                </div>
                            </section>
                        </>
                    ) : (
                        <p className="inline-empty">No cluster selected.</p>
                    )}

                    <section className="inspector-section">
                        <div className="inspector-section-label">Cluster List</div>
                        <div className="cluster-list-rail">
                            {topClusters.slice(0, 8).map((cluster) => (
                                <button
                                    key={`cluster-list-${cluster.key}`}
                                    className={cluster.key === selectedCluster?.key ? "active" : ""}
                                    onClick={() => onSelectCluster(cluster.key)}
                                >
                                    <strong>{cluster.label}</strong>
                                    <span>{cluster.nodeIds.length} • {cluster.role}</span>
                                </button>
                            ))}
                        </div>
                    </section>
                </aside>
            </div>
        </div>
    );
}
