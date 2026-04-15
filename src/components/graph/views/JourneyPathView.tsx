import { useEffect, useState } from "react";
import { GraphNodeData } from "../../../api/tauri";
import { curvedConnectionPath } from "../graphLayouts";
import { EdgePairIndex, JourneySemantics, TypedGraphNode } from "../graphTypes";
import {
    EDGE_TYPE_LABELS,
    describeNode,
    formatDateTime,
    shorten,
} from "../graphViewConstants";
import { useViewport } from "../useViewport";

interface JourneyPathViewProps {
    journeyOptions: TypedGraphNode[];
    journeyStartId: string;
    journeyEndId: string;
    onJourneyStartChange: (id: string) => void;
    onJourneyEndChange: (id: string) => void;
    journeyPath: string[];
    journeySemantics: JourneySemantics;
    journeyLayout: {
        points: Array<{ id: string; x: number; y: number; branch: boolean }>;
        edges: Array<{ source: string; target: string; branch: boolean }>;
        branchCandidatesByPathId: Map<string, string[]>;
    };
    nodeMap: Map<string, TypedGraphNode>;
    edgeByPair: EdgePairIndex;
    timelineBridge: GraphNodeData[];
    onOpenFocus: (id: string) => void;
}

export function JourneyPathView({
    journeyOptions,
    journeyStartId,
    journeyEndId,
    onJourneyStartChange,
    onJourneyEndChange,
    journeyPath,
    journeySemantics,
    journeyLayout,
    nodeMap,
    edgeByPair,
    timelineBridge,
    onOpenFocus,
}: JourneyPathViewProps) {
    const viewport = useViewport({ minZoom: 0.65, maxZoom: 2.3, step: 0.12 });
    const [draftStartId, setDraftStartId] = useState(journeyStartId);
    const [draftEndId, setDraftEndId] = useState(journeyEndId);
    const [selectedBranchParent, setSelectedBranchParent] = useState<string | null>(null);

    const selectedBranchCandidates = selectedBranchParent
        ? journeyLayout.branchCandidatesByPathId.get(selectedBranchParent) ?? []
        : [];

    useEffect(() => {
        setDraftStartId(journeyStartId);
    }, [journeyStartId]);

    useEffect(() => {
        setDraftEndId(journeyEndId);
    }, [journeyEndId]);

    return (
        <div className="graph-view journey-view">
            <div className="view-header row">
                <div>
                    <h3>Journey Path</h3>
                    <p>Set endpoints, generate route, then inspect bridge quality.</p>
                </div>
                <div className="graph-canvas-controls" role="group" aria-label="Journey map controls">
                    <button className="ui-action-btn" onClick={viewport.zoomOut} aria-label="Zoom out">-</button>
                    <button className="ui-action-btn" onClick={viewport.zoomIn} aria-label="Zoom in">+</button>
                    <button className="ui-action-btn" onClick={viewport.reset}>Reset</button>
                    <span className="graph-canvas-zoom">{Math.round(viewport.zoom * 100)}%</span>
                </div>
            </div>

            <div className="journey-controls">
                <label>
                    Start
                    <select value={draftStartId} onChange={(event) => setDraftStartId(event.target.value)}>
                        {journeyOptions.map((node) => (
                            <option key={node.id} value={node.id}>
                                {formatDateTime(node.created_at)} • {shorten(node.label, 70)}
                            </option>
                        ))}
                    </select>
                </label>

                <button
                    className="ui-action-btn"
                    onClick={() => {
                        setDraftStartId(draftEndId);
                        setDraftEndId(draftStartId);
                    }}
                >
                    Swap
                </button>

                <label>
                    End
                    <select value={draftEndId} onChange={(event) => setDraftEndId(event.target.value)}>
                        {journeyOptions.map((node) => (
                            <option key={node.id} value={node.id}>
                                {formatDateTime(node.created_at)} • {shorten(node.label, 70)}
                            </option>
                        ))}
                    </select>
                </label>

                <button
                    className="ui-action-btn generate-path-btn"
                    onClick={() => {
                        onJourneyStartChange(draftStartId);
                        onJourneyEndChange(draftEndId);
                    }}
                >
                    Generate Path
                </button>
            </div>

            <div className="journey-layout">
                <div>
                    {journeyLayout.points.length > 0 && (
                        <div className="journey-route-surface" style={{ marginBottom: 8 }}>
                            <svg
                                viewBox="0 0 980 250"
                                className={`journey-route-svg ${viewport.isDragging ? "dragging" : ""}`}
                                onWheel={viewport.onWheel}
                                onMouseDown={viewport.onMouseDown}
                                onMouseMove={viewport.onMouseMove}
                                onMouseUp={viewport.onMouseUp}
                                onMouseLeave={viewport.onMouseLeave}
                            >
                                <g transform={viewport.transform}>
                                    {journeyLayout.edges.map((edge, index) => {
                                        const source = journeyLayout.points.find((point) => point.id === edge.source);
                                        const target = journeyLayout.points.find((point) => point.id === edge.target);
                                        if (!source || !target) {
                                            return null;
                                        }

                                        return (
                                            <path
                                                key={`journey-route-edge-${index}`}
                                                d={curvedConnectionPath(source, target, edge.branch ? 0.2 : 0.1)}
                                                className={edge.branch ? "journey-branch-edge" : "journey-main-edge"}
                                            />
                                        );
                                    })}

                                    {journeySemantics.hops.slice(0, -1).map((hop) => {
                                        const next = journeySemantics.hops[hop.index + 1];
                                        const source = journeyLayout.points.find((point) => point.id === hop.nodeId);
                                        const target = next ? journeyLayout.points.find((point) => point.id === next.nodeId) : null;
                                        if (!source || !target) {
                                            return null;
                                        }

                                        const midX = (source.x + target.x) / 2;
                                        const midY = (source.y + target.y) / 2 - 12;
                                        return (
                                            <text key={`journey-hop-label-${hop.nodeId}`} className="journey-hop-label" x={midX} y={midY} textAnchor="middle">
                                                {hop.reason}
                                            </text>
                                        );
                                    })}

                                    {journeyLayout.points.map((point) => {
                                        const node = nodeMap.get(point.id);
                                        if (!node) {
                                            return null;
                                        }

                                        return (
                                            <g
                                                key={`journey-route-node-${point.id}-${point.branch ? "branch" : "main"}`}
                                                className={`journey-route-node ${point.branch ? "branch" : "main"}`}
                                                onClick={() => {
                                                    if (point.branch) {
                                                        const parentEdge = journeyLayout.edges.find((edge) => edge.target === point.id && edge.branch);
                                                        setSelectedBranchParent(parentEdge?.source ?? null);
                                                    } else {
                                                        onOpenFocus(point.id);
                                                    }
                                                }}
                                            >
                                                <circle cx={point.x} cy={point.y} r={point.branch ? 4 : 6} />
                                                <title>{node.label}</title>
                                            </g>
                                        );
                                    })}
                                </g>
                            </svg>
                        </div>
                    )}

                    {journeyPath.length > 0 ? (
                        <div className="journey-steps-list">
                            {journeySemantics.hops.map((hop) => {
                                const node = nodeMap.get(hop.nodeId);
                                if (!node) {
                                    return null;
                                }

                                const edge = edgeByPair[`${hop.nodeId}|${journeyPath[hop.index + 1]}`];

                                return (
                                    <div key={`journey-hop-row-${hop.nodeId}`} className="journey-step-row">
                                        <span className="role">step {hop.index + 1} • {hop.role}</span>
                                        <button onClick={() => onOpenFocus(hop.nodeId)}>
                                            <div className="title">{shorten(node.label, 90)}</div>
                                            <div className="meta">{describeNode(node)}</div>
                                        </button>
                                        <span className="reason">
                                            {hop.reason}
                                            {edge ? ` • ${EDGE_TYPE_LABELS[edge.edge_type] ?? edge.edge_type}` : ""}
                                        </span>
                                    </div>
                                );
                            })}
                        </div>
                    ) : (
                        <div className="journey-fallback">
                            <p>No graph route in current filter.</p>
                        </div>
                    )}
                </div>

                <aside className="journey-panel">
                    <h4>Alternate Branches</h4>
                    <div className="inspector-row">Click a small side node on the route to load alternates.</div>
                    <div className="inspector-row-list">
                        {selectedBranchParent && selectedBranchCandidates.length > 0 ? (
                            selectedBranchCandidates.slice(0, 8).map((candidateId) => {
                                const node = nodeMap.get(candidateId);
                                if (!node) {
                                    return null;
                                }
                                return (
                                    <button key={`alt-branch-${candidateId}`} className="inspector-row-btn" onClick={() => onOpenFocus(candidateId)}>
                                        <strong>{shorten(node.label, 84)}</strong>
                                        <small>{describeNode(node)}</small>
                                    </button>
                                );
                            })
                        ) : (
                            <div className="inspector-row">Click a branch node in the route to inspect alternates.</div>
                        )}
                    </div>

                    {timelineBridge.length > 0 && (
                        <section className="inspector-section">
                            <div className="inspector-section-label">Temporal Bridge</div>
                            <div className="inspector-row-list">
                                {timelineBridge.map((node) => (
                                    <button key={node.id} className="inspector-row-btn" onClick={() => onOpenFocus(node.id)}>
                                        <strong>{shorten(node.label, 86)}</strong>
                                        <small>{formatDateTime(node.created_at)}</small>
                                    </button>
                                ))}
                            </div>
                        </section>
                    )}
                </aside>
            </div>
        </div>
    );
}
