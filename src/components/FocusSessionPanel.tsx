// Inspired by Claude Code's context.ts — tracks your "active context" (git status,
// current project) and surfaces it. Here we surface the active screen context:
// what app/window you're currently working in and recent activity clusters.
import { useEffect, useState } from "react";
import { MemoryCard, listMemoryCards } from "../api/tauri";
import "./FocusSessionPanel.css";

interface FocusSessionPanelProps {
    isVisible: boolean;
    onClose: () => void;
    onSearchApp: (appName: string) => void;
}

interface AppCluster {
    appName: string;
    windowTitles: string[];
    cards: MemoryCard[];
    mostRecent: number;
}

function clusterByApp(cards: MemoryCard[]): AppCluster[] {
    const map = new Map<string, AppCluster>();
    for (const card of cards) {
        const key = card.app_name;
        if (!map.has(key)) {
            map.set(key, { appName: key, windowTitles: [], cards: [], mostRecent: 0 });
        }
        const cluster = map.get(key)!;
        cluster.cards.push(card);
        if (!cluster.windowTitles.includes(card.window_title)) {
            cluster.windowTitles.push(card.window_title);
        }
        if (card.timestamp > cluster.mostRecent) {
            cluster.mostRecent = card.timestamp;
        }
    }
    return Array.from(map.values()).sort((a, b) => b.mostRecent - a.mostRecent);
}

function formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

function sessionDuration(cards: MemoryCard[]): string {
    if (cards.length < 2) return "";
    const times = cards.map((c) => c.timestamp);
    const span = Math.max(...times) - Math.min(...times);
    const m = Math.round(span / 60_000);
    if (m < 1) return "";
    if (m < 60) return `${m}m`;
    const h = Math.floor(m / 60);
    const rem = m % 60;
    return rem > 0 ? `${h}h ${rem}m` : `${h}h`;
}

// Derive a one-line description of what was happening in this cluster
function clusterSummary(cluster: AppCluster): string {
    const titles = cluster.windowTitles.slice(0, 3);
    return titles.join(" · ");
}

export function FocusSessionPanel({ isVisible, onClose, onSearchApp }: FocusSessionPanelProps) {
    const [cards, setCards] = useState<MemoryCard[]>([]);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [expanded, setExpanded] = useState<string | null>(null);

    useEffect(() => {
        if (!isVisible) return;
        let mounted = true;

        const load = async () => {
            setLoading(true);
            setError(null);
            try {
                // Get the 60 most recent memories — mirrors CC's context snapshot approach
                const data = await listMemoryCards(60);
                if (mounted) setCards(data);
            } catch (err) {
                if (mounted) setError(err instanceof Error ? err.message : "Failed to load activity.");
            } finally {
                if (mounted) setLoading(false);
            }
        };

        void load();

        // Refresh every 30s to keep context current
        const interval = window.setInterval(() => void load(), 30_000);
        return () => {
            mounted = false;
            window.clearInterval(interval);
        };
    }, [isVisible]);

    if (!isVisible) return null;

    const clusters = clusterByApp(cards);
    const activeApp = clusters[0];

    return (
        <div className="fs-page">
            <header className="fs-header">
                <div>
                    <h2>Focus Session</h2>
                    <p>Current work context — what you're in right now</p>
                </div>
                <button className="ui-action-btn fs-close-btn" onClick={onClose}>
                    Close
                </button>
            </header>

            <div className="fs-body">
                {loading && cards.length === 0 && (
                    <div className="fs-state">
                        <div className="thinking-loader thinking-loader-md" aria-hidden="true" />
                        <p>Reading your active context…</p>
                    </div>
                )}

                {error && (
                    <div className="fs-state">
                        <p className="fs-error">{error}</p>
                    </div>
                )}

                {!loading && !error && cards.length === 0 && (
                    <div className="fs-state">
                        <p>No recent activity captured yet.</p>
                    </div>
                )}

                {cards.length > 0 && (
                    <>
                        {/* Active context banner — the app with most recent activity */}
                        {activeApp && (
                            <div className="fs-active-banner">
                                <div className="fs-active-indicator" />
                                <div className="fs-active-info">
                                    <span className="fs-active-label">Currently in</span>
                                    <span className="fs-active-app">{activeApp.appName}</span>
                                    {activeApp.windowTitles[0] && (
                                        <span className="fs-active-window" title={activeApp.windowTitles[0]}>
                                            {activeApp.windowTitles[0].length > 60
                                                ? activeApp.windowTitles[0].slice(0, 60) + "…"
                                                : activeApp.windowTitles[0]}
                                        </span>
                                    )}
                                </div>
                                <button
                                    className="ui-action-btn fs-search-btn"
                                    onClick={() => {
                                        onSearchApp(activeApp.appName);
                                        onClose();
                                    }}
                                >
                                    Search →
                                </button>
                            </div>
                        )}

                        {/* Timeline of recent app clusters */}
                        <div className="fs-section-label">Recent session</div>
                        <div className="fs-clusters">
                            {clusters.map((cluster) => {
                                const dur = sessionDuration(cluster.cards);
                                const isOpen = expanded === cluster.appName;
                                return (
                                    <div key={cluster.appName} className={`fs-cluster ${isOpen ? "open" : ""}`}>
                                        <button
                                            className="fs-cluster-header"
                                            onClick={() => setExpanded(isOpen ? null : cluster.appName)}
                                        >
                                            <span className="fs-cluster-app">{cluster.appName}</span>
                                            <span className="fs-cluster-meta">
                                                {cluster.cards.length} memories
                                                {dur ? ` · ${dur}` : ""}
                                                {" · "}
                                                {formatTime(cluster.mostRecent)}
                                            </span>
                                            <span className="fs-cluster-chevron">{isOpen ? "▲" : "▼"}</span>
                                        </button>

                                        {isOpen && (
                                            <div className="fs-cluster-body">
                                                <p className="fs-cluster-summary">{clusterSummary(cluster)}</p>
                                                <div className="fs-cluster-memories">
                                                    {cluster.cards.slice(0, 5).map((card) => (
                                                        <div key={card.id} className="fs-memory-row">
                                                            <span className="fs-memory-time">
                                                                {formatTime(card.timestamp)}
                                                            </span>
                                                            <span className="fs-memory-title">
                                                                {card.title}
                                                            </span>
                                                        </div>
                                                    ))}
                                                    {cluster.cards.length > 5 && (
                                                        <p className="fs-more-hint">
                                                            +{cluster.cards.length - 5} more
                                                        </p>
                                                    )}
                                                </div>
                                                <button
                                                    className="ui-action-btn fs-cluster-search"
                                                    onClick={() => {
                                                        onSearchApp(cluster.appName);
                                                        onClose();
                                                    }}
                                                >
                                                    Search all {cluster.appName} activity →
                                                </button>
                                            </div>
                                        )}
                                    </div>
                                );
                            })}
                        </div>
                    </>
                )}
            </div>
        </div>
    );
}
