import { useEffect, useState } from "react";
import type {
    EvidencePack,
    MemoryCard,
} from "../../shared/ipc/tauri";
import { fndrGetMemorySubgraph, fndrGetRelatedMemories } from "../../shared/ipc/tauri";
import { CopyForAgentButton } from "./CopyForAgentButton";
import { SurfacingReason } from "./SurfacingReason";

interface Props {
    card: MemoryCard;
    evidence?: EvidencePack | null;
    onClose: () => void;
}

/**
 * Phase 5 — expanded view of a memory card. Shows the underlying evidence
 * (files / decisions / commands / errors / todos / urls), a subgraph
 * placeholder, and quick actions (Open, Related, Copy for Agent).
 */
export function ExpandedMemoryCard({ card, evidence, onClose }: Props) {
    const [related, setRelated] = useState<MemoryCard[]>([]);
    const [subgraph, setSubgraph] = useState<{ node_count: number; edge_count: number } | null>(
        null,
    );

    useEffect(() => {
        let cancelled = false;
        void fndrGetRelatedMemories(card.id, 4).then((cards) => {
            if (!cancelled) setRelated(cards);
        });
        void fndrGetMemorySubgraph([card.id], 2).then((sub) => {
            if (!cancelled) setSubgraph({ node_count: sub.node_count, edge_count: sub.edge_count });
        });
        return () => {
            cancelled = true;
        };
    }, [card.id]);

    return (
        <div
            role="dialog"
            aria-label={`Expanded memory: ${card.title}`}
            style={{
                position: "fixed",
                inset: 0,
                background: "rgba(0,0,0,0.45)",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                zIndex: 200,
            }}
            onClick={onClose}
        >
            <div
                onClick={(e) => e.stopPropagation()}
                style={{
                    background: "#FAF9F6",
                    color: "#3E2723",
                    width: "min(720px, 92vw)",
                    maxHeight: "82vh",
                    overflowY: "auto",
                    borderRadius: 12,
                    padding: 24,
                    boxShadow: "0 24px 64px rgba(0,0,0,0.35)",
                }}
            >
                <header style={{ display: "flex", justifyContent: "space-between", marginBottom: 8 }}>
                    <h2 style={{ margin: 0, fontSize: 18 }}>{card.title}</h2>
                    <button
                        type="button"
                        onClick={onClose}
                        aria-label="Close"
                        style={{ background: "transparent", border: "none", fontSize: 18, cursor: "pointer" }}
                    >
                        ×
                    </button>
                </header>
                {card.surfacing_reason && <SurfacingReason reason={card.surfacing_reason} />}
                <p style={{ marginTop: 12 }}>{card.summary}</p>
                {evidence && (
                    <section style={{ marginTop: 16 }}>
                        <h3 style={{ fontSize: 13, opacity: 0.7 }}>Evidence</h3>
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
                )}
                <section style={{ marginTop: 16 }}>
                    <h3 style={{ fontSize: 13, opacity: 0.7 }}>Subgraph</h3>
                    <p style={{ fontSize: 12 }} data-testid="fndr-subgraph-summary">
                        {subgraph
                            ? `${subgraph.node_count} nodes · ${subgraph.edge_count} edges`
                            : "Loading subgraph…"}
                    </p>
                </section>
                {related.length > 0 && (
                    <section style={{ marginTop: 16 }}>
                        <h3 style={{ fontSize: 13, opacity: 0.7 }}>Related memories</h3>
                        <ul style={{ paddingLeft: 18, margin: 0 }}>
                            {related.map((r) => (
                                <li key={r.id} style={{ marginBottom: 4 }}>
                                    {r.title}
                                </li>
                            ))}
                        </ul>
                    </section>
                )}
                <footer style={{ marginTop: 20, display: "flex", gap: 8 }}>
                    {card.reopen_target && (
                        <a
                            href={card.reopen_target}
                            target="_blank"
                            rel="noreferrer"
                            style={primaryButtonStyle}
                        >
                            Open
                        </a>
                    )}
                    <CopyForAgentButton query={card.title} />
                </footer>
            </div>
        </div>
    );
}

function EvidenceList({ label, items }: { label: string; items: string[] }) {
    if (items.length === 0) return null;
    return (
        <div style={{ marginBottom: 8 }}>
            <strong style={{ fontSize: 12 }}>{label}:</strong>{" "}
            <span style={{ fontSize: 12 }}>{items.slice(0, 5).join(", ")}</span>
            {items.length > 5 && (
                <span style={{ fontSize: 11, opacity: 0.6 }}> +{items.length - 5} more</span>
            )}
        </div>
    );
}

const primaryButtonStyle: React.CSSProperties = {
    padding: "8px 14px",
    background: "#E65100",
    color: "#FAF9F6",
    borderRadius: 8,
    textDecoration: "none",
    border: "none",
    fontSize: 13,
};
