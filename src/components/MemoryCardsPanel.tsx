import { useEffect, useMemo, useState } from "react";
import { MemoryCard, listMemoryCards } from "../api/tauri";
import "./MemoryCardsPanel.css";

interface MemoryCardsPanelProps {
    isVisible: boolean;
    onClose: () => void;
    appNames: string[];
}

const APP_FILTER_ALL = "__all__";

function normalizeText(value: string | undefined | null): string {
    if (!value) {
        return "";
    }
    return value
        .replace(/[\u0000-\u001f\u007f-\u009f]/g, " ")
        .replace(/\s+/g, " ")
        .trim();
}

function hasReadableCharacters(value: string): boolean {
    return /[\p{L}\p{N}]/u.test(value);
}

function pickReadable(...candidates: Array<string | undefined | null>): string {
    for (const candidate of candidates) {
        const cleaned = normalizeText(candidate);
        if (cleaned && hasReadableCharacters(cleaned)) {
            return cleaned;
        }
    }
    return "";
}

function fallbackTitle(card: MemoryCard): string {
    return pickReadable(card.title, card.window_title)
        || `Memory in ${card.app_name}`;
}

function fallbackSummary(card: MemoryCard): string {
    return pickReadable(card.summary, card.raw_snippets[0], card.window_title)
        || `Captured context in ${card.app_name}.`;
}

function formatDay(timestamp: number): string {
    const date = new Date(timestamp);
    const today = new Date();
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    if (date.toDateString() === today.toDateString()) {
        return "Today";
    }
    if (date.toDateString() === yesterday.toDateString()) {
        return "Yesterday";
    }
    return date.toLocaleDateString(undefined, {
        weekday: "short",
        month: "short",
        day: "numeric",
    });
}

export function MemoryCardsPanel({ isVisible, onClose, appNames }: MemoryCardsPanelProps) {
    const [cards, setCards] = useState<MemoryCard[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [appFilter, setAppFilter] = useState<string>(APP_FILTER_ALL);

    const selectableApps = useMemo(() => {
        return appNames
            .map((name) => name.trim())
            .filter((name) => name.length > 0)
            .sort((a, b) => a.localeCompare(b));
    }, [appNames]);

    useEffect(() => {
        if (!isVisible) {
            return;
        }

        let cancelled = false;
        const selectedApp = appFilter === APP_FILTER_ALL ? undefined : appFilter;

        setLoading(true);
        setError(null);

        void listMemoryCards(1500, selectedApp)
            .then((items) => {
                if (cancelled) {
                    return;
                }
                setCards(items);
            })
            .catch((err) => {
                if (cancelled) {
                    return;
                }
                setCards([]);
                setError(err instanceof Error ? err.message : "Unable to load memory cards.");
            })
            .finally(() => {
                if (!cancelled) {
                    setLoading(false);
                }
            });

        return () => {
            cancelled = true;
        };
    }, [isVisible, appFilter]);

    if (!isVisible) {
        return null;
    }

    return (
        <div className="memory-cards-panel">
            <div className="memory-cards-header">
                <div className="memory-cards-heading">
                    <h2>All Memory Cards</h2>
                    <p>Newest to oldest</p>
                </div>
                <button className="ui-action-btn memory-cards-close-btn" onClick={onClose}>
                    ✕ Close
                </button>
            </div>

            <div className="memory-cards-toolbar">
                <label className="memory-cards-filter">
                    App
                    <select
                        value={appFilter}
                        onChange={(event) => setAppFilter(event.target.value)}
                    >
                        <option value={APP_FILTER_ALL}>All apps</option>
                        {selectableApps.map((name) => (
                            <option key={name} value={name}>
                                {name}
                            </option>
                        ))}
                    </select>
                </label>
                <div className="memory-cards-count">{cards.length} cards</div>
            </div>

            <div className="memory-cards-body">
                {loading && (
                    <div className="memory-cards-state">
                        <div className="spinner" />
                        <p>Loading memory cards...</p>
                    </div>
                )}

                {!loading && error && (
                    <div className="memory-cards-state">
                        <p>{error}</p>
                    </div>
                )}

                {!loading && !error && cards.length === 0 && (
                    <div className="memory-cards-state">
                        <p>No memory cards yet for this filter.</p>
                    </div>
                )}

                {!loading && !error && cards.length > 0 && (
                    <div className="memory-cards-stream">
                        {cards.map((card) => {
                            const title = fallbackTitle(card);
                            const summary = fallbackSummary(card);
                            const chips = card.context
                                .map((item) => normalizeText(item))
                                .filter((item) => {
                                    const lower = item.toLowerCase();
                                    return (
                                        item.length > 0
                                        && !lower.startsWith("app:")
                                        && !lower.startsWith("type:")
                                    );
                                })
                                .slice(0, 4);

                            return (
                                <article key={card.id} className="result-card memory-browse-card">
                                    <div className="result-meta">
                                        <span className="result-app">{card.app_name}</span>
                                        <span className="result-time">
                                            {formatDay(card.timestamp)} ·{" "}
                                            {new Date(card.timestamp).toLocaleTimeString(undefined, {
                                                hour: "2-digit",
                                                minute: "2-digit",
                                            })}
                                        </span>
                                    </div>
                                    <div className="memory-browse-content">
                                        <div className="memory-browse-title">{title}</div>
                                        <div className="memory-browse-summary">{summary}</div>
                                    </div>
                                    {chips.length > 0 && (
                                        <div className="result-context-chips">
                                            {chips.map((item, index) => (
                                                <span key={`${card.id}-ctx-${index}`} className="result-chip">
                                                    {item}
                                                </span>
                                            ))}
                                        </div>
                                    )}
                                </article>
                            );
                        })}
                    </div>
                )}
            </div>
        </div>
    );
}
