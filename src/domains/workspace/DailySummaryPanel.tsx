import { useState, useEffect, useRef } from "react";
import {
    addTodo,
    generateDailySummaryForDate,
    getDailySummaryFollowups,
    getDailySummaryOverview,
    exportDailySummaryPdf,
    openExportedPdf,
    setTodoCompleted,
    type Task,
} from "@/shared/ipc/tauri";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import "./DailySummaryPanel.css";
import { ThinkingIndicator } from "@/shared/components/ThinkingIndicator";

interface DailySummaryPanelProps {
    isVisible: boolean;
    onClose: () => void;
    onOpenMemoryById: (memoryId: string) => void;
}

function localDateString(date = new Date()) {
    const yyyy = date.getFullYear();
    const mm = String(date.getMonth() + 1).padStart(2, "0");
    const dd = String(date.getDate()).padStart(2, "0");
    return `${yyyy}-${mm}-${dd}`;
}

function shiftLocalDate(dateStr: string, days: number) {
    const [year, month, day] = dateStr.split("-").map(Number);
    const date = new Date(year, month - 1, day);
    date.setDate(date.getDate() + days);
    return localDateString(date);
}

function displayDate(dateStr: string) {
    if (!dateStr) return "Select a date";
    const [year, month, day] = dateStr.split("-").map(Number);
    return new Intl.DateTimeFormat("en-US", {
        weekday: "long",
        month: "long",
        day: "numeric",
        year: "numeric",
    }).format(new Date(year, month - 1, day));
}

function followupSourceLabel(followup: Task) {
    const source = followup.source_app.trim();
    if (source.toLowerCase().startsWith("memory:")) {
        return `${source.slice("memory:".length).trim() || "Captured"} memory`;
    }
    if (source.toLowerCase().startsWith("meeting:")) {
        return "Meeting recording";
    }
    return source || "FNDR";
}

function followupContext(followup: Task) {
    const description = followup.description.trim();
    if (description) return description;

    const linkedUrl = followup.linked_urls[0];
    if (linkedUrl) {
        try {
            return `Linked context: ${new URL(linkedUrl).hostname}`;
        } catch {
            return "Linked context is available.";
        }
    }

    if (followup.source_memory_id || followup.linked_memory_ids.length > 0) {
        return "Captured memory context is available.";
    }
    return "No additional context was saved.";
}

export function DailySummaryPanel({ isVisible, onClose, onOpenMemoryById }: DailySummaryPanelProps) {
    const [dateStr, setDateStr] = useState<string>("");
    const [summary, setSummary] = useState<string | null>(null);
    const [summaryDateStr, setSummaryDateStr] = useState<string | null>(null);
    const [overview, setOverview] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [exporting, setExporting] = useState(false);
    const [showToast, setShowToast] = useState(false);
    const [exportedPdfPath, setExportedPdfPath] = useState<string | null>(null);
    const [cache, setCache] = useState<Map<string, string>>(new Map());
    const [followups, setFollowups] = useState<Task[]>([]);
    const [followupsLoading, setFollowupsLoading] = useState(false);
    const [followupError, setFollowupError] = useState<string | null>(null);
    const [addingTaskId, setAddingTaskId] = useState<string | null>(null);
    const [addedTaskIds, setAddedTaskIds] = useState<Set<string>>(new Set());
    const [expandedFollowupIds, setExpandedFollowupIds] = useState<Set<string>>(new Set());
    const [updatingFollowupId, setUpdatingFollowupId] = useState<string | null>(null);
    const generationRequestRef = useRef(0);
    const dialogRef = useRef<HTMLDivElement>(null);
    const closeButtonRef = useRef<HTMLButtonElement>(null);
    const todayDateStr = localDateString();
    const isViewingToday = dateStr === todayDateStr;

    // Initialize to today's date in local YYYY-MM-DD
    useEffect(() => {
        setDateStr(localDateString());
    }, []);

    useModalFocus(isVisible, dialogRef, closeButtonRef, onClose);

    const selectDate = (nextDate: string) => {
        generationRequestRef.current += 1;
        setDateStr(nextDate);
        setError(null);
        setOverview(null);
        setFollowups([]);
        setFollowupError(null);
        setFollowupsLoading(false);
        setLoading(false);
        setExportedPdfPath(null);
        setShowToast(false);
        setSummary(null);
        setSummaryDateStr(null);
    };

    const handleGenerate = async (targetDate = dateStr) => {
        if (!targetDate) return;

        const requestId = generationRequestRef.current + 1;
        generationRequestRef.current = requestId;

        setOverview(null);
        setFollowupError(null);
        setFollowupsLoading(true);
        void getDailySummaryFollowups()
            .then((tasks) => {
                if (generationRequestRef.current === requestId) {
                    setFollowups(tasks);
                }
            })
            .catch(() => {
                if (generationRequestRef.current === requestId) {
                    setFollowupError("Current follow-ups could not be loaded.");
                }
            })
            .finally(() => {
                if (generationRequestRef.current === requestId) {
                    setFollowupsLoading(false);
                }
            });
        void getDailySummaryOverview(targetDate)
            .then((rawOverview) => {
                if (generationRequestRef.current === requestId) {
                    setOverview(rawOverview.trim() || null);
                }
            })
            .catch(() => {
                if (generationRequestRef.current === requestId) {
                    setOverview(null);
                }
            });

        if (cache.has(targetDate)) {
            setSummary(cache.get(targetDate) ?? null);
            setSummaryDateStr(targetDate);
            setError(null);
            setLoading(false);
            return;
        }

        setLoading(true);
        setError(null);
        setSummary(null);

        try {
            const rawSummary = await generateDailySummaryForDate(targetDate);
            if (generationRequestRef.current !== requestId) return;
            const nextSummary = rawSummary.trim() || "No memories were recorded for this date.";
            setSummary(nextSummary);
            setSummaryDateStr(targetDate);
            setCache((previous) => new Map(previous).set(targetDate, nextSummary));
        } catch (err) {
            if (generationRequestRef.current === requestId) {
                setError(err instanceof Error ? err.message : "Failed to generate summary.");
            }
        } finally {
            if (generationRequestRef.current === requestId) {
                setLoading(false);
            }
        }
    };

    const handleDateNavigation = (days: number) => {
        if (!dateStr) return;
        const nextDate = shiftLocalDate(dateStr, days);
        if (nextDate > todayDateStr) return;
        selectDate(nextDate);
        void handleGenerate(nextDate);
    };

    const handleToday = () => {
        if (isViewingToday) return;
        selectDate(todayDateStr);
        void handleGenerate(todayDateStr);
    };

    const handleToggleFollowup = async (followup: Task) => {
        const isCompleted = !followup.is_completed;
        setUpdatingFollowupId(followup.id);
        setFollowupError(null);
        try {
            const updated = await setTodoCompleted(followup.id, isCompleted);
            if (updated) {
                setFollowups((previous) => previous.map((task) => (
                    task.id === followup.id ? { ...task, is_completed: isCompleted } : task
                )));
            } else {
                setFollowupError("That follow-up no longer exists. Refresh the summary and try again.");
            }
        } catch (err) {
            setFollowupError(err instanceof Error ? err.message : "Unable to update follow-up.");
        } finally {
            setUpdatingFollowupId(null);
        }
    };

    const toggleFollowupDetails = (taskId: string) => {
        setExpandedFollowupIds((previous) => {
            const next = new Set(previous);
            if (next.has(taskId)) {
                next.delete(taskId);
            } else {
                next.add(taskId);
            }
            return next;
        });
    };

    const handleAddToTasks = async (followup: Task) => {
        if (addingTaskId || addedTaskIds.has(followup.id)) return;
        setAddingTaskId(followup.id);
        setFollowupError(null);
        try {
            await addTodo(followup.title, "Todo");
            setAddedTaskIds((previous) => new Set(previous).add(followup.id));
        } catch (err) {
            setFollowupError(err instanceof Error ? err.message : "Unable to add task.");
        } finally {
            setAddingTaskId(null);
        }
    };

    const handleDownloadPdf = async () => {
        if (!summaryDateStr || !summary) return;
        setExporting(true);
        setError(null);
        try {
            const path = await exportDailySummaryPdf(summaryDateStr, summary);
            setExportedPdfPath(path);
            setShowToast(true);
            setTimeout(() => {
                setShowToast(false);
            }, 6000);
        } catch (err) {
            setError(String(err));
        } finally {
            setExporting(false);
        }
    };

    const handleOpenPdf = async () => {
        if (!exportedPdfPath) {
            return;
        }

        try {
            await openExportedPdf(exportedPdfPath);
            setShowToast(false);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
        }
    };

    if (!isVisible) {
        return null;
    }

    const openFollowupCount = followups.filter((followup) => !followup.is_completed).length;

    return (
        <div
            ref={dialogRef}
            className="daily-summary-page"
            role="dialog"
            aria-modal="true"
            aria-labelledby="daily-summary-title"
            tabIndex={-1}
        >
            <header className="daily-summary-header">
                <div>
                    <h2 id="daily-summary-title">Daily Summary</h2>
                    <p>Review locally captured activity for one calendar day.</p>
                </div>
                <div className="daily-summary-actions">
                    <button
                        ref={closeButtonRef}
                        type="button"
                        className="ui-action-btn daily-summary-close-btn"
                        onClick={onClose}
                        aria-label="Close Daily Summary"
                    >
                        <span aria-hidden="true">×</span>
                    </button>
                </div>
            </header>

            <div className="daily-summary-body">
                <div className="daily-summary-controls">
                    <div className="daily-date-navigation" aria-label="Daily summary date navigation">
                        <button
                            type="button"
                            className="daily-date-nav-btn"
                            onClick={() => handleDateNavigation(-1)}
                            disabled={!dateStr || loading}
                            aria-label="View previous day"
                        >
                            <span aria-hidden="true">‹</span>
                        </button>
                        <div className="date-picker-wrapper">
                            <label htmlFor="daily-summary-date">{displayDate(dateStr)}</label>
                            <div className="daily-date-input-wrap">
                                <span className="daily-date-calendar-icon" aria-hidden="true">
                                    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                                        <rect x="3.5" y="5" width="17" height="15" rx="2" />
                                        <path d="M7.5 3v4M16.5 3v4M3.5 9h17" />
                                    </svg>
                                </span>
                                <input
                                    id="daily-summary-date"
                                    type="date"
                                    value={dateStr}
                                    onChange={(event) => selectDate(event.target.value)}
                                    max={todayDateStr}
                                    aria-label="Select summary date"
                                />
                            </div>
                        </div>
                        <button
                            type="button"
                            className="daily-date-nav-btn"
                            onClick={() => handleDateNavigation(1)}
                            disabled={!dateStr || isViewingToday || loading}
                            aria-label="View next day"
                        >
                            <span aria-hidden="true">›</span>
                        </button>
                        <button
                            type="button"
                            className="daily-today-btn"
                            onClick={handleToday}
                            disabled={isViewingToday || loading}
                        >
                            Today
                        </button>
                    </div>
                    <button
                        className="ui-action-btn generate-btn"
                        onClick={() => void handleGenerate()}
                        disabled={loading || !dateStr}
                    >
                        {loading ? "Generating..." : "Generate Summary"}
                    </button>
                    {summary && summaryDateStr === dateStr && (
                        <button
                            className={`ui-action-btn generate-btn download-pdf-btn ${exporting ? "loading" : ""}`}
                            onClick={() => void handleDownloadPdf()}
                            disabled={exporting || loading}
                        >
                            {exporting ? "Exporting..." : "↓ Download PDF"}
                        </button>
                    )}
                </div>

                <div className="daily-summary-content">
                    {loading && (
                        <div className="daily-summary-state">
                            <ThinkingIndicator state="composing" size="md" />
                            <p>Clustering the day&apos;s local memories...</p>
                        </div>
                    )}

                    {!loading && error && (
                        <div className="daily-summary-state error-state" role="alert">
                            <p>{error}</p>
                            <button type="button" className="ui-action-btn" onClick={() => void handleGenerate()}>
                                Try again
                            </button>
                        </div>
                    )}

                    {!loading && !error && summary && summaryDateStr === dateStr && (
                        <>
                            {overview && <p className="daily-summary-overview">{overview}</p>}
                            <section className="daily-followups-card" aria-labelledby="daily-followups-heading">
                                <div className="daily-followups-header">
                                    <div>
                                        <p className="daily-followups-eyebrow">Across your task list</p>
                                        <h3 id="daily-followups-heading">Current open follow-ups</h3>
                                    </div>
                                    <span className="daily-followups-count">{openFollowupCount}</span>
                                </div>
                                {followupError && <p className="daily-followups-error" role="alert">{followupError}</p>}
                                {followupsLoading ? (
                                    <p className="daily-followups-empty" role="status">Loading current follow-ups…</p>
                                ) : followups.length === 0 ? (
                                    <p className="daily-followups-empty">No open follow-ups right now.</p>
                                ) : (
                                    <div className="daily-followups-list">
                                        {followups.map((followup) => {
                                            const memoryId = followup.source_memory_id ?? followup.linked_memory_ids[0];
                                            const isAdded = addedTaskIds.has(followup.id);
                                            const isExpanded = expandedFollowupIds.has(followup.id);
                                            const isUpdating = updatingFollowupId === followup.id;
                                            return (
                                                <article className={`daily-followup-item ${followup.is_completed ? "completed" : ""}`} key={followup.id}>
                                                    <button
                                                        type="button"
                                                        className="daily-followup-main"
                                                        onClick={() => toggleFollowupDetails(followup.id)}
                                                        aria-expanded={isExpanded}
                                                    >
                                                        <span className="daily-followup-title">{followup.title}</span>
                                                        <span className="daily-followup-expand-icon" aria-hidden="true">{isExpanded ? "⌃" : "⌄"}</span>
                                                    </button>
                                                    <label className="daily-followup-completion">
                                                        <input
                                                            type="checkbox"
                                                            checked={followup.is_completed}
                                                            disabled={isUpdating}
                                                            aria-label={`${followup.is_completed ? "Undo" : "Mark done"}: ${followup.title}`}
                                                            onChange={() => void handleToggleFollowup(followup)}
                                                        />
                                                        <span>{isUpdating ? "Saving..." : followup.is_completed ? "Undo" : "Done"}</span>
                                                    </label>
                                                    {isExpanded && (
                                                        <div className="daily-followup-expanded">
                                                            <dl className="daily-followup-details">
                                                                <div>
                                                                    <dt>When</dt>
                                                                    <dd>{followup.due_date ? `Due ${new Date(followup.due_date).toLocaleDateString()}` : "Carry forward"}</dd>
                                                                </div>
                                                                <div>
                                                                    <dt>From</dt>
                                                                    <dd>{followupSourceLabel(followup)}</dd>
                                                                </div>
                                                                <div className="daily-followup-detail-wide">
                                                                    <dt>Details</dt>
                                                                    <dd>{followupContext(followup)}</dd>
                                                                </div>
                                                            </dl>
                                                            <div className="daily-followup-actions">
                                                                <button
                                                                    type="button"
                                                                    className="daily-followup-link"
                                                                    disabled={!memoryId}
                                                                    onClick={() => memoryId && onOpenMemoryById(memoryId)}
                                                                >
                                                                    Open related memory
                                                                </button>
                                                                <button
                                                                    type="button"
                                                                    className="daily-followup-link"
                                                                    disabled={isAdded || addingTaskId === followup.id}
                                                                    onClick={() => void handleAddToTasks(followup)}
                                                                >
                                                                    {isAdded ? "Added to tasks" : addingTaskId === followup.id ? "Adding..." : "Add to tasks"}
                                                                </button>
                                                            </div>
                                                        </div>
                                                    )}
                                                </article>
                                            );
                                        })}
                                    </div>
                                )}
                            </section>
                            <section className="daily-summary-result" aria-labelledby="daily-summary-result-title">
                                <h3 id="daily-summary-result-title">Summary for {displayDate(summaryDateStr)}</h3>
                                <ul className="summary-bullets">
                                {summary.split("\n").map((line, idx) => {
                                    const trim = line.trim();
                                    if (!trim) return null;
                                    const lineWithoutMarker = trim.replace(/^[-•*]\s*/, "");
                                    return (
                                        <li key={`${idx}-${lineWithoutMarker}`} className="summary-bullet">
                                            {lineWithoutMarker}
                                        </li>
                                    );
                                })}
                                </ul>
                            </section>
                        </>
                    )}

                    {!loading && !error && (!summary || summaryDateStr !== dateStr) && (
                        <div className="daily-summary-state empty-state">
                            <span className="shining-shield" aria-hidden="true">📅</span>
                            <p>Generate a summary for {displayDate(dateStr)} to review the memories FNDR recorded that day.</p>
                        </div>
                    )}
                </div>
            </div>

            {showToast && (
                <div className="daily-toast" role="status" aria-live="polite">
                    <div className="daily-toast-copy">
                        <strong>Summary PDF is ready</strong>
                        <span>Open the exported daily summary from FNDR.</span>
                    </div>
                    <div className="daily-toast-actions">
                        <button className="daily-toast-btn primary" onClick={() => void handleOpenPdf()}>
                            Open PDF
                        </button>
                        <button className="daily-toast-btn" onClick={() => setShowToast(false)}>
                            Dismiss
                        </button>
                    </div>
                </div>
            )}
        </div>
    );
}
