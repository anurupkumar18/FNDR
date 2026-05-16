import type { GraphLegendRow } from "./graph/types";

function Swatch({ row }: { row: GraphLegendRow }) {
    const { color, shape } = row.swatch;
    if (shape === "dash") {
        return (
            <span
                className="kg-legend-swatch kg-legend-swatch-dash"
                style={{ background: color }}
            />
        );
    }
    if (shape === "dot-dot") {
        return (
            <span className="kg-legend-swatch kg-legend-swatch-dotdot">
                <span style={{ background: color }} />
                <span style={{ background: color }} />
            </span>
        );
    }
    if (shape === "arrow") {
        return (
            <span className="kg-legend-swatch kg-legend-swatch-arrow" style={{ color }}>
                →
            </span>
        );
    }
    if (shape === "ring") {
        return (
            <span
                className="kg-legend-swatch kg-legend-swatch-ring"
                style={{ borderColor: color }}
            />
        );
    }
    return (
        <span className="kg-legend-swatch kg-legend-swatch-dot" style={{ background: color }} />
    );
}

export interface KnowledgeGraphLegendProps {
    rows: readonly GraphLegendRow[];
    collapsed: boolean;
    onToggle: () => void;
}

export function KnowledgeGraphLegend({ rows, collapsed, onToggle }: KnowledgeGraphLegendProps) {
    if (rows.length === 0) return null;
    return (
        <div
            className={`kg-legend${collapsed ? " kg-legend-collapsed" : ""}`}
            aria-label="Graph legend"
        >
            <button
                type="button"
                className="kg-legend-toggle"
                onClick={onToggle}
                aria-expanded={!collapsed}
            >
                {collapsed ? "legend ›" : "legend"}
            </button>
            {!collapsed && (
                <ul className="kg-legend-rows">
                    {rows.map((r, i) => (
                        <li
                            key={`${r.kind}-${i}-${r.label}`}
                            className={`kg-legend-row kg-legend-row-${r.kind}`}
                        >
                            <Swatch row={r} />
                            <span className="kg-legend-label">{r.label}</span>
                        </li>
                    ))}
                </ul>
            )}
        </div>
    );
}
