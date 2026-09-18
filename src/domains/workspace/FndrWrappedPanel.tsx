import { useCallback, useLayoutEffect, useMemo, useState } from "react";
import {
    exportWeeklyWrappedPdf,
    getWeeklyWrapped,
    openExportedPdf,
    type WeeklyWrapped,
} from "@/shared/ipc/tauri";
import "./FndrWrappedPanel.css";

interface FndrWrappedPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

interface WeekOption {
    startDate: string;
    endDate: string;
    isCurrentWeek: boolean;
}

type WrappedScreen = "selection" | "results";
type MonthScope = "current" | "previous";

function localDateString(date = new Date()) {
    const yyyy = date.getFullYear();
    const mm = String(date.getMonth() + 1).padStart(2, "0");
    const dd = String(date.getDate()).padStart(2, "0");
    return `${yyyy}-${mm}-${dd}`;
}

function parseLocalDate(value: string) {
    const [year, month, day] = value.split("-").map(Number);
    return new Date(year, month - 1, day);
}

function mondayOf(date: Date) {
    const monday = new Date(date.getFullYear(), date.getMonth(), date.getDate());
    monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
    return monday;
}

function currentWeek(today: string): WeekOption {
    const startDate = localDateString(mondayOf(parseLocalDate(today)));
    return { startDate, endDate: today, isCurrentWeek: true };
}

function weeksForMonth(year: number, month: number, today: string): WeekOption[] {
    const firstDay = new Date(year, month, 1);
    const lastDay = new Date(year, month + 1, 0);
    const cursor = mondayOf(firstDay);
    const lastWeekStart = mondayOf(lastDay);
    const options: WeekOption[] = [];

    while (cursor <= lastWeekStart) {
        const startDate = localDateString(cursor);
        if (startDate <= today) {
            const sunday = new Date(cursor.getFullYear(), cursor.getMonth(), cursor.getDate() + 6);
            const fullEndDate = localDateString(sunday);
            const isCurrentWeek = startDate <= today && fullEndDate >= today;
            options.push({
                startDate,
                endDate: isCurrentWeek ? today : fullEndDate,
                isCurrentWeek,
            });
        }
        cursor.setDate(cursor.getDate() + 7);
    }
    return options;
}

function formatMinutes(minutes: number): string {
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.floor(minutes / 60);
    const remainder = minutes % 60;
    return remainder > 0 ? `${hours}h ${remainder}m` : `${hours}h`;
}

function formatFullDateRange(start: string, end: string) {
    const startDate = parseLocalDate(start);
    const endDate = parseLocalDate(end);
    const sameYear = startDate.getFullYear() === endDate.getFullYear();
    const sameMonth = sameYear && startDate.getMonth() === endDate.getMonth();
    const longMonth = new Intl.DateTimeFormat(undefined, { month: "long" });
    const longDate = new Intl.DateTimeFormat(undefined, { month: "long", day: "numeric" });

    if (sameMonth) {
        return `${longMonth.format(startDate)} ${startDate.getDate()}–${endDate.getDate()}, ${endDate.getFullYear()}`;
    }
    if (sameYear) {
        return `${longDate.format(startDate)}–${longDate.format(endDate)}, ${endDate.getFullYear()}`;
    }
    const dateWithYear = new Intl.DateTimeFormat(undefined, { month: "long", day: "numeric", year: "numeric" });
    return `${dateWithYear.format(startDate)}–${dateWithYear.format(endDate)}`;
}

function formatLastUpdated(timestamp: number) {
    return new Intl.DateTimeFormat(undefined, {
        month: "short",
        day: "numeric",
        hour: "numeric",
        minute: "2-digit",
    }).format(new Date(timestamp));
}

function formatHour(hour: number | null): string {
    if (hour == null) return "No clear peak yet";
    return `${hour % 12 || 12} ${hour >= 12 ? "PM" : "AM"}`;
}

function RankedList({ items, emptyLabel }: { items: WeeklyWrapped["apps"]; emptyLabel: string }) {
    if (items.length === 0) return <p className="wrapped-empty-list">{emptyLabel}</p>;

    return (
        <ol className="wrapped-ranked-list">
            {items.map((item, index) => (
                <li className={index === 0 ? "top-rank" : ""} key={item.name}>
                    <span className="wrapped-rank-number">{index + 1}</span>
                    <span className="wrapped-rank-name">{item.name}</span>
                    <span className="wrapped-rank-time">{formatMinutes(item.duration_minutes)}</span>
                </li>
            ))}
        </ol>
    );
}

export function FndrWrappedPanel({ isVisible, onClose }: FndrWrappedPanelProps) {
    const today = localDateString();
    const initialWeek = currentWeek(today);
    const [screen, setScreen] = useState<WrappedScreen>("selection");
    const [monthScope, setMonthScope] = useState<MonthScope>("current");
    const [selectedWeek, setSelectedWeek] = useState<WeekOption>(initialWeek);
    const [wrapped, setWrapped] = useState<WeeklyWrapped | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [exporting, setExporting] = useState(false);
    const [exportedPdfPath, setExportedPdfPath] = useState<string | null>(null);

    const currentMonthWeeks = useMemo(() => {
        const now = parseLocalDate(today);
        return weeksForMonth(now.getFullYear(), now.getMonth(), today);
    }, [today]);
    const previousMonthWeeks = useMemo(() => {
        const now = parseLocalDate(today);
        return weeksForMonth(now.getFullYear(), now.getMonth() - 1, today);
    }, [today]);
    const visibleWeeks = monthScope === "current" ? currentMonthWeeks : previousMonthWeeks;
    const selectedRange = formatFullDateRange(selectedWeek.startDate, selectedWeek.endDate);

    const handleMonthScopeChange = (scope: MonthScope) => {
        setMonthScope(scope);
        const weeks = scope === "current" ? currentMonthWeeks : previousMonthWeeks;
        if (weeks.length > 0) {
            setSelectedWeek(weeks[weeks.length - 1]);
        }
    };

    useLayoutEffect(() => {
        if (!isVisible) return;
        setScreen("selection");
        setMonthScope("current");
        setSelectedWeek(currentWeek(localDateString()));
        setWrapped(null);
        setError(null);
        setExportedPdfPath(null);
    }, [isVisible]);

    const loadWrapped = useCallback(async () => {
        setLoading(true);
        setError(null);
        setWrapped(null);
        try {
            const recap = await getWeeklyWrapped(selectedWeek.startDate, selectedWeek.endDate);
            setWrapped(recap);
            setScreen("results");
        } catch (err) {
            setError(err instanceof Error ? err.message : "Unable to build FNDR Wrapped.");
        } finally {
            setLoading(false);
        }
    }, [selectedWeek]);

    const recapText = useMemo(() => {
        if (!wrapped) return "";
        const app = wrapped.apps[0];
        return [
            `FNDR recorded about ${formatMinutes(wrapped.total_minutes)} of activity across ${wrapped.active_days} active day${wrapped.active_days === 1 ? "" : "s"}${app ? `, mostly in ${app.name}` : ""}.`,
            `Captured memories: ${wrapped.total_captures.toLocaleString()}.`,
            `Busiest day: ${wrapped.busiest_day?.day ?? "none recorded"}.`,
            `Open follow-ups or to-dos: ${wrapped.open_tasks}.`,
        ].join("\n");
    }, [wrapped]);

    const handleExport = async () => {
        if (!wrapped || exporting) return;
        setExporting(true);
        setError(null);
        try {
            setExportedPdfPath(await exportWeeklyWrappedPdf(wrapped.start_date, wrapped.end_date, recapText));
        } catch (err) {
            setError(err instanceof Error ? err.message : "Unable to export recap.");
        } finally {
            setExporting(false);
        }
    };

    const handleChangeWeek = () => {
        setScreen("selection");
        setWrapped(null);
        setError(null);
        setExportedPdfPath(null);
    };

    if (!isVisible) return null;

    const topApp = wrapped?.apps[0];
    const activityNarrative = wrapped
        ? wrapped.total_captures > 0
            ? `FNDR recorded about ${formatMinutes(wrapped.total_minutes)} of activity across ${wrapped.active_days} active day${wrapped.active_days === 1 ? "" : "s"}${topApp ? `, mostly in ${topApp.name}` : ""}.`
            : "FNDR did not record activity for this week."
        : "";

    return (
        <div className="wrapped-page">
            {screen === "selection" ? (
                <main className="wrapped-selection-page">
                    <header className="wrapped-selection-header">
                        <div>
                            <h2>FNDR Wrapped</h2>
                            <p>Choose a week to review your recorded activity.</p>
                        </div>
                        <button className="ui-action-btn wrapped-close-btn" onClick={onClose}>X</button>
                    </header>

                    <section className="wrapped-week-selector" aria-labelledby="wrapped-week-title">
                        <span className="wrapped-eyebrow">SELECT A WEEK</span>
                        <h3 id="wrapped-week-title">{selectedRange}</h3>
                        <p>{selectedWeek.isCurrentWeek ? "This week is still in progress. Your recap will show activity recorded so far." : "Weeks run from Monday through Sunday."}</p>
                        <div className="wrapped-month-tabs" role="tablist" aria-label="Available Wrapped months">
                            <button
                                type="button"
                                role="tab"
                                aria-selected={monthScope === "current"}
                                className={monthScope === "current" ? "active" : ""}
                                onClick={() => handleMonthScopeChange("current")}
                            >
                                This month
                            </button>
                            <button
                                type="button"
                                role="tab"
                                aria-selected={monthScope === "previous"}
                                className={monthScope === "previous" ? "active" : ""}
                                onClick={() => handleMonthScopeChange("previous")}
                            >
                                Last month
                            </button>
                        </div>
                        <div className="wrapped-week-options" role="list" aria-label="Available weeks">
                            {visibleWeeks.map((week, index) => {
                                const isSelected = week.startDate === selectedWeek.startDate;
                                return (
                                    <button
                                        type="button"
                                        role="listitem"
                                        className={`wrapped-week-option ${isSelected ? "selected" : ""}`}
                                        onClick={() => setSelectedWeek(week)}
                                        key={week.startDate}
                                    >
                                        <span>Week {index + 1}</span>
                                        <strong>{formatFullDateRange(week.startDate, week.endDate)}</strong>
                                        {week.isCurrentWeek && <em>Week so far</em>}
                                    </button>
                                );
                            })}
                        </div>
                        {error && <p className="wrapped-selection-error">{error}</p>}
                        <button type="button" className="wrapped-start-btn" onClick={() => void loadWrapped()} disabled={loading}>
                            {loading ? "Building recap…" : "Start Wrapped"}
                        </button>
                    </section>
                </main>
            ) : (
                <>
                    <header className="wrapped-results-header">
                        <div className="wrapped-results-title">
                            <h2>FNDR Wrapped</h2>
                            <strong>{wrapped ? formatFullDateRange(wrapped.start_date, wrapped.end_date) : selectedRange}</strong>
                            {wrapped && (
                                <div>
                                    <span>Data collected from {formatFullDateRange(wrapped.start_date, wrapped.end_date)}</span>
                                    <span>Last updated {formatLastUpdated(wrapped.generated_at_ms)}</span>
                                </div>
                            )}
                        </div>
                        <div className="wrapped-results-actions">
                            <button className="ui-action-btn wrapped-change-week-btn" onClick={handleChangeWeek}>Change week</button>
                            <button className="ui-action-btn wrapped-refresh-btn" onClick={() => void loadWrapped()} disabled={loading}>
                                {loading ? "Updating…" : "Update recap"}
                            </button>
                            <button className="ui-action-btn wrapped-export-btn" onClick={() => void handleExport()} disabled={!wrapped || exporting}>
                                {exporting ? "Exporting…" : "Export"}
                            </button>
                            <button className="ui-action-btn wrapped-close-btn" onClick={onClose}>X</button>
                        </div>
                    </header>

                    <main className="wrapped-results-body">
                        {error && <p className="wrapped-results-error">{error}</p>}
                        {!wrapped && loading && (
                            <div className="wrapped-state">
                                <div className="thinking-loader thinking-loader-lg" aria-hidden="true" />
                                <p>Updating your recap…</p>
                            </div>
                        )}
                        {wrapped && (
                            <div className="wrapped-results-content">
                                <section className="wrapped-hero">
                                    <span className="wrapped-eyebrow">WEEK OVERVIEW</span>
                                    <h3>{activityNarrative}</h3>
                                </section>

                                {wrapped.total_captures === 0 ? (
                                    <section className="wrapped-week-empty">
                                        <h3>No captured memories for this week yet.</h3>
                                        <p>Try another week, or keep FNDR running while you work to build a future recap.</p>
                                        <button type="button" onClick={handleChangeWeek}>Choose another week</button>
                                    </section>
                                ) : (
                                    <div className="wrapped-results-grid">
                                        <section className="wrapped-card wrapped-overview-card">
                                            <span className="wrapped-card-label">WEEK OVERVIEW</span>
                                            <div className="wrapped-overview-stats">
                                                <div><strong>{wrapped.total_captures.toLocaleString()}</strong><span>Captured memories</span></div>
                                                <div><strong>{wrapped.active_days}</strong><span>Active days</span></div>
                                                <div><strong>{wrapped.meeting_count}</strong><span>Meetings</span></div>
                                            </div>
                                        </section>

                                        <section className="wrapped-card wrapped-rank-card">
                                            <span className="wrapped-card-label">APPS WITH THE MOST RECORDED ACTIVITY</span>
                                            <RankedList items={wrapped.apps} emptyLabel="No app activity recorded yet." />
                                        </section>

                                        <section className="wrapped-card wrapped-rank-card">
                                            <span className="wrapped-card-label">MOST-VISITED WEBSITES</span>
                                            <RankedList items={wrapped.websites} emptyLabel="No website activity recorded yet." />
                                        </section>

                                        <section className="wrapped-card">
                                            <span className="wrapped-card-label">BUSIEST DAY</span>
                                            <div className="wrapped-static-detail">
                                                <span>{wrapped.busiest_day?.day ?? "No activity recorded"}</span>
                                                <small>Most active hour: {formatHour(wrapped.busiest_hour)}</small>
                                            </div>
                                        </section>

                                        <section className="wrapped-card">
                                            <span className="wrapped-card-label">OPEN FOLLOW-UPS OR TO-DOS</span>
                                            <div className="wrapped-static-detail">
                                                <strong>{wrapped.open_tasks}</strong>
                                                <small>{wrapped.open_followups} follow-up{wrapped.open_followups === 1 ? "" : "s"} open</small>
                                            </div>
                                        </section>
                                    </div>
                                )}
                            </div>
                        )}
                    </main>
                </>
            )}

            {exportedPdfPath && (
                <div className="wrapped-export-toast" role="status">
                    <span>Recap exported to Downloads.</span>
                    <button type="button" onClick={() => void openExportedPdf(exportedPdfPath)}>Open PDF</button>
                    <button type="button" onClick={() => setExportedPdfPath(null)}>Dismiss</button>
                </div>
            )}
        </div>
    );
}
