import { GraphEdgeData } from "../../../api/tauri";
import { FocusMode, FocusNeighborhood, TypedGraphNode } from "../graphTypes";
import {
    NODE_TYPE_META,
    describeNode,
    formatDateTime,
    shorten,
} from "../graphViewConstants";
import { useViewport } from "../useViewport";

interface FocusGraphViewProps {
    focusMode: FocusMode;
    onFocusModeChange: (mode: FocusMode) => void;
    centerId: string | null;
    onCenterChange: (id: string) => void;
    options: TypedGraphNode[];
    neighborhood: FocusNeighborhood;
    positions: Map<string, { x: number; y: number; ring: 0 | 1 | 2 }>;
    visibleEdges: GraphEdgeData[];
    nodeMap: Map<string, TypedGraphNode>;
    onOpenFocus: (id: string) => void;
}

const FOCUS_MODES: Array<{ key: FocusMode; label: string }> = [
    { key: "structural", label: "Structural" },
    { key: "semantic", label: "Semantic" },
    { key: "causal", label: "Causal" },
];

export function FocusGraphView({
    focusMode,
    onFocusModeChange,
    centerId,
    onCenterChange,
    options,
    neighborhood,
    positions,
    visibleEdges,
    nodeMap,
    onOpenFocus,
}: FocusGraphViewProps) {
    const viewport = useViewport({ minZoom: 0.65, maxZoom: 2.6, step: 0.1 });
    const center = centerId ? nodeMap.get(centerId) : null;

    const directNodes = neighborhood.direct
        .map((neighbor) => {
            const node = nodeMap.get(neighbor.nodeId);
            return node ? { neighbor, node } : null;
        })
        .filter((item): item is { neighbor: FocusNeighborhood["direct"][number]; node: TypedGraphNode } => Boolean(item));

    const secondaryNodes = neighborhood.secondary
        .map((neighbor) => {
            const node = nodeMap.get(neighbor.nodeId);
            return node ? { neighbor, node } : null;
        })
        .filter((item): item is { neighbor: FocusNeighborhood["secondary"][number]; node: TypedGraphNode } => Boolean(item));

    const relationshipReasons = Array.from(
        new Set([...directNodes.flatMap((item) => item.neighbor.reasons), ...secondaryNodes.flatMap((item) => item.neighbor.reasons)])
    ).slice(0, 8);

    const explainDirect = directNodes.slice(0, 6);

    const similarMemories = directNodes
        .filter((item) => item.node.node_type === "MemoryChunk")
        .filter((item) => item.neighbor.reasons.some((reason) => reason.includes("same") || reason.includes("revisited")))
        .slice(0, 5);

    const followUps = [...directNodes, ...secondaryNodes]
        .filter((item) => item.node.created_at >= (center?.created_at ?? 0))
        .slice(0, 6);

    return (
        <div className="graph-view focus-view">
            <div className="view-header row">
                <div>
                    <h3>Focus Graph</h3>
                    <p>{center ? `${neighborhood.direct.length} direct • ${neighborhood.secondary.length} secondary` : ""}</p>
                </div>
                <div className="view-header-controls">
                    <select className="focus-node-select" value={centerId ?? ""} onChange={(event) => onCenterChange(event.target.value)}>
                        {options.map((node) => (
                            <option key={node.id} value={node.id}>
                                {NODE_TYPE_META[node.node_type].label} • {shorten(node.label, 66)}
                            </option>
                        ))}
                    </select>
                    <div className="graph-canvas-controls" role="group" aria-label="Focus graph controls">
                        <button className="ui-action-btn" onClick={viewport.zoomOut} aria-label="Zoom out">-</button>
                        <button className="ui-action-btn" onClick={viewport.zoomIn} aria-label="Zoom in">+</button>
                        <button className="ui-action-btn" onClick={viewport.reset}>Reset</button>
                        <span className="graph-canvas-zoom">{Math.round(viewport.zoom * 100)}%</span>
                    </div>
                </div>
            </div>

            <div className="focus-mode-tabs" style={{ marginBottom: 8 }}>
                {FOCUS_MODES.map((mode) => (
                    <button
                        key={mode.key}
                        className={`ui-action-btn ${focusMode === mode.key ? "active" : ""}`}
                        onClick={() => onFocusModeChange(mode.key)}
                    >
                        {mode.label}
                    </button>
                ))}
            </div>

            <div className="focus-view-layout">
                <div className="focus-map-surface">
                    <svg
                        viewBox="0 0 980 430"
                        className={`focus-map-svg ${viewport.isDragging ? "dragging" : ""}`}
                        onWheel={viewport.onWheel}
                        onMouseDown={viewport.onMouseDown}
                        onMouseMove={viewport.onMouseMove}
                        onMouseUp={viewport.onMouseUp}
                        onMouseLeave={viewport.onMouseLeave}
                    >
                        <g transform={viewport.transform}>
                            <ellipse className="focus-ring" cx={490} cy={215} rx={130} ry={105} />
                            <ellipse className="focus-ring" cx={490} cy={215} rx={200} ry={164} />

                            {visibleEdges.map((edge) => {
                                const source = positions.get(edge.source);
                                const target = positions.get(edge.target);
                                if (!source || !target) {
                                    return null;
                                }

                                const isCenterEdge = edge.source === centerId || edge.target === centerId;
                                return (
                                    <line
                                        key={`focus-edge-${edge.id}`}
                                        x1={source.x}
                                        y1={source.y}
                                        x2={target.x}
                                        y2={target.y}
                                        stroke={isCenterEdge ? "rgba(160, 160, 160, 0.62)" : "rgba(120, 120, 120, 0.3)"}
                                        strokeWidth={isCenterEdge ? 1.2 : 0.9}
                                        opacity={isCenterEdge ? 0.82 : 0.5}
                                    />
                                );
                            })}

                            {[...neighborhood.displayNodeIds].map((nodeId) => {
                                const node = nodeMap.get(nodeId);
                                const pos = positions.get(nodeId);
                                if (!node || !pos) {
                                    return null;
                                }

                                const meta = NODE_TYPE_META[node.node_type];
                                const radius = pos.ring === 0 ? 14 : pos.ring === 1 ? 10 : 7;
                                const cls = pos.ring === 0 ? "center" : pos.ring === 2 ? "secondary" : "direct";

                                return (
                                    <g
                                        key={`focus-node-${node.id}`}
                                        className={`focus-node-g ${cls}`}
                                        onClick={() => onOpenFocus(node.id)}
                                    >
                                        <circle
                                            cx={pos.x}
                                            cy={pos.y}
                                            r={radius}
                                            fill={pos.ring === 0 ? `${meta.color}45` : `${meta.color}1f`}
                                            stroke={meta.color}
                                            strokeWidth={pos.ring === 0 ? 1.8 : 1}
                                        />
                                        <text x={pos.x} y={pos.y + 3} textAnchor="middle" className="focus-short">
                                            {meta.short}
                                        </text>
                                        <text x={pos.x} y={pos.y + radius + 11} textAnchor="middle" className="focus-label">
                                            {shorten(node.label, 20)}
                                        </text>
                                    </g>
                                );
                            })}
                        </g>
                    </svg>
                </div>

                <aside className="focus-panel">
                    <div className="inspector-title-row">
                        <strong>{center ? shorten(center.label, 42) : "Selection"}</strong>
                        <span>{center ? describeNode(center) : ""}</span>
                    </div>

                    <section className="inspector-section">
                        <div className="inspector-section-label">Direct</div>
                        <div className="inspector-row-list">
                            {directNodes.map(({ node, neighbor }) => (
                                <button key={`direct-${node.id}`} className="inspector-row-btn" onClick={() => onOpenFocus(node.id)}>
                                    <strong>{shorten(node.label, 74)}</strong>
                                    <small>{neighbor.reasons.join(" • ")}</small>
                                </button>
                            ))}
                        </div>
                    </section>

                    <section className="inspector-section">
                        <div className="inspector-section-label">Secondary</div>
                        <div className="inspector-row-list">
                            {secondaryNodes.slice(0, 7).map(({ node, neighbor }) => (
                                <button key={`secondary-${node.id}`} className="inspector-row-btn" onClick={() => onOpenFocus(node.id)}>
                                    <strong>{shorten(node.label, 74)}</strong>
                                    <small>{neighbor.reasons.join(" • ")}</small>
                                </button>
                            ))}
                        </div>
                    </section>

                    <section className="inspector-section">
                        <div className="inspector-section-label">Reasons</div>
                        <div className="inspector-row-list">
                            {relationshipReasons.length > 0
                                ? relationshipReasons.map((reason) => (
                                    <div className="inspector-row" key={`reason-${reason}`}>{reason}</div>
                                ))
                                : <div className="inspector-row">No explicit reasons.</div>}
                        </div>
                    </section>

                    <section className="inspector-section">
                        <div className="inspector-section-label">Why Connected</div>
                        <div className="inspector-row-list">
                            {explainDirect.length > 0
                                ? explainDirect.map(({ node, neighbor }) => {
                                    const viaNode = neighbor.viaNodeId ? nodeMap.get(neighbor.viaNodeId) : null;
                                    return (
                                        <div key={`why-${node.id}`} className="inspector-row">
                                            <strong>{shorten(node.label, 62)}</strong>
                                            <span>{neighbor.edgeType ? `${neighbor.edgeType.toLowerCase()} • ` : ""}{neighbor.reasons.join(" • ")}</span>
                                            {viaNode && <small>via: {shorten(viaNode.label, 44)}</small>}
                                        </div>
                                    );
                                })
                                : <div className="inspector-row">Select a center node to inspect reasons.</div>}
                        </div>
                    </section>

                    <section className="inspector-section">
                        <div className="inspector-section-label">Similar</div>
                        <div className="inspector-row-list">
                            {similarMemories.length > 0
                                ? similarMemories.map(({ node }) => (
                                    <button key={`similar-${node.id}`} className="inspector-row-btn" onClick={() => onOpenFocus(node.id)}>
                                        <strong>{shorten(node.label, 68)}</strong>
                                        <small>{formatDateTime(node.created_at)}</small>
                                    </button>
                                ))
                                : <div className="inspector-row">No close matches.</div>}
                        </div>
                    </section>

                    <section className="inspector-section">
                        <div className="inspector-section-label">Follow-up</div>
                        <div className="inspector-row-list">
                            {followUps.length > 0
                                ? followUps.map(({ node }) => (
                                    <button key={`follow-${node.id}`} className="inspector-row-btn" onClick={() => onOpenFocus(node.id)}>
                                        <strong>{shorten(node.label, 72)}</strong>
                                        <small>{formatDateTime(node.created_at)}</small>
                                    </button>
                                ))
                                : <div className="inspector-row">No downstream nodes.</div>}
                        </div>
                    </section>
                </aside>
            </div>
        </div>
    );
}
