import type { MemoryCard } from "@/shared/ipc/tauri";
import { isMetaOcrNarration } from "./MemoryCard";
import "./InsightLayers.css";

/** Insight slots stay in a consistent order when they have useful content. */
const SLOTS = [
    { label: "What happened", field: "insight_what_happened" },
    { label: "Why it mattered", field: "insight_why_mattered" },
    { label: "What changed", field: "insight_what_changed" },
    { label: "Thread", field: "insight_context_thread" },
] as const;

/** Empty or rejected insight rows stay out of the memory card. */
export function InsightLayers({ card, evalUi = false }: { card: MemoryCard; evalUi?: boolean }) {
    const ic = card.insight_card_confidence ?? 0;
    const low = ic > 0 && ic < 0.4;

    const categories = card.topic_categories?.filter((c) => c.trim()) ?? [];

    return (
        <div className={`insight-layers${low ? " insight-layers--low-conf" : ""}`}>
            {low && <div className="insight-meta-row"><div className="insight-low-badge">Low insight confidence</div></div>}
            {SLOTS.map((slot) => {
                const raw = (card as unknown as Record<string, unknown>)[slot.field];
                const trimmed = typeof raw === "string" ? raw.trim() : "";
                // Strip meta-OCR narration ("The OCR text indicates…") so the
                // panel never dresses up a noisy raw extraction as an insight.
                const value = trimmed && !isMetaOcrNarration(trimmed) ? trimmed : "";
                if (!value) return null;
                return (
                    <div
                        className="insight-row"
                        key={slot.label}
                    >
                        <span className="insight-label">{slot.label}</span>
                        <p className="insight-value">{value}</p>
                    </div>
                );
            })}
            {categories.length > 0 && (
                <div className="insight-categories">
                    {categories.slice(0, 6).map((c) => (
                        <span className="insight-category-chip" key={c}>
                            {c}
                        </span>
                    ))}
                </div>
            )}
            {evalUi && card.insight_spans_json?.trim() && (
                <details className="insight-spans-debug" onClick={(e) => e.stopPropagation()}>
                    <summary>Salience spans (debug)</summary>
                    <pre className="insight-spans-pre">{card.insight_spans_json}</pre>
                </details>
            )}
        </div>
    );
}
