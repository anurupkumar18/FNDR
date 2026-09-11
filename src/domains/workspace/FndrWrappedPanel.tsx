import { useCallback, useEffect, useMemo, useState } from "react";
import { getWeeklyWrapped, type WeeklyWrapped } from "@/shared/ipc/tauri";
import { STORAGE_KEYS } from "@/shared/utils/config";
import "./FndrWrappedPanel.css";

interface FndrWrappedPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

interface SearchHistoryEntry {
    query: string;
    timestamp: number;
}

function formatMinutes(minutes: number): string {
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.floor(minutes / 60);
    const remainder = minutes % 60;
    return remainder > 0 ? `${hours}h ${remainder}m` : `${hours}h`;
}

function formatDateRange(start: string, end: string): string {
    const parseLocalDate = (value: string) => {
        const [year, month, day] = value.split("-").map(Number);
        return new Date(year, month - 1, day);
    };
    const options: Intl.DateTimeFormatOptions = { month: "short", day: "numeric" };
    return `${parseLocalDate(start).toLocaleDateString(undefined, options)} – ${parseLocalDate(end).toLocaleDateString(undefined, options)}`;
}

function formatHour(hour: number | null): string {
    if (hour == null) return "—";
    const period = hour >= 12 ? "PM" : "AM";
    return `${hour % 12 || 12} ${period}`;
}

function loadSearchCount(startDate: string, endDate: string): number {
    try {
        const raw = localStorage.getItem(STORAGE_KEYS.searchHistory);
        if (!raw) return 0;
        const entries = JSON.parse(raw) as SearchHistoryEntry[];
        const [startYear, startMonth, startDay] = startDate.split("-").map(Number);
        const [endYear, endMonth, endDay] = endDate.split("-").map(Number);
        const startMs = new Date(startYear, startMonth - 1, startDay).getTime();
        const endMs = new Date(endYear, endMonth - 1, endDay + 1).getTime();
        return entries.filter((entry) => entry.timestamp >= startMs && entry.timestamp < endMs).length;
    } catch {
        return 0;
    }
}

function RankList({ items, emptyLabel }: { items: WeeklyWrapped["apps"]; emptyLabel: string }) {
    if (items.length === 0) {
        return <p className="wrapped-empty-list">{emptyLabel}</p>;
    }

    return (
        <div className="wrapped-rank-list">
            {items.map((item, index) => (
                <div className="wrapped-rank-row" key={item.name}>
                    <span className="wrapped-rank-number">{String(index + 1).padStart(2, "0")}</span>
                    <span className="wrapped-rank-name">{item.name}</span>
                    <span className="wrapped-rank-time">{formatMinutes(item.duration_minutes)}</span>
                </div>
            ))}
        </div>
    );
}

export function FndrWrappedPanel({ isVisible, onClose }: FndrWrappedPanelProps) {
    const [wrapped, setWrapped] = useState<WeeklyWrapped | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const load = useCallback(async () => {
        setLoading(true);
        setError(null);
        try {
            setWrapped(await getWeeklyWrapped());
        } catch (err) {
            setError(err instanceof Error ? err.message : "Unable to load FNDR Wrapped.");
        } finally {
            setLoading(false);
        }
    }, []);

    useEffect(() => {
        if (isVisible) void load();
    }, [isVisible, load]);

    const searchCount = useMemo(
        () => wrapped ? loadSearchCount(wrapped.start_date, wrapped.end_date) : 0,
        [wrapped]
    );

    if (!isVisible) return null;

    const topApp = wrapped?.apps[0]?.name;
    const narrative = wrapped && wrapped.total_captures > 0
        ? topApp
            ? `You spent about ${formatMinutes(wrapped.total_minutes)} across ${wrapped.active_days} active day${wrapped.active_days === 1 ? "" : "s"}, mostly in “${topApp}”.`
            : `You captured ${wrapped.total_captures.toLocaleString()} moments across ${wrapped.active_days} active day${wrapped.active_days === 1 ? "" : "s"}.`
        : "Your week is still waiting to be remembered.";

    return (
        <div className="wrapped-page">
            <header className="wrapped-header">
                <div>
                    <h2>FNDR Wrapped</h2>
                    <p>{wrapped ? formatDateRange(wrapped.start_date, wrapped.end_date) : "Your week in review"}</p>
                </div>
                <div className="wrapped-header-actions">
                    <button className="ui-action-btn wrapped-refresh-btn" onClick={() => void load()} disabled={loading}>
                        {loading ? "Refreshing…" : "Refresh"}
                    </button>
                    <button className="ui-action-btn wrapped-close-btn" onClick={onClose}>X</button>
                </div>
            </header>

            <main className="wrapped-body">
                {loading && !wrapped && (
                    <div className="wrapped-state">
                        <div className="thinking-loader thinking-loader-lg" aria-hidden="true" />
                        <p>Rewinding your week…</p>
                    </div>
                )}

                {!loading && error && (
                    <div className="wrapped-state wrapped-error">
                        <p>{error}</p>
                        <button className="ui-action-btn" onClick={() => void load()}>Try again</button>
                    </div>
                )}

                {wrapped && !error && (
                    <>
                        <section className="wrapped-hero">
                            <span className="wrapped-eyebrow">YOUR WEEK IN REVIEW</span>
                            <h3>{narrative}</h3>
                            <p>Every snapshot is a small piece of the story you built this week.</p>
                        </section>

                        <section className="wrapped-stat-grid" aria-label="Weekly totals">
                            <div className="wrapped-stat-card"><strong>{wrapped.total_captures.toLocaleString()}</strong><span>Memories</span></div>
                            <div className="wrapped-stat-card"><strong>{wrapped.meeting_count}</strong><span>Meetings</span></div>
                            <div className="wrapped-stat-card"><strong>{wrapped.document_count}</strong><span>Documents</span></div>
                            <div className="wrapped-stat-card"><strong>{searchCount}</strong><span>Searches</span></div>
                        </section>

                        <section className="wrapped-two-column">
                            <article className="wrapped-card">
                                <span className="wrapped-card-label">MOST-USED APPS</span>
                                <RankList items={wrapped.apps} emptyLabel="No app activity recorded yet." />
                            </article>
                            <article className="wrapped-card">
                                <span className="wrapped-card-label">MOST-VISITED WEBSITES</span>
                                <RankList items={wrapped.websites} emptyLabel="No websites recorded yet." />
                            </article>
                        </section>

                        <section className="wrapped-two-column">
                            <article className="wrapped-card">
                                <span className="wrapped-card-label">WHAT YOU WORKED ON</span>
                                {wrapped.projects_and_topics.length > 0 ? (
                                    <div className="wrapped-topic-list">
                                        {wrapped.projects_and_topics.map((item) => (
                                            <div className="wrapped-topic-row" key={item.name}>
                                                <span>{item.name}</span><strong>{item.count}</strong>
                                            </div>
                                        ))}
                                    </div>
                                ) : <p className="wrapped-empty-list">No project or topic labels yet.</p>}
                            </article>
                            <article className="wrapped-card">
                                <span className="wrapped-card-label">WEEKLY SUPERLATIVES</span>
                                <div className="wrapped-superlatives">
                                    <div><span>Busiest day</span><strong>{wrapped.busiest_day?.day ?? "—"}</strong></div>
                                    <div><span>Most productive hour</span><strong>{formatHour(wrapped.busiest_hour)}</strong></div>
                                    <div><span>Most revisited file</span><strong>{wrapped.most_revisited_file ?? "—"}</strong></div>
                                    <div><span>Open follow-ups</span><strong>{wrapped.open_followups}</strong></div>
                                </div>
                            </article>
                        </section>
                    </>
                )}
            </main>
        </div>
    );
}
