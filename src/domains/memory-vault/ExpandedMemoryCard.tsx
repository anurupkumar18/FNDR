import { useEffect, useRef, useState, type ReactNode } from "react";
import type {
    EvidencePack,
    MemoryCard as MemoryCardData,
} from "../../shared/ipc/tauri";
import { fndrGetMemorySubgraph, fndrGetRelatedMemories } from "../../shared/ipc/tauri";
import { CopyForAgentButton } from "./CopyForAgentButton";
import { SurfacingReason } from "./SurfacingReason";
import { MemoryCard } from "./MemoryCard";
import "./ExpandedMemoryCard.css";

interface Props {
    card: MemoryCardData;
    evidence?: EvidencePack | null;
    /** Optional slots — passed straight through to <MemoryCard variant="expanded">. */
    insightsSlot?: ReactNode;
    debugSlot?: ReactNode;
    similarSlot?: ReactNode;
    onClose: () => void;
    onDelete?: (id: string) => boolean | void | Promise<boolean | void>;
    onOpenRelated?: (id: string) => void;
    onOpenInGraph?: (card: MemoryCardData) => void;
    onReopen?: (card: MemoryCardData) => void;
    onResearch?: (card: MemoryCardData) => void;
}

/**
 * Modal shell around `<MemoryCard variant="expanded">`. The card itself
 * handles dossier corners, frame strip, provenance, threads, and action
 * buttons; this component supplies the modal scrim and stitches in the
 * dynamically-loaded extras (related memories, subgraph, evidence pack,
 * insight layers).
 */
export function ExpandedMemoryCard({
    card,
    evidence,
    insightsSlot,
    debugSlot,
    similarSlot,
    onClose,
    onDelete,
    onOpenRelated,
    onOpenInGraph,
    onReopen,
    onResearch,
}: Props) {
    const [related, setRelated] = useState<MemoryCardData[]>([]);
    const [subgraph, setSubgraph] = useState<{ node_count: number; edge_count: number } | null>(null);
    const [relatedState, setRelatedState] = useState<"loading" | "ready" | "error">("loading");
    const [subgraphState, setSubgraphState] = useState<"loading" | "ready" | "error">("loading");
    const closeButtonRef = useRef<HTMLButtonElement>(null);

    useEffect(() => {
        let cancelled = false;
        setRelated([]);
        setSubgraph(null);
        setRelatedState("loading");
        setSubgraphState("loading");
        void fndrGetRelatedMemories(card.id, 4)
            .then((cards) => {
                if (!cancelled) {
                    setRelated(cards);
                    setRelatedState("ready");
                }
            })
            .catch(() => {
                if (!cancelled) setRelatedState("error");
            });
        void fndrGetMemorySubgraph([card.id], 2)
            .then((sub) => {
                if (!cancelled) {
                    setSubgraph({ node_count: sub.node_count, edge_count: sub.edge_count });
                    setSubgraphState("ready");
                }
            })
            .catch(() => {
                if (!cancelled) setSubgraphState("error");
            });
        return () => {
            cancelled = true;
        };
    }, [card.id]);

    useEffect(() => {
        const previouslyFocused = document.activeElement as HTMLElement | null;
        closeButtonRef.current?.focus();
        return () => previouslyFocused?.focus();
    }, [card.id]);

    useEffect(() => {
        const handler = (e: KeyboardEvent) => {
            if (e.key === "Escape") {
                e.preventDefault();
                e.stopImmediatePropagation();
                onClose();
            }
        };
        window.addEventListener("keydown", handler, true);
        return () => window.removeEventListener("keydown", handler, true);
    }, [onClose]);

    const reasonNode = card.surfacing_reason ? (
        <SurfacingReason reason={card.surfacing_reason} />
    ) : undefined;

    const evidenceNode =
        evidence ? (
            <section>
                <h4 className="fndr-emc-section-heading">Evidence</h4>
                <EvidenceList label="Files" items={evidence.files.map((f) => f.path)} />
                <EvidenceList
                    label="Decisions"
                    items={evidence.decisions.map((d) => d.decision)}
                />
                <EvidenceList
                    label="Commands"
                    items={evidence.commands.map((c) => c.command)}
                />
                <EvidenceList label="Errors" items={evidence.errors.map((e) => e.error)} />
                <EvidenceList label="Todos" items={evidence.todos.map((t) => t.task)} />
                <EvidenceList label="URLs" items={evidence.urls.map((u) => u.url)} />
            </section>
        ) : undefined;

    const chunkEvidenceNode =
        Array.isArray(card.chunk_evidence) && card.chunk_evidence.length > 0 ? (
            <section>
                <h4 className="fndr-emc-section-heading">Matched chunks</h4>
                <ul className="fndr-emc-chunks">
                    {card.chunk_evidence.slice(0, 3).map((chunk) => (
                        <li key={chunk.chunk_id}>
                            <span className="fndr-emc-chunk-meta">
                                Chunk {chunk.chunk_index + 1}
                                {Number.isFinite(chunk.score)
                                    ? ` · ${(chunk.score * 100).toFixed(0)}%`
                                    : ""}
                            </span>
                            <span>{chunk.text}</span>
                        </li>
                    ))}
                </ul>
            </section>
        ) : undefined;

    const subgraphNode = (
        <section>
            <h4 className="fndr-emc-section-heading">Connected context</h4>
            <p className="fndr-emc-meta" data-testid="fndr-subgraph-summary">
                {subgraphState === "error"
                    ? "Graph context unavailable."
                    : subgraphState === "loading"
                    ? "Loading connected context…"
                    : subgraph
                    ? `${subgraph.node_count} nodes · ${subgraph.edge_count} edges`
                    : "No connected context found."}
            </p>
        </section>
    );

    const relatedNode = (
        <section>
            <h4 className="fndr-emc-section-heading">Related memories</h4>
            {relatedState === "error" ? (
                <p className="fndr-emc-meta">Related memories unavailable.</p>
            ) : relatedState === "loading" ? (
                <p className="fndr-emc-meta">Finding related memories…</p>
            ) : related.length > 0 ? (
                <ul className="fndr-emc-related">
                    {related.map((r) => (
                        <li key={r.id}>
                            {onOpenRelated ? (
                                <button
                                    type="button"
                                    onClick={() => onOpenRelated(r.id)}
                                    aria-label={`Open related memory: ${r.title}`}
                                >
                                    {r.title}
                                </button>
                            ) : (
                                r.title
                            )}
                        </li>
                    ))}
                </ul>
            ) : (
                <p className="fndr-emc-meta">No related memories found.</p>
            )}
        </section>
    );

    const combinedInsights = (
        <>
            {reasonNode}
            {insightsSlot}
        </>
    );

    const combinedEvidence = (
        <>
            {chunkEvidenceNode}
            {evidenceNode}
            {subgraphNode}
            {relatedNode}
        </>
    );

    return (
        <div
            role="dialog"
            aria-modal="true"
            aria-label={`Expanded memory: ${card.title}`}
            className="fndr-emc-overlay"
            onClick={onClose}
        >
            <div
                className="fndr-emc-shell"
                onClick={(e) => e.stopPropagation()}
            >
                <button
                    ref={closeButtonRef}
                    type="button"
                    onClick={onClose}
                    aria-label="Close memory details"
                    className="fndr-emc-close"
                >
                    ×
                </button>
                <MemoryCard
                    card={card}
                    variant="expanded"
                    insightsSlot={combinedInsights}
                    evidenceSlot={combinedEvidence}
                    relatedSlot={
                        <div className="fndr-emc-extra-actions">
                            <CopyForAgentButton card={card} />
                        </div>
                    }
                    footerSlot={
                        debugSlot || similarSlot ? (
                            <div className="fndr-emc-drawers">
                                {debugSlot}
                                {similarSlot}
                            </div>
                        ) : null
                    }
                    onDelete={onDelete}
                    onOpenInGraph={onOpenInGraph}
                    onReopen={onReopen}
                    onResearch={onResearch}
                />
            </div>
        </div>
    );
}

function EvidenceList({ label, items }: { label: string; items: string[] }) {
    if (items.length === 0) return null;
    return (
        <div className="fndr-emc-evidence-row">
            <strong>{label}:</strong>{" "}
            <span>{items.slice(0, 5).join(", ")}</span>
            {items.length > 5 && (
                <span className="fndr-emc-evidence-more"> +{items.length - 5} more</span>
            )}
        </div>
    );
}
