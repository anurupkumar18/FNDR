import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
    exportWeeklyWrappedPdf,
    getWeeklyWrapped,
    openExportedPdf,
    type WeeklyWrapped,
} from "@/shared/ipc/tauri";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import "./FndrWrappedPanel.css";
import { ThinkingIndicator } from "@/shared/components/ThinkingIndicator";
import { PanelHeader } from "@/shared/components/PanelHeader";
import { SegmentedControl } from "@/shared/components/SegmentedControl";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
} from "@/shared/activity/activityTrace";

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

function CountedList({ items, emptyLabel }: { items: WeeklyWrapped["projects_and_topics"]; emptyLabel: string }) {
    if (items.length === 0) return <p className="wrapped-empty-list">{emptyLabel}</p>;

    return (
        <ol className="wrapped-ranked-list">
            {items.map((item, index) => (
                <li className={index === 0 ? "top-rank" : ""} key={item.name}>
                    <span className="wrapped-rank-number">{index + 1}</span>
                    <span className="wrapped-rank-name">{item.name}</span>
                    <span className="wrapped-rank-time">{item.count.toLocaleString()}</span>
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
    const [wrappedActivity, setWrappedActivity] = useState<ActivityTraceSnapshot | null>(null);
    const [exporting, setExporting] = useState(false);
    const [exportedPdfPath, setExportedPdfPath] = useState<string | null>(null);
    const dialogRef = useRef<HTMLDivElement>(null);
    const closeButtonRef = useRef<HTMLButtonElement>(null);
    const loadGenerationRef = useRef(0);

    useModalFocus(isVisible, dialogRef, closeButtonRef, onClose);

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
        loadGenerationRef.current += 1;
        if (!isVisible) return;
        setScreen("selection");
        setMonthScope("current");
        setSelectedWeek(currentWeek(localDateString()));
        setWrapped(null);
        setLoading(false);
        setError(null);
        setExportedPdfPath(null);
        setWrappedActivity(null);
    }, [isVisible]);

    const loadWrapped = useCallback(async () => {
        const loadGeneration = ++loadGenerationRef.current;
        const startedAtMs = Date.now();
        const startedTrace = recordActivityStep(
            beginActivityTrace({
                id: `weekly-wrapped-${startedAtMs}`,
                title: "FNDR Wrapped activity",
                startedAtMs,
            }),
            {
                id: "wrapped-request",
                label: "Building weekly recap",
                actor: "Wrapped aggregator",
                status: "running",
                evidence: "ipc-boundary",
                atMs: startedAtMs,
            },
        );
        setLoading(true);
        setError(null);
        setWrapped(null);
        setWrappedActivity(startedTrace);
        try {
            const recap = await getWeeklyWrapped(selectedWeek.startDate, selectedWeek.endDate);
            if (loadGenerationRef.current !== loadGeneration) return;
            setWrapped(recap);
            setScreen("results");
            const finishedAtMs = Date.now();
            setWrappedActivity(recordActivityStep(startedTrace, {
                id: "wrapped-request",
                label: "Weekly recap ready",
                actor: "Wrapped aggregator",
                status: "completed",
                evidence: "result-metadata",
                atMs: finishedAtMs,
                durationMs: finishedAtMs - startedAtMs,
                detail: `${recap.total_captures.toLocaleString()} captures across ${recap.active_days.toLocaleString()} active ${recap.active_days === 1 ? "day" : "days"}`,
            }));
        } catch (err) {
            if (loadGenerationRef.current !== loadGeneration) return;
            setError(err instanceof Error ? err.message : "Unable to build FNDR Wrapped.");
            const finishedAtMs = Date.now();
            setWrappedActivity(recordActivityStep(startedTrace, {
                id: "wrapped-request",
                label: "Weekly recap unavailable",
                actor: "Wrapped aggregator",
                status: "failed",
                evidence: "ipc-boundary",
                atMs: finishedAtMs,
                durationMs: finishedAtMs - startedAtMs,
            }));
        } finally {
            if (loadGenerationRef.current === loadGeneration) {
                setLoading(false);
            }
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
        const startedAtMs = Date.now();
        setWrappedActivity((current) => recordActivityStep(
            current ?? beginActivityTrace({
                id: `wrapped-export-${startedAtMs}`,
                title: "FNDR Wrapped activity",
                startedAtMs,
            }),
            {
                id: "pdf-export",
                label: "Exporting weekly recap PDF",
                actor: "Local PDF exporter",
                status: "running",
                evidence: "ipc-boundary",
                atMs: startedAtMs,
            },
        ));
        setExporting(true);
        setError(null);
        try {
            setExportedPdfPath(await exportWeeklyWrappedPdf(wrapped.start_date, wrapped.end_date, recapText));
            const finishedAtMs = Date.now();
            setWrappedActivity((current) => current ? recordActivityStep(current, {
                id: "pdf-export",
                label: "Weekly recap PDF saved locally",
                actor: "Local PDF exporter",
                status: "completed",
                evidence: "result-metadata",
                atMs: finishedAtMs,
                durationMs: finishedAtMs - startedAtMs,
            }) : current);
        } catch (err) {
            setError(err instanceof Error ? err.message : "Unable to export recap.");
            const failedAtMs = Date.now();
            setWrappedActivity((current) => current ? recordActivityStep(current, {
                id: "pdf-export",
                label: "Weekly recap PDF export failed",
                actor: "Local PDF exporter",
                status: "failed",
                evidence: "ipc-boundary",
                atMs: failedAtMs,
                durationMs: failedAtMs - startedAtMs,
            }) : current);
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

    const handleOpenExport = async () => {
        if (!exportedPdfPath) return;
        const startedAtMs = Date.now();
        setWrappedActivity((current) => current ? recordActivityStep(current, {
            id: "pdf-open",
            label: "Opening exported recap",
            actor: "macOS workspace",
            status: "running",
            evidence: "ipc-boundary",
            atMs: startedAtMs,
        }) : current);
        setError(null);
        try {
            await openExportedPdf(exportedPdfPath);
            setExportedPdfPath(null);
            const finishedAtMs = Date.now();
            setWrappedActivity((current) => current ? recordActivityStep(current, {
                id: "pdf-open",
                label: "Exported recap opened",
                actor: "macOS workspace",
                status: "completed",
                evidence: "result-metadata",
                atMs: finishedAtMs,
                durationMs: finishedAtMs - startedAtMs,
            }) : current);
        } catch (err) {
            setError(err instanceof Error ? err.message : "Unable to open the exported recap.");
            const failedAtMs = Date.now();
            setWrappedActivity((current) => current ? recordActivityStep(current, {
                id: "pdf-open",
                label: "Exported recap could not be opened",
                actor: "macOS workspace",
                status: "failed",
                evidence: "ipc-boundary",
                atMs: failedAtMs,
                durationMs: failedAtMs - startedAtMs,
            }) : current);
        }
    };

    if (!isVisible) return null;

    const topApp = wrapped?.apps[0];
    const activityNarrative = wrapped
        ? wrapped.total_captures > 0
            ? `FNDR recorded about ${formatMinutes(wrapped.total_minutes)} of activity across ${wrapped.active_days} active day${wrapped.active_days === 1 ? "" : "s"}${topApp ? `, mostly in ${topApp.name}` : ""}.`
            : "FNDR did not record activity for this week."
        : "";

    return (
        <div
            ref={dialogRef}
            className="wrapped-page"
            role="dialog"
            aria-modal="true"
            aria-labelledby="wrapped-panel-title"
            tabIndex={-1}
        >
            {screen === "selection" ? (
                <main className="wrapped-selection-page">
                    <PanelHeader
                        title="FNDR Wrapped"
                        titleId="wrapped-panel-title"
                        subtitle="Choose a week to review your recorded activity."
                        closeLabel="Close FNDR Wrapped"
                        closeRef={closeButtonRef}
                        onClose={onClose}
                    />

                    <section className="wrapped-week-selector" aria-labelledby="wrapped-week-title">
                        <span className="wrapped-eyebrow">Select a week</span>
                        <h3 id="wrapped-week-title">{selectedRange}</h3>
                        <p>{selectedWeek.isCurrentWeek ? "This week is still in progress. Your recap will show activity recorded so far." : "Weeks run from Monday through Sunday."}</p>
                        <SegmentedControl
                            className="wrapped-month-tabs"
                            ariaLabel="Available Wrapped months"
                            value={monthScope}
                            onChange={handleMonthScopeChange}
                            options={[
                                { value: "current", label: "This month" },
                                { value: "previous", label: "Last month" },
                            ]}
                        />
                        <fieldset className="wrapped-week-options">
                            <legend className="sr-only">Available weeks</legend>
                            {visibleWeeks.map((week, index) => {
                                const isSelected = week.startDate === selectedWeek.startDate;
                                return (
                                    <label
                                        className={`wrapped-week-option ${isSelected ? "selected" : ""}`}
                                        key={week.startDate}
                                    >
                                        <input
                                            type="radio"
                                            name="wrapped-week"
                                            checked={isSelected}
                                            onChange={() => setSelectedWeek(week)}
                                            aria-label={`Week ${index + 1}: ${formatFullDateRange(week.startDate, week.endDate)}`}
                                        />
                                        <span>Week {index + 1}</span>
                                        <strong>{formatFullDateRange(week.startDate, week.endDate)}</strong>
                                        {week.isCurrentWeek && <em>Week so far</em>}
                                    </label>
                                );
                            })}
                        </fieldset>
                        {error && <p className="wrapped-selection-error" role="alert">{error}</p>}
                        {wrappedActivity && (
                            <ActivityTrace
                                trace={wrappedActivity}
                                className="wrapped-activity-trace"
                                announce={!error}
                            />
                        )}
                        <button type="button" className="wrapped-start-btn" onClick={() => void loadWrapped()} disabled={loading}>
                            {loading ? "Building recap…" : "Start Wrapped"}
                        </button>
                    </section>
                </main>
            ) : (
                <>
                    <header className="wrapped-results-header">
                        <div className="wrapped-results-title">
                            <h2 id="wrapped-panel-title">FNDR Wrapped</h2>
                            <strong>{wrapped ? formatFullDateRange(wrapped.start_date, wrapped.end_date) : selectedRange}</strong>
                            {wrapped && (
                                <div>
                                    <span>Data collected from {formatFullDateRange(wrapped.start_date, wrapped.end_date)}</span>
                                    <span>Last updated {formatLastUpdated(wrapped.generated_at_ms)}</span>
                                </div>
                            )}
                        </div>
                        <div className="wrapped-results-actions">
                            <button type="button" className="ui-action-btn wrapped-change-week-btn" onClick={handleChangeWeek}>Change week</button>
                            <button type="button" className="ui-action-btn wrapped-refresh-btn" onClick={() => void loadWrapped()} disabled={loading}>
                                {loading ? "Updating…" : "Update recap"}
                            </button>
                            <button type="button" className="ui-action-btn wrapped-export-btn" onClick={() => void handleExport()} disabled={!wrapped || exporting}>
                                {exporting ? "Exporting…" : "Export"}
                            </button>
                            <button
                                ref={closeButtonRef}
                                type="button"
                                className="ui-action-btn wrapped-close-btn"
                                onClick={onClose}
                                aria-label="Close FNDR Wrapped"
                            >
                                <span aria-hidden="true">×</span>
                            </button>
                        </div>
                    </header>

                    <main className="wrapped-results-body">
                        {wrappedActivity && (
                            <ActivityTrace
                                trace={wrappedActivity}
                                className="wrapped-activity-trace"
                                announce={!error}
                            />
                        )}
                        {error && <p className="wrapped-results-error" role="alert">{error}</p>}
                        {!wrapped && loading && (
                            <div className="wrapped-state">
                                <ThinkingIndicator state="weaving" size="md" />
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
                                            <span className="wrapped-card-label">PROJECTS &amp; TOPICS</span>
                                            <CountedList items={wrapped.projects_and_topics} emptyLabel="No project or topic labels recorded yet." />
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
                                            <span className="wrapped-card-label">DOCUMENTS &amp; FILES</span>
                                            <div className="wrapped-static-detail">
                                                <strong>{wrapped.document_count.toLocaleString()}</strong>
                                                <small>
                                                    {wrapped.most_revisited_file
                                                        ? `Most revisited: ${wrapped.most_revisited_file}`
                                                        : "No revisited file was identified."}
                                                </small>
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
                    <span>Recap PDF is ready.</span>
                    <button type="button" onClick={() => void handleOpenExport()}>Open PDF</button>
                    <button type="button" onClick={() => setExportedPdfPath(null)}>Dismiss</button>
                </div>
            )}
        </div>
    );
}
