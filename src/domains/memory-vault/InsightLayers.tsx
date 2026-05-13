import type { MemoryCard } from "@/shared/ipc/tauri";
import "./InsightLayers.css";

function hasInsight(card: MemoryCard): boolean {
    return Boolean(
        card.insight_what_happened?.trim() ||
            card.insight_why_mattered?.trim() ||
            card.insight_what_changed?.trim() ||
            card.insight_context_thread?.trim(),
    );
}

/** Renders persisted insight rows when present; optional eval debug for span JSON. */
export function InsightLayers({ card, evalUi = false }: { card: MemoryCard; evalUi?: boolean }) {
    const ic = card.insight_card_confidence ?? 0;
    const low = ic > 0 && ic < 0.4;
    if (!hasInsight(card) && !low) {
        return null;
    }

    const rows: { label: string; value: string }[] = [];
    if (card.insight_what_happened?.trim()) {
        rows.push({ label: "What happened", value: card.insight_what_happened.trim() });
    }
    if (card.insight_why_mattered?.trim()) {
        rows.push({ label: "Why it mattered", value: card.insight_why_mattered.trim() });
    }
    if (card.insight_what_changed?.trim()) {
        rows.push({ label: "What changed", value: card.insight_what_changed.trim() });
    }
    if (card.insight_context_thread?.trim()) {
        rows.push({ label: "Thread", value: card.insight_context_thread.trim() });
    }

    return (
        <div className={`insight-layers${low ? " insight-layers--low-conf" : ""}`}>
            {low && <div className="insight-low-badge">Low insight confidence</div>}
            {rows.map((row) => (
                <div className="insight-row" key={row.label}>
                    <span className="insight-label">{row.label}</span>
                    <p className="insight-value">{row.value}</p>
                </div>
            ))}
            {evalUi && card.insight_spans_json?.trim() && (
                <details className="insight-spans-debug" onClick={(e) => e.stopPropagation()}>
                    <summary>Salience spans (debug)</summary>
                    <pre className="insight-spans-pre">{card.insight_spans_json}</pre>
                </details>
            )}
        </div>
    );
}
