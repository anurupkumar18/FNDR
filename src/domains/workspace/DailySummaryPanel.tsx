import { useState, useEffect, useRef } from "react";
import {
    addTodo,
    generateDailySummaryForDate,
    generateDailySummaryNarrative,
    getDailySummaryFollowups,
    exportDailySummaryPdf,
    openExportedPdf,
    setTodoCompleted,
    type DailyNarrative,
    type DailyNarrativeItem,
    type Task,
} from "@/shared/ipc/tauri";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import "./DailySummaryPanel.css";
import { PanelState } from "@/shared/components/PanelState";
import { PanelHeader } from "@/shared/components/PanelHeader";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
} from "@/shared/activity/activityTrace";

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

const DONE_STORAGE_KEY = "fndr.dailySummary.done";

function readDoneIds(dateStr: string): Set<string> {
    try {
        const raw = window.localStorage.getItem(`${DONE_STORAGE_KEY}.${dateStr}`);
        const parsed: unknown = raw ? JSON.parse(raw) : [];
        return new Set(Array.isArray(parsed) ? parsed.filter((id): id is string => typeof id === "string") : []);
    } catch {
        return new Set();
    }
}

function writeDoneIds(dateStr: string, ids: Set<string>) {
    try {
        window.localStorage.setItem(`${DONE_STORAGE_KEY}.${dateStr}`, JSON.stringify([...ids]));
    } catch {
        // Checking an item still works for this session without storage.
    }
}

function backendLabel(backend: DailyNarrative["backend"]) {
    if (backend === "apple") return "Apple's on-device model";
    if (backend === "local") return "FNDR's local model";
    return null;
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

function NarrativeItemRow({
    item,
    done,
    onToggle,
    onOpen,
}: {
    item: DailyNarrativeItem;
    done: boolean;
    onToggle: () => void;
    onOpen: (memoryId: string) => void;
}) {
    return (
        <li className={`daily-item ${done ? "completed" : ""}`}>
            <label className="daily-item-check">
                <input
                    type="checkbox"
                    checked={done}
                    onChange={onToggle}
                    aria-label={`${done ? "Undo" : "Mark done"}: ${item.text}`}
                />
                <span className="daily-item-text">{item.text}</span>
            </label>
            {item.due_label && <span className="daily-item-due">{item.due_label}</span>}
            {item.memory_ids[0] && (
                <button
                    type="button"
                    className="daily-source-link"
                    onClick={() => onOpen(item.memory_ids[0])}
                    aria-label={`Open the memory behind: ${item.text}`}
                >
                    Source
                </button>
            )}
        </li>
    );
}

export function DailySummaryPanel({ isVisible, onClose, onOpenMemoryById }: DailySummaryPanelProps) {
    const [dateStr, setDateStr] = useState<string>("");
    const [summary, setSummary] = useState<string | null>(null);
    const [summaryDateStr, setSummaryDateStr] = useState<string | null>(null);
    const [narrative, setNarrative] = useState<DailyNarrative | null>(null);
    const [narrativeLoading, setNarrativeLoading] = useState(false);
    const [narrativeError, setNarrativeError] = useState<string | null>(null);
    const [narrativeCache, setNarrativeCache] = useState<Map<string, DailyNarrative>>(new Map());
    const [doneIds, setDoneIds] = useState<Set<string>>(new Set());
    const [itemError, setItemError] = useState<string | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [exporting, setExporting] = useState(false);
    const [showToast, setShowToast] = useState(false);
    const [exportedPdfPath, setExportedPdfPath] = useState<string | null>(null);
    const [cache, setCache] = useState<Map<string, string>>(new Map());
    const [followups, setFollowups] = useState<Task[]>([]);
    const [followupsLoading, setFollowupsLoading] = useState(false);
    const [followupError, setFollowupError] = useState<string | null>(null);
    const [summaryActivity, setSummaryActivity] = useState<ActivityTraceSnapshot | null>(null);
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
        setNarrative(null);
        setNarrativeLoading(false);
        setNarrativeError(null);
        setItemError(null);
        setFollowups([]);
        setFollowupError(null);
        setFollowupsLoading(false);
        setLoading(false);
        setExportedPdfPath(null);
        setShowToast(false);
        setSummary(null);
        setSummaryDateStr(null);
        setSummaryActivity(null);
    };

    const handleGenerate = async (targetDate = dateStr) => {
        if (!targetDate) return;

        const requestId = generationRequestRef.current + 1;
        generationRequestRef.current = requestId;
        const startedAtMs = Date.now();
        const startedTrace = recordActivityStep(
            beginActivityTrace({
                id: `daily-summary-${requestId}`,
                title: "Daily summary activity",
                startedAtMs,
            }),
            {
                id: "summary-request",
                label: "Requesting daily summary",
                actor: "Local summary engine",
                status: "running",
                evidence: "ipc-boundary",
                atMs: startedAtMs,
            },
        );
        setSummaryActivity(startedTrace);

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
        setItemError(null);
        const cachedNarrative = narrativeCache.get(targetDate);
        if (cachedNarrative) {
            setNarrative(cachedNarrative);
            setDoneIds(readDoneIds(targetDate));
            setNarrativeError(null);
            setNarrativeLoading(false);
        } else {
            setNarrative(null);
            setNarrativeError(null);
            setNarrativeLoading(true);
            void generateDailySummaryNarrative(targetDate)
                .then((result) => {
                    if (generationRequestRef.current !== requestId) return;
                    setNarrative(result);
                    setDoneIds(readDoneIds(targetDate));
                    // A basic summary is not kept, so asking again retries the model.
                    if (result.mode !== "basic") {
                        setNarrativeCache((previous) => new Map(previous).set(targetDate, result));
                    }
                })
                .catch((err: unknown) => {
                    if (generationRequestRef.current === requestId) {
                        setNarrativeError(err instanceof Error ? err.message : String(err));
                    }
                })
                .finally(() => {
                    if (generationRequestRef.current === requestId) {
                        setNarrativeLoading(false);
                    }
                });
        }

        if (cache.has(targetDate)) {
            setSummary(cache.get(targetDate) ?? null);
            setSummaryDateStr(targetDate);
            setError(null);
            setLoading(false);
            const finishedAtMs = Date.now();
            setSummaryActivity(recordActivityStep(startedTrace, {
                id: "summary-request",
                label: "Daily summary loaded from session cache",
                actor: "Summary cache",
                status: "completed",
                evidence: "frontend-event",
                atMs: finishedAtMs,
                durationMs: finishedAtMs - startedAtMs,
            }));
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
            const finishedAtMs = Date.now();
            setSummaryActivity((current) => recordActivityStep(current ?? startedTrace, {
                id: "summary-request",
                label: "Daily summary ready",
                actor: "Local summary engine",
                status: "completed",
                evidence: "result-metadata",
                atMs: finishedAtMs,
                durationMs: finishedAtMs - startedAtMs,
            }));
        } catch (err) {
            if (generationRequestRef.current === requestId) {
                setError(err instanceof Error ? err.message : "Failed to generate summary.");
                const finishedAtMs = Date.now();
                setSummaryActivity((current) => recordActivityStep(current ?? startedTrace, {
                    id: "summary-request",
                    label: "Daily summary request failed",
                    actor: "Local summary engine",
                    status: "failed",
                    evidence: "ipc-boundary",
                    atMs: finishedAtMs,
                    durationMs: finishedAtMs - startedAtMs,
                }));
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

    const handleToggleItem = async (item: DailyNarrativeItem) => {
        const nowDone = !doneIds.has(item.id);
        const apply = (done: boolean) => {
            setDoneIds((previous) => {
                const next = new Set(previous);
                if (done) next.add(item.id);
                else next.delete(item.id);
                writeDoneIds(dateStr, next);
                return next;
            });
        };
        setItemError(null);
        apply(nowDone);
        if (!item.task_id) return;
        try {
            const updated = await setTodoCompleted(item.task_id, nowDone);
            if (!updated) {
                setItemError("That task no longer exists in your task list.");
            }
        } catch (err) {
            apply(!nowDone);
            setItemError(err instanceof Error ? err.message : "Unable to update the task.");
        }
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
        const startedAtMs = Date.now();
        setSummaryActivity((current) => recordActivityStep(
            current ?? beginActivityTrace({
                id: `daily-summary-export-${startedAtMs}`,
                title: "Daily summary activity",
                startedAtMs,
            }),
            {
                id: "pdf-export",
                label: "Exporting daily summary PDF",
                actor: "Local PDF exporter",
                status: "running",
                evidence: "ipc-boundary",
                atMs: startedAtMs,
            },
        ));
        setExporting(true);
        setError(null);
        try {
            const path = await exportDailySummaryPdf(
                summaryDateStr,
                summary,
                narrative && narrative.mode !== "empty"
                    ? {
                        basic: narrative.mode === "basic",
                        paragraphs: narrative.paragraphs.map((paragraph) => paragraph.text),
                        action_items: narrative.action_items.map((item) => item.text),
                        reminders: narrative.reminders.map((item) => item.text),
                    }
                    : undefined,
            );
            setExportedPdfPath(path);
            const finishedAtMs = Date.now();
            setSummaryActivity((current) => current ? recordActivityStep(current, {
                id: "pdf-export",
                label: "Daily summary PDF saved locally",
                actor: "Local PDF exporter",
                status: "completed",
                evidence: "result-metadata",
                atMs: finishedAtMs,
                durationMs: finishedAtMs - startedAtMs,
            }) : current);
            setShowToast(true);
            setTimeout(() => {
                setShowToast(false);
            }, 6000);
        } catch (err) {
            setError(String(err));
            const failedAtMs = Date.now();
            setSummaryActivity((current) => current ? recordActivityStep(current, {
                id: "pdf-export",
                label: "Daily summary PDF export failed",
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

    const handleOpenPdf = async () => {
        if (!exportedPdfPath) {
            return;
        }

        const startedAtMs = Date.now();
        setSummaryActivity((current) => current ? recordActivityStep(current, {
            id: "pdf-open",
            label: "Opening exported PDF",
            actor: "macOS workspace",
            status: "running",
            evidence: "ipc-boundary",
            atMs: startedAtMs,
        }) : current);
        try {
            await openExportedPdf(exportedPdfPath);
            setShowToast(false);
            const finishedAtMs = Date.now();
            setSummaryActivity((current) => current ? recordActivityStep(current, {
                id: "pdf-open",
                label: "Exported PDF opened",
                actor: "macOS workspace",
                status: "completed",
                evidence: "result-metadata",
                atMs: finishedAtMs,
                durationMs: finishedAtMs - startedAtMs,
            }) : current);
        } catch (err) {
            setError(err instanceof Error ? err.message : String(err));
            const failedAtMs = Date.now();
            setSummaryActivity((current) => current ? recordActivityStep(current, {
                id: "pdf-open",
                label: "Exported PDF could not be opened",
                actor: "macOS workspace",
                status: "failed",
                evidence: "ipc-boundary",
                atMs: failedAtMs,
                durationMs: failedAtMs - startedAtMs,
            }) : current);
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
            <PanelHeader
                title="Daily Summary"
                titleId="daily-summary-title"
                subtitle="Review locally captured activity for one calendar day."
                closeLabel="Close Daily Summary"
                closeRef={closeButtonRef}
                onClose={onClose}
            />

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
                            disabled={exporting || loading || narrativeLoading}
                        >
                            {exporting ? "Exporting..." : "↓ Download PDF"}
                        </button>
                    )}
                </div>

                {summaryActivity && <ActivityTrace trace={summaryActivity} />}

                <div className="daily-summary-content">
                    {loading && (
                        <PanelState kind="loading" title="Preparing daily summary" description="Clustering this day’s local memories." />
                    )}

                    {!loading && error && (
                        <PanelState kind="error" title="Daily summary could not load" description={error} action={{ label: "Try again", onAction: () => void handleGenerate() }} />
                    )}

                    {!loading && !error && summary && summaryDateStr === dateStr && (
                        <>
                            <section className="daily-narrative-card" aria-labelledby="daily-narrative-heading">
                                <div className="daily-section-header">
                                    <h3 id="daily-narrative-heading">Your day</h3>
                                    {narrative?.mode === "basic" && (
                                        <span className="daily-basic-badge">Basic summary</span>
                                    )}
                                </div>
                                {narrativeLoading && (
                                    <PanelState
                                        kind="loading"
                                        title="Writing your day"
                                        description="The on-device model is reading this day's memories. Nothing leaves this Mac."
                                    />
                                )}
                                {!narrativeLoading && narrativeError && (
                                    <PanelState
                                        kind="error"
                                        title="Your day could not be written"
                                        description={narrativeError}
                                        action={{ label: "Try again", onAction: () => void handleGenerate() }}
                                    />
                                )}
                                {!narrativeLoading && !narrativeError && narrative?.mode === "empty" && (
                                    <p className="daily-section-empty">No memories were recorded for this date.</p>
                                )}
                                {!narrativeLoading && !narrativeError && narrative && narrative.mode !== "empty" && (
                                    <>
                                        {narrative.paragraphs.map((paragraph, index) => (
                                            <p className="daily-narrative-paragraph" key={`${index}-${paragraph.text}`}>
                                                {paragraph.text}
                                                {paragraph.memory_ids[0] && (
                                                    <button
                                                        type="button"
                                                        className="daily-source-link"
                                                        onClick={() => onOpenMemoryById(paragraph.memory_ids[0])}
                                                        aria-label={`Open the memory behind: ${paragraph.text}`}
                                                    >
                                                        Source
                                                    </button>
                                                )}
                                            </p>
                                        ))}
                                        {narrative.mode === "basic" ? (
                                            <p className="daily-narrative-note">
                                                Composed from your captures without a model: {narrative.basic_reason ?? "the model was not used"}.
                                            </p>
                                        ) : (
                                            <p className="daily-narrative-note">
                                                Written on this Mac by {backendLabel(narrative.backend) ?? "a local model"} and checked against your captures.
                                            </p>
                                        )}
                                    </>
                                )}
                            </section>
                            {!narrativeLoading && !narrativeError && narrative && narrative.action_items.length > 0 && (
                                <section className="daily-narrative-card" aria-labelledby="daily-actions-heading">
                                    <div className="daily-section-header">
                                        <h3 id="daily-actions-heading">Action items</h3>
                                    </div>
                                    <ul className="daily-item-list">
                                        {narrative.action_items.map((item) => (
                                            <NarrativeItemRow
                                                key={item.id}
                                                item={item}
                                                done={doneIds.has(item.id)}
                                                onToggle={() => void handleToggleItem(item)}
                                                onOpen={onOpenMemoryById}
                                            />
                                        ))}
                                    </ul>
                                    {itemError && <p className="daily-followups-error" role="alert">{itemError}</p>}
                                </section>
                            )}
                            {!narrativeLoading && !narrativeError && narrative && narrative.reminders.length > 0 && (
                                <section className="daily-narrative-card" aria-labelledby="daily-reminders-heading">
                                    <div className="daily-section-header">
                                        <h3 id="daily-reminders-heading">Reminders</h3>
                                    </div>
                                    <ul className="daily-item-list">
                                        {narrative.reminders.map((item) => (
                                            <NarrativeItemRow
                                                key={item.id}
                                                item={item}
                                                done={doneIds.has(item.id)}
                                                onToggle={() => void handleToggleItem(item)}
                                                onOpen={onOpenMemoryById}
                                            />
                                        ))}
                                    </ul>
                                </section>
                            )}
                            <details className="daily-all-activity">
                                <summary>All activity</summary>
                            <section className="daily-followups-card" aria-labelledby="daily-followups-heading">
                                <div className="daily-followups-header">
                                    <div>
                                        <p className="daily-followups-eyebrow">Across your task list</p>
                                        <h3 id="daily-followups-heading">Your open tasks</h3>
                                    </div>
                                    <span className="daily-followups-count">{openFollowupCount}</span>
                                </div>
                                {followupError && <p className="daily-followups-error" role="alert">{followupError}</p>}
                                {followupsLoading ? (
                                    <p className="daily-followups-empty" role="status">Loading your open tasks…</p>
                                ) : followups.length === 0 ? (
                                    <p className="daily-followups-empty">No open tasks right now.</p>
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
                            </details>
                        </>
                    )}

                    {!loading && !error && (!summary || summaryDateStr !== dateStr) && (
                        <PanelState kind="empty" title="No daily summary yet" description={`Generate a summary for ${displayDate(dateStr)} using the button above to review the memories FNDR recorded that day.`} />
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
