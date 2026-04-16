import { useEffect, useMemo, useState } from "react";
import { Stats, getStats } from "../api/tauri";
import "./StatsPanel.css";

interface StatsPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

function formatPercent(value: number) {
    return `${value.toFixed(1)}%`;
}

function formatHourLabel(hour: number) {
    const period = hour >= 12 ? "PM" : "AM";
    const base = hour % 12 || 12;
    return `${base}${period}`;
}

function formatTimestamp(ts: number | null) {
    if (!ts) return "—";
    return new Date(ts).toLocaleString(undefined, {
        month: "short",
        day: "numeric",
        year: "numeric",
        hour: "numeric",
        minute: "2-digit",
    });
}

type CardId = 'metrics' | 'insights' | 'ranks' | 'hourly' | 'rhythms';
const ALL_CARDS: CardId[] = ['metrics', 'insights', 'ranks', 'hourly', 'rhythms'];

export function StatsPanel({ isVisible, onClose }: StatsPanelProps) {
    const [stats, setStats] = useState<Stats | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [refreshing, setRefreshing] = useState(false);

    const [viewMode, setViewMode] = useState<"stacked" | "grid">("stacked");
    const [deckOrder, setDeckOrder] = useState<CardId[]>(ALL_CARDS);

    const loadStats = async (showLoading = false) => {
        if (showLoading) {
            setLoading(true);
        } else {
            setRefreshing(true);
        }
        setError(null);
        try {
            const snapshot = await getStats();
            setStats(snapshot);
        } catch (err) {
            setError(err instanceof Error ? err.message : "Unable to load stats.");
        } finally {
            setLoading(false);
            setRefreshing(false);
        }
    };

    useEffect(() => {
        if (!isVisible) {
            return;
        }
        void loadStats(true);
        const interval = window.setInterval(() => {
            void loadStats(false);
        }, 15_000);
        return () => window.clearInterval(interval);
    }, [isVisible]);

    const quickStats = useMemo(() => {
        if (!stats) return [];
        return [
            { label: "Total memories", value: stats.total_records.toLocaleString() },
            { label: "Today", value: stats.today_count.toLocaleString() },
            { label: "Last hour", value: stats.records_last_hour.toLocaleString() },
            { label: "Last 24h", value: stats.records_last_24h.toLocaleString() },
            { label: "Last 7 days", value: stats.records_last_7d.toLocaleString() },
            { label: "Days active", value: stats.total_days.toLocaleString() },
            { label: "Current streak", value: `${stats.current_streak_days}d` },
            { label: "Longest streak", value: `${stats.longest_streak_days}d` },
            { label: "Unique apps", value: stats.unique_apps.toLocaleString() },
            { label: "Unique sessions", value: stats.unique_sessions.toLocaleString() },
            { label: "Unique windows", value: stats.unique_window_titles.toLocaleString() },
            { label: "Unique domains", value: stats.unique_domains.toLocaleString() },
            { label: "Records with URLs", value: formatPercent((stats.records_with_url / Math.max(stats.total_records, 1)) * 100) },
            { label: "Records with screenshots", value: formatPercent((stats.records_with_screenshot / Math.max(stats.total_records, 1)) * 100) },
            { label: "Top-app focus share", value: formatPercent(stats.focus_app_share_pct) },
            { label: "App switches", value: stats.app_switches.toLocaleString() },
            { label: "Switches/hour", value: stats.app_switch_rate_per_hour.toFixed(1) },
            { label: "Avg records/day", value: stats.avg_records_per_active_day.toFixed(1) },
            { label: "Avg records/hour", value: stats.avg_records_per_hour.toFixed(1) },
            { label: "Capture span", value: `${stats.capture_span_hours.toFixed(1)}h` },
            { label: "Avg OCR confidence", value: formatPercent(stats.avg_ocr_confidence * 100) },
            { label: "Low-confidence OCR", value: stats.low_confidence_records.toLocaleString() },
            { label: "Avg OCR blocks", value: stats.avg_ocr_blocks.toFixed(1) },
            { label: "Avg noise score", value: stats.avg_noise_score.toFixed(2) },
            { label: "High-noise records", value: stats.high_noise_records.toLocaleString() },
            { label: "Avg memory gap", value: `${stats.avg_gap_minutes.toFixed(1)}m` },
            { label: "Longest gap", value: `${stats.longest_gap_minutes.toLocaleString()}m` },
        ];
    }, [stats]);

    const insights = useMemo(() => {
        if (!stats) return [];
        const topApp = stats.apps[0];
        const topDomain = stats.top_domains[0];
        const dominantDaypart = [...stats.daypart_distribution].sort((a, b) => b.count - a.count)[0];
        return [
            topApp
                ? `Top app is ${topApp.name}, contributing ${formatPercent((topApp.count / Math.max(stats.total_records, 1)) * 100)} of captures.`
                : "Top app insight unavailable yet.",
            topDomain
                ? `Top domain is ${topDomain.domain} with ${topDomain.count.toLocaleString()} captured memories.`
                : "No domain usage captured yet.",
            stats.busiest_hour
                ? `Peak hour is ${formatHourLabel(stats.busiest_hour.hour)} (${stats.busiest_hour.count.toLocaleString()} memories).`
                : "Peak hour unavailable yet.",
            stats.busiest_day
                ? `Most active day is ${stats.busiest_day.day} (${stats.busiest_day.count.toLocaleString()} memories).`
                : "Most active day unavailable yet.",
            `Current streak is ${stats.current_streak_days} day(s); best streak is ${stats.longest_streak_days}.`,
            `Most active daypart is ${dominantDaypart?.daypart ?? "n/a"} (${dominantDaypart?.count.toLocaleString() ?? "0"} memories).`,
        ];
    }, [stats]);

    const handleCardClick = (id: CardId) => {
        if (viewMode !== "stacked") return;
        setDeckOrder((prev) => {
            if (prev[0] === id) {
                return [...prev.slice(1), id];
            } else {
                return [id, ...prev.filter(c => c !== id)];
            }
        });
    };

    const renderCardContent = (id: CardId) => {
        if (!stats) return null;
        switch (id) {
            case 'metrics':
                return (
                    <div className="stats-card-scroller">
                        <h3>Quick Metrics</h3>
                        <div className="stats-page-grid">
                            {quickStats.map((item) => (
                                <div key={item.label} className="stats-page-card stats-data-card">
                                    <span className="stats-page-value">{item.value}</span>
                                    <span className="stats-page-label">{item.label}</span>
                                </div>
                            ))}
                        </div>
                    </div>
                );
            case 'insights':
                return (
                    <div className="stats-card-scroller">
                        <h3>Quick Insights</h3>
                        <div className="stats-page-insights">
                            {insights.map((insight) => (
                                <p key={insight}>{insight}</p>
                            ))}
                        </div>
                    </div>
                );
            case 'ranks':
                return (
                    <div className="stats-card-scroller stats-two-column">
                        <div>
                            <h3>Top Apps</h3>
                            <div className="stats-page-rank-list">
                                {stats.apps.length === 0 && <p className="stats-page-empty">No app activity yet.</p>}
                                {stats.apps.map((app) => {
                                    const max = Math.max(stats.apps[0]?.count ?? 1, 1);
                                    const width = (app.count / max) * 100;
                                    return (
                                        <div key={app.name} className="stats-page-rank-row">
                                            <div className="stats-page-rank-meta">
                                                <span>{app.name}</span>
                                                <span>{app.count.toLocaleString()}</span>
                                            </div>
                                            <div className="stats-page-rank-bar">
                                                <span style={{ width: `${width}%` }} />
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        </div>
                        <div>
                            <h3>Top Domains</h3>
                            <div className="stats-page-rank-list">
                                {stats.top_domains.length === 0 && <p className="stats-page-empty">No domain activity yet.</p>}
                                {stats.top_domains.map((domain) => {
                                    const max = Math.max(stats.top_domains[0]?.count ?? 1, 1);
                                    const width = (domain.count / max) * 100;
                                    return (
                                        <div key={domain.domain} className="stats-page-rank-row">
                                            <div className="stats-page-rank-meta">
                                                <span>{domain.domain}</span>
                                                <span>{domain.count.toLocaleString()}</span>
                                            </div>
                                            <div className="stats-page-rank-bar">
                                                <span style={{ width: `${width}%` }} />
                                            </div>
                                        </div>
                                    );
                                })}
                            </div>
                        </div>
                    </div>
                );
            case 'hourly':
                return (
                    <div className="stats-card-scroller">
                        <h3>Hourly Distribution</h3>
                        <div className="stats-hourly-heatmap" aria-label="Hourly capture distribution">
                            {stats.hourly_distribution.map((entry) => {
                                const maxHourly = Math.max(...stats.hourly_distribution.map((h) => h.count), 1);
                                const intensity = Math.max(0.12, entry.count / maxHourly);
                                return (
                                    <div
                                        key={entry.hour}
                                        className="stats-hour-cell"
                                        style={{ opacity: intensity }}
                                        title={`${formatHourLabel(entry.hour)}: ${entry.count.toLocaleString()}`}
                                    />
                                );
                            })}
                        </div>
                        <div className="stats-hour-labels">
                            <span>12AM</span>
                            <span>6AM</span>
                            <span>12PM</span>
                            <span>6PM</span>
                            <span>11PM</span>
                        </div>
                    </div>
                );
            case 'rhythms':
                return (
                    <div className="stats-card-scroller">
                        <h3>Capture Timeline</h3>
                        <div className="stats-page-meta-grid" style={{ marginBottom: "20px" }}>
                            <div className="stats-page-meta-row">
                                <span>First capture</span>
                                <strong>{formatTimestamp(stats.first_capture_ts)}</strong>
                            </div>
                            <div className="stats-page-meta-row">
                                <span>Most recent capture</span>
                                <strong>{formatTimestamp(stats.last_capture_ts)}</strong>
                            </div>
                            <div className="stats-page-meta-row">
                                <span>Busiest day</span>
                                <strong>{stats.busiest_day ? `${stats.busiest_day.day} (${stats.busiest_day.count.toLocaleString()})` : "—"}</strong>
                            </div>
                            <div className="stats-page-meta-row">
                                <span>Quietest day</span>
                                <strong>{stats.quietest_day ? `${stats.quietest_day.day} (${stats.quietest_day.count.toLocaleString()})` : "—"}</strong>
                            </div>
                        </div>

                        <div className="stats-two-column">
                            <div>
                                <h3>Weekday</h3>
                                <div className="stats-page-rank-list">
                                    {stats.weekday_distribution.map((entry) => {
                                        const maxWeekday = Math.max(...stats.weekday_distribution.map((d) => d.count), 1);
                                        const width = (entry.count / maxWeekday) * 100;
                                        return (
                                            <div key={entry.weekday} className="stats-page-rank-row">
                                                <div className="stats-page-rank-meta">
                                                    <span>{entry.weekday}</span>
                                                    <span>{entry.count.toLocaleString()}</span>
                                                </div>
                                                <div className="stats-page-rank-bar">
                                                    <span style={{ width: `${width}%` }} />
                                                </div>
                                            </div>
                                        );
                                    })}
                                </div>
                            </div>
                            <div>
                                <h3>Daypart</h3>
                                <div className="stats-page-rank-list">
                                    {stats.daypart_distribution.map((entry) => {
                                        const maxDaypart = Math.max(...stats.daypart_distribution.map((d) => d.count), 1);
                                        const width = (entry.count / maxDaypart) * 100;
                                        return (
                                            <div key={entry.daypart} className="stats-page-rank-row">
                                                <div className="stats-page-rank-meta">
                                                    <span>{entry.daypart}</span>
                                                    <span>{entry.count.toLocaleString()}</span>
                                                </div>
                                                <div className="stats-page-rank-bar">
                                                    <span style={{ width: `${width}%` }} />
                                                </div>
                                            </div>
                                        );
                                    })}
                                </div>
                            </div>
                        </div>
                    </div>
                );
        }
    };

    if (!isVisible) {
        return null;
    }

    return (
        <div className="stats-page">
            <header className="stats-page-header">
                <div>
                    <h2>FNDR Stats & Insights</h2>
                    <p>Full analytics page for capture quality, behavior, cadence, and context breadth.</p>
                </div>
                <div className="stats-page-actions">
                    <button
                        className="ui-action-btn stats-layout-btn"
                        onClick={() => setViewMode(v => v === "stacked" ? "grid" : "stacked")}
                    >
                        {viewMode === "stacked" ? "⊞ Lay Out All" : "📚 Stack Cards"}
                    </button>
                    <button
                        className="ui-action-btn stats-refresh-btn"
                        onClick={() => void loadStats(false)}
                        disabled={loading || refreshing}
                    >
                        {refreshing ? "Refreshing..." : "Refresh"}
                    </button>
                    <button className="ui-action-btn stats-close-btn" onClick={onClose}>
                        ✕ Close
                    </button>
                </div>
            </header>

            <div className="stats-page-body">
                {loading && (
                    <div className="stats-page-state">
                        <div className="thinking-loader thinking-loader-lg" aria-hidden="true" />
                        <p>Loading stats...</p>
                    </div>
                )}

                {!loading && error && (
                    <div className="stats-page-state">
                        <p>{error}</p>
                    </div>
                )}

                {!loading && !error && stats && (
                    <div className={`stats-deck-container is-${viewMode}`}>
                        {ALL_CARDS.map(id => {
                            const stackIndex = deckOrder.indexOf(id);
                            return (
                                <div 
                                    key={id}
                                    className={`stats-playing-card ${stackIndex === 0 ? "is-top" : ""} card-${id}`}
                                    style={{ "--stack-index": stackIndex } as React.CSSProperties}
                                    onClick={() => handleCardClick(id)}
                                    role="button"
                                    tabIndex={0}
                                >
                                    <div className={`stats-card-bg bg-${id}`} />
                                    <div className="stats-card-content">
                                        {renderCardContent(id)}
                                    </div>
                                </div>
                            );
                        })}
                    </div>
                )}
            </div>
        </div>
    );
}
