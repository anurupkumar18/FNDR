import { useState } from "react";
import type { SurfacingReason as SurfacingReasonType } from "../../shared/ipc/tauri";

interface Props {
    reason: SurfacingReasonType;
}

/**
 * Phase 5 — "Why this surfaced" chip rendered under a MemoryCard title.
 * Single-line headline with a click/hover tooltip that exposes the route
 * mix, the graph path (when present), and the anchor terms that matched.
 *
 * Styled off the shared design tokens (--fg/--bg/--accent) so it follows
 * the active theme and palette instead of a fixed color.
 */
export function SurfacingReason({ reason }: Props) {
    const [open, setOpen] = useState(false);
    if (!reason || !reason.headline) {
        return null;
    }

    return (
        <span
            className="fndr-surfacing-reason"
            style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                padding: "2px 8px",
                marginTop: 4,
                borderRadius: 999,
                fontSize: 11,
                lineHeight: "16px",
                fontFamily: "var(--font-body)",
                color: "var(--fg-2)",
                background: "var(--surface-translucent, var(--bg-2))",
                border: "1px solid var(--border)",
                cursor: reason.routes.length > 0 ? "help" : "default",
                position: "relative",
            }}
            title={[
                `Routes: ${reason.routes.join(", ") || "—"}`,
                reason.graph_path && reason.graph_path.length > 0
                    ? `Path: ${reason.graph_path
                          .map((s) => `${s.from_label} —${s.edge}→ ${s.to_label}`)
                          .join(" / ")}`
                    : null,
                reason.anchor_terms_hit && reason.anchor_terms_hit.length > 0
                    ? `Anchors: ${reason.anchor_terms_hit.join(", ")}`
                    : null,
            ]
                .filter(Boolean)
                .join("\n")}
            onMouseEnter={() => setOpen(true)}
            onMouseLeave={() => setOpen(false)}
            onClick={() => setOpen((v) => !v)}
            data-testid="fndr-surfacing-reason"
        >
            <span
                style={{
                    width: 6,
                    height: 6,
                    borderRadius: 999,
                    background: "var(--accent)",
                    display: "inline-block",
                }}
            />
            <span>{reason.headline}</span>
            {open && reason.routes.length > 0 && (
                <span
                    role="tooltip"
                    style={{
                        position: "absolute",
                        top: "100%",
                        left: 0,
                        marginTop: 6,
                        padding: "6px 10px",
                        background: "var(--bg-3)",
                        color: "var(--fg)",
                        border: "1px solid var(--border)",
                        borderRadius: "var(--radius-md, 8px)",
                        fontSize: 11,
                        fontFamily: "var(--font-body)",
                        whiteSpace: "nowrap",
                        zIndex: 50,
                        boxShadow: "0 8px 24px var(--shadow-color, rgba(0,0,0,0.25))",
                    }}
                >
                    {reason.routes.join(" + ")}
                </span>
            )}
        </span>
    );
}
