import { useEffect, useState } from "react";
import { getNodeDetail, type InsightGraphNode } from "@/shared/ipc/tauri";
import type { GraphEdgeView, GraphNodeView, RelationshipReason } from "./graph/types";

export interface KnowledgeGraphSidePanelProps {
    selected: GraphNodeView | null;
    incidentEdges: GraphEdgeView[];
    nodeIndex: Map<string, GraphNodeView>;
    onSelectNode: (node: GraphNodeView) => void;
    onOpenContext?: (node: InsightGraphNode) => void;
    onFilterRelated?: (node: InsightGraphNode) => void;
    /** Optional async fetcher override (testing). Defaults to getNodeDetail. */
    fetchDetail?: (id: string) => Promise<InsightGraphNode | null>;
}

function metaField(node: InsightGraphNode, key: string): string | null {
    const md = node.metadata;
    if (md && typeof md === "object" && key in md) {
        const v = (md as Record<string, unknown>)[key];
        return typeof v === "string" && v.trim() ? v : null;
    }
    return null;
}

function previewFrom(detail: InsightGraphNode | null): string | null {
    if (!detail) return null;
    const preview = metaField(detail, "preview") ?? metaField(detail, "summary");
    return preview;
}

export function KnowledgeGraphSidePanel({
    selected,
    incidentEdges,
    nodeIndex,
    onSelectNode,
    onOpenContext,
    onFilterRelated,
    fetchDetail = getNodeDetail,
}: KnowledgeGraphSidePanelProps) {
    const [detail, setDetail] = useState<InsightGraphNode | null>(null);
    const [previewError, setPreviewError] = useState(false);

    useEffect(() => {
        let cancelled = false;
        setDetail(null);
        setPreviewError(false);
        if (!selected) return;
        fetchDetail(selected.id)
            .then((d) => {
                if (!cancelled) setDetail(d);
            })
            .catch(() => {
                if (!cancelled) setPreviewError(true);
            });
        return () => {
            cancelled = true;
        };
    }, [selected, fetchDetail]);

    if (!selected) {
        return (
            <aside className="kg-side-panel kg-side-panel-empty" aria-label="Memory card">
                <p className="kg-side-panel-empty-text">Pick a frame to follow its threads.</p>
            </aside>
        );
    }

    const project = metaField(selected.raw, "project");
    const topic = metaField(selected.raw, "topic");
    const source = metaField(selected.raw, "source") ?? selected.nodeType;
    const timestamp = new Date(selected.raw.created_at).toLocaleString();
    const preview = previewFrom(detail);

    return (
        <aside className="kg-side-panel" aria-label="Memory card">
            <header className="kg-side-panel-head">
                <span className="kg-stamp" aria-hidden="true">
                    FRAME · {selected.id.slice(0, 6).toUpperCase()}
                </span>
            </header>

            <h3 className="kg-side-panel-title">{selected.raw.label}</h3>
            <p className="kg-side-panel-meta">
                {source} · {timestamp}
            </p>

            {preview && <p className="kg-side-panel-preview">"{preview}"</p>}
            {!preview && previewError && (
                <p className="kg-side-panel-preview kg-side-panel-preview-muted">preview unavailable</p>
            )}

            {(project || topic || selected.nodeType) && (
                <section>
                    <p className="kg-side-panel-label">threads</p>
                    <div className="kg-side-panel-pills">
                        {project && <span className="kg-pill">{project}</span>}
                        {topic && <span className="kg-pill">{topic}</span>}
                        <span className="kg-pill">{selected.nodeType.toLowerCase()}</span>
                    </div>
                </section>
            )}

            <section>
                <p className="kg-side-panel-label">connections · {incidentEdges.length}</p>
                <ul className="kg-side-panel-connections">
                    {incidentEdges.map((edge) => {
                        const otherId = edge.sourceId === selected.id ? edge.targetId : edge.sourceId;
                        const other = nodeIndex.get(otherId);
                        if (!other) return null;
                        const reason: RelationshipReason | undefined = edge.reasons[0];
                        return (
                            <li
                                key={edge.id}
                                className={`kg-connection kg-connection-${edge.kind}`}
                                onClick={() => onSelectNode(other)}
                                role="button"
                                tabIndex={0}
                                onKeyDown={(ev) => {
                                    if (ev.key === "Enter" || ev.key === " ") {
                                        ev.preventDefault();
                                        onSelectNode(other);
                                    }
                                }}
                            >
                                <span className="kg-connection-label">{other.raw.label}</span>
                                {reason && (
                                    <span className={`kg-connection-reason kg-tone-${reason.tone}`}>
                                        {reason.text}
                                    </span>
                                )}
                            </li>
                        );
                    })}
                </ul>
            </section>

            <footer className="kg-side-panel-actions">
                <button
                    type="button"
                    className="kg-action"
                    onClick={() => onOpenContext?.(selected.raw)}
                    disabled={!onOpenContext}
                >
                    open
                </button>
                <button
                    type="button"
                    className="kg-action"
                    onClick={() => onFilterRelated?.(selected.raw)}
                    disabled={!onFilterRelated}
                >
                    filter related
                </button>
            </footer>
        </aside>
    );
}
