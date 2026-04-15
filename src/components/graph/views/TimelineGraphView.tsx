import { useMemo, useState } from "react";
import { buildTimelineWaveform, curvedConnectionPath } from "../graphLayouts";
import { TimelineInsight, TypedGraphNode } from "../graphTypes";
import {
    describeNode,
    formatTime,
    memoryTypeForNode,
    shorten,
} from "../graphViewConstants";
import { useViewport } from "../useViewport";

interface TimelineGraphViewProps {
    timelineInsight: TimelineInsight;
    nodeMap: Map<string, TypedGraphNode>;
    adjacency: Map<string, Set<string>>;
    relationSignalsByMemory: Map<string, string[]>;
    onOpenFocus: (id: string) => void;
}

function segmentLabelShort(label: TimelineInsight["segments"][number]["label"]): string {
    switch (label) {
        case "implementation":
            return "impl";
        case "exploration":
            return "explore";
        case "refinement":
            return "refine";
        case "validation":
            return "validate";
        case "drift":
            return "drift";
        case "return":
            return "return";
        default:
            return label;
    }
}

export function TimelineGraphView({
    timelineInsight,
    nodeMap,
    adjacency,
    relationSignalsByMemory,
    onOpenFocus,
}: TimelineGraphViewProps) {
    const viewport = useViewport({ minZoom: 0.7, maxZoom: 2.4, step: 0.1 });
    const [selectedSegmentId, setSelectedSegmentId] = useState<string | null>(timelineInsight.segments[0]?.id ?? null);
    const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

    const waveform = useMemo(
        () => buildTimelineWaveform(timelineInsight.segments, 980, 168),
        [timelineInsight.segments]
    );

    const segmentById = new Map(timelineInsight.segments.map((segment) => [segment.id, segment]));
    const selectedSegment = selectedSegmentId ? segmentById.get(selectedSegmentId) : null;

    const visibleGroups = timelineInsight.groupedMemories.filter((group) => {
        if (!selectedSegment) {
            return true;
        }
        return group.memoryIds.some((memoryId) => selectedSegment.memoryIds.includes(memoryId));
    });

    const pivotMap = new Map(timelineInsight.pivots.map((pivot) => [pivot.memoryId, pivot.reason]));

    return (
        <div className="graph-view timeline-view">
            <div className="view-header row">
                <div>
                    <h3>Timeline</h3>
                    <p>Chronological narrative with phase focus and dependency hints.</p>
                </div>
                <div className="view-header-controls">
                    <div className="timeline-segment-strip">
                        {timelineInsight.segments.map((segment) => (
                            <button
                                key={`segment-chip-${segment.id}`}
                                className={`ui-action-btn ${selectedSegmentId === segment.id ? "active" : ""}`}
                                onClick={() => setSelectedSegmentId(segment.id)}
                            >
                                {segment.label}
                            </button>
                        ))}
                    </div>
                    <div className="graph-canvas-controls" role="group" aria-label="Timeline controls">
                        <button className="ui-action-btn" onClick={viewport.zoomOut} aria-label="Zoom out">-</button>
                        <button className="ui-action-btn" onClick={viewport.zoomIn} aria-label="Zoom in">+</button>
                        <button className="ui-action-btn" onClick={viewport.reset}>Reset</button>
                        <span className="graph-canvas-zoom">{Math.round(viewport.zoom * 100)}%</span>
                    </div>
                </div>
            </div>

            {selectedSegment && (
                <div className="timeline-header-row">
                    <p>Active phase: {selectedSegment.label}</p>
                    <p>{selectedSegment.memoryIds.length} memories</p>
                </div>
            )}

            <div className="timeline-spine-surface" style={{ marginBottom: 8 }}>
                <svg
                    viewBox="0 0 980 168"
                    className={`timeline-spine-svg ${viewport.isDragging ? "dragging" : ""}`}
                    onWheel={viewport.onWheel}
                    onMouseDown={viewport.onMouseDown}
                    onMouseMove={viewport.onMouseMove}
                    onMouseUp={viewport.onMouseUp}
                    onMouseLeave={viewport.onMouseLeave}
                >
                    <g transform={viewport.transform}>
                        <path d={waveform.areaPath} fill="rgba(160, 160, 160, 0.06)" />
                        <path d={waveform.linePath} fill="none" stroke="rgba(160, 160, 160, 0.68)" strokeWidth={1.4} />

                        {waveform.points.map((point, index) => {
                            const segment = timelineInsight.segments[index];
                            const isActive = selectedSegmentId === segment.id;

                            const prev = waveform.points[index - 1];
                            const edgePath = prev ? curvedConnectionPath(prev, point, 0.06) : "";

                            return (
                                <g key={`wave-point-${segment.id}`} className="timeline-wave-dot" onClick={() => setSelectedSegmentId(segment.id)}>
                                    {prev && (
                                        <path
                                            d={edgePath}
                                            fill="none"
                                            stroke="rgba(120, 120, 120, 0.2)"
                                            strokeWidth={0.75}
                                        />
                                    )}
                                    <circle cx={point.x} cy={point.y} r={isActive ? 5.3 : 4.2} />
                                    <text x={point.x} y={point.y - 10} textAnchor="middle">
                                        {segmentLabelShort(segment.label)}
                                    </text>
                                </g>
                            );
                        })}
                    </g>
                </svg>
            </div>

            <div className="timeline-items-stack">
                {visibleGroups.map((group) => {
                    const representative = nodeMap.get(group.representativeId);
                    if (!representative) {
                        return null;
                    }

                    const isExpanded = expandedGroups.has(group.id) || !group.isCollapsedByDefault;
                    const memoryIds = isExpanded ? group.memoryIds : [group.representativeId];

                    return (
                        <article className="timeline-group" key={`timeline-group-${group.id}`}>
                            <button
                                className="timeline-group-header"
                                onClick={() => {
                                    setExpandedGroups((current) => {
                                        const next = new Set(current);
                                        if (next.has(group.id)) {
                                            next.delete(group.id);
                                        } else {
                                            next.add(group.id);
                                        }
                                        return next;
                                    });
                                }}
                            >
                                <div>
                                    <strong>{shorten(representative.label, 80)}</strong>
                                    <small>{describeNode(representative)}</small>
                                </div>
                                <small>{group.memoryIds.length}</small>
                            </button>

                            <div className="timeline-items-stack">
                                {memoryIds.map((memoryId) => {
                                    const memory = nodeMap.get(memoryId);
                                    if (!memory) {
                                        return null;
                                    }

                                    const relationSignals = relationSignalsByMemory.get(memory.id) ?? [];

                                    const neighbors = [...(adjacency.get(memory.id) ?? new Set())]
                                        .map((id) => nodeMap.get(id))
                                        .filter((node): node is TypedGraphNode => Boolean(node));

                                    const taskCount = neighbors.filter((node) => node.node_type === "Task").length;

                                    return (
                                        <div className="timeline-item-card" key={`memory-card-${memory.id}`}>
                                            <button onClick={() => onOpenFocus(memory.id)}>
                                                <div className="timeline-item-head">
                                                    <span>{formatTime(memory.created_at)}</span>
                                                    <span>{memoryTypeForNode(memory)}</span>
                                                </div>
                                                <h6>{shorten(memory.label, 168)}</h6>
                                            </button>

                                            <div className="timeline-meta-line">
                                                <span>{taskCount} tasks</span>
                                                {relationSignals[0] && <span>{relationSignals[0]}</span>}
                                                {pivotMap.has(memory.id) && <span className="pivot">pivot: {pivotMap.get(memory.id)}</span>}
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        </article>
                    );
                })}
            </div>
        </div>
    );
}
