import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Task, addTodo, completeTodo, dismissTodo, generateDailyBriefing, getTodos, updateTodo } from "@/shared/ipc/tauri";
import { acceptSuggestion, isSuggestion, relativeTime, sourceLabel } from "./todoSuggestions";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import "./TodoPanel.css";
import { ThinkingIndicator } from "@/shared/components/ThinkingIndicator";
import { PanelHeader } from "@/shared/components/PanelHeader";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
} from "@/shared/activity/activityTrace";

interface TodoPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

type TodoType = "Todo" | "Reminder" | "Followup";

function typeLabel(type: TodoType) {
    if (type === "Todo") return "To-do";
    if (type === "Followup") return "Follow-up";
    return type;
}

/** Type (when it says more than "to-do"), where it was seen, and how long ago. */
function metaLine(task: Task): string {
    return [
        task.task_type === "Todo" ? "" : typeLabel(task.task_type),
        sourceLabel(task),
        relativeTime(task.created_at),
    ]
        .filter(Boolean)
        .join(" · ");
}

export function TodoPanel({ isVisible, onClose }: TodoPanelProps) {
    const [tasks, setTasks] = useState<Task[]>([]);
    const [loading, setLoading] = useState(false);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [actionNotice, setActionNotice] = useState<{ kind: "error" | "success"; text: string } | null>(null);
    const [creating, setCreating] = useState(false);
    const [newTitle, setNewTitle] = useState("");
    const [newType, setNewType] = useState<TodoType>("Todo");
    const [editingTaskId, setEditingTaskId] = useState<string | null>(null);
    const [editingTitle, setEditingTitle] = useState("");
    const [dailyBriefing, setDailyBriefing] = useState<string>("");
    const [dailyBriefingLoading, setDailyBriefingLoading] = useState(false);
    const [dailyBriefingError, setDailyBriefingError] = useState(false);
    const [briefingActivity, setBriefingActivity] = useState<ActivityTraceSnapshot | null>(null);
    const [pendingTaskId, setPendingTaskId] = useState<string | null>(null);
    const [savingTaskId, setSavingTaskId] = useState<string | null>(null);
    const dialogRef = useRef<HTMLDivElement>(null);
    const closeButtonRef = useRef<HTMLButtonElement>(null);

    useModalFocus(isVisible, dialogRef, closeButtonRef, onClose);

    const loadTasks = useCallback(async (showLoading = false, isMounted: () => boolean = () => true) => {
        if (showLoading) {
            setLoading(true);
        }
        setLoadError(null);
        try {
            const data = await getTodos();
            if (isMounted()) {
                setTasks(data);
            }
        } catch (err) {
            if (isMounted()) {
                setLoadError(err instanceof Error ? err.message : "Unable to load tasks.");
            }
        } finally {
            if (isMounted()) {
                setLoading(false);
            }
        }
    }, []);

    useEffect(() => {
        if (!isVisible) {
            return;
        }
        let mounted = true;
        void loadTasks(true, () => mounted);
        const timer = window.setInterval(() => {
            void loadTasks(false, () => mounted);
        }, 20_000);
        return () => {
            mounted = false;
            window.clearInterval(timer);
        };
    }, [isVisible, loadTasks]);

    useEffect(() => {
        if (!isVisible) {
            return;
        }
        let mounted = true;
        const startedAtMs = Date.now();
        const startedTrace = recordActivityStep(
            beginActivityTrace({
                id: `daily-briefing-${startedAtMs}`,
                title: "Daily briefing activity",
                startedAtMs,
            }),
            {
                id: "briefing-request",
                label: "Generating daily briefing",
                actor: "Written from your captures, no model",
                status: "running",
                evidence: "ipc-boundary",
                atMs: startedAtMs,
            },
        );
        setDailyBriefingError(false);
        setDailyBriefingLoading(true);
        setBriefingActivity(startedTrace);
        generateDailyBriefing()
            .then((text) => {
                if (!mounted) {
                    return;
                }
                const briefing = (text ?? "").trim();
                setDailyBriefing(briefing);
                const finishedAtMs = Date.now();
                setBriefingActivity(recordActivityStep(startedTrace, {
                    id: "briefing-request",
                    label: briefing ? "Daily briefing ready" : "Daily briefing checked",
                    actor: "Written from your captures, no model",
                    status: briefing ? "completed" : "degraded",
                    evidence: "result-metadata",
                    atMs: finishedAtMs,
                    durationMs: finishedAtMs - startedAtMs,
                    detail: briefing ? "Briefing response received" : "No briefing was returned",
                }));
            })
            .catch(() => {
                if (!mounted) {
                    return;
                }
                setDailyBriefing("");
                setDailyBriefingError(true);
                const finishedAtMs = Date.now();
                setBriefingActivity(recordActivityStep(startedTrace, {
                    id: "briefing-request",
                    label: "Daily briefing unavailable",
                    actor: "Written from your captures, no model",
                    status: "failed",
                    evidence: "ipc-boundary",
                    atMs: finishedAtMs,
                    durationMs: finishedAtMs - startedAtMs,
                }));
            })
            .finally(() => {
                if (!mounted) {
                    return;
                }
                setDailyBriefingLoading(false);
            });
        return () => {
            mounted = false;
        };
    }, [isVisible]);

    // What the person committed to, and what FNDR only noticed. A suggestion
    // becomes a task when they add it, never on its own.
    const myTasks = useMemo(() => tasks.filter((task) => !isSuggestion(task)), [tasks]);
    const suggestedTasks = useMemo(() => tasks.filter(isSuggestion), [tasks]);

    const handleAddTask = async () => {
        const title = newTitle.trim();
        if (!title || creating) {
            return;
        }

        setCreating(true);
        setActionNotice(null);
        try {
            const created = await addTodo(title, newType);
            setTasks((prev) => [created, ...prev]);
            setNewTitle("");
            setActionNotice({ kind: "success", text: `Added “${created.title}”.` });
        } catch (err) {
            setActionNotice({
                kind: "error",
                text: err instanceof Error ? err.message : "Unable to add task.",
            });
        } finally {
            setCreating(false);
        }
    };

    const handleComplete = async (task: Task) => {
        if (pendingTaskId) return;
        setPendingTaskId(task.id);
        setActionNotice(null);
        try {
            const completed = await completeTodo(task.id);
            if (!completed) {
                setActionNotice({
                    kind: "error",
                    text: "That task no longer exists. Refresh the list and try again.",
                });
                return;
            }
            setTasks((previous) => previous.filter((item) => item.id !== task.id));
            setActionNotice({ kind: "success", text: `Completed “${task.title}”.` });
        } catch (err) {
            setActionNotice({
                kind: "error",
                text: err instanceof Error ? err.message : "Unable to complete task.",
            });
        } finally {
            setPendingTaskId(null);
        }
    };

    const handleAccept = async (task: Task) => {
        if (pendingTaskId) return;
        setPendingTaskId(task.id);
        setActionNotice(null);
        try {
            const accepted = await acceptSuggestion(task);
            setTasks((previous) => previous.map((item) => (item.id === task.id ? accepted : item)));
        } catch (err) {
            setActionNotice({
                kind: "error",
                text: err instanceof Error ? err.message : "Unable to add that suggestion.",
            });
        } finally {
            setPendingTaskId(null);
        }
    };

    const handleNotATask = async (task: Task) => {
        if (pendingTaskId) return;
        setPendingTaskId(task.id);
        setActionNotice(null);
        try {
            await dismissTodo(task.id);
            setTasks((previous) => previous.filter((item) => item.id !== task.id));
        } catch (err) {
            setActionNotice({
                kind: "error",
                text: err instanceof Error ? err.message : "Unable to remove that suggestion.",
            });
        } finally {
            setPendingTaskId(null);
        }
    };

    const handleSaveTask = async (task: Task) => {
        const nextTitle = editingTitle.trim();
        if (!nextTitle || savingTaskId) return;
        setSavingTaskId(task.id);
        setActionNotice(null);
        try {
            const updated = await updateTodo(task.id, nextTitle, task.task_type);
            setTasks((previous) => previous.map((item) => (item.id === updated.id ? updated : item)));
            setEditingTaskId(null);
            setEditingTitle("");
            setActionNotice({ kind: "success", text: `Saved “${updated.title}”.` });
        } catch (err) {
            setActionNotice({
                kind: "error",
                text: err instanceof Error ? err.message : "Unable to update task.",
            });
        } finally {
            setSavingTaskId(null);
        }
    };

    if (!isVisible) {
        return null;
    }

    return (
        <div
            ref={dialogRef}
            className="todo-page"
            role="dialog"
            aria-modal="true"
            aria-labelledby="todo-panel-title"
            tabIndex={-1}
        >
            <PanelHeader
                title="To-dos"
                titleId="todo-panel-title"
                subtitle="What you committed to, and what FNDR noticed on your screen."
                closeLabel="Close To-dos"
                closeRef={closeButtonRef}
                onClose={onClose}
            />

            {/* With nothing to brief on, the block says nothing useful and
                only pushes the list down. */}
            {(dailyBriefingLoading || dailyBriefingError || dailyBriefing) && (
                <section className="todo-briefing-row">
                    {briefingActivity && <ActivityTrace trace={briefingActivity} />}
                    <section className="todo-briefing-summary" aria-live="polite">
                        <p className="todo-briefing-label">Today&apos;s Briefing</p>
                        <p className="todo-briefing-text">
                            {dailyBriefingLoading
                                ? "Generating your summary…"
                                : dailyBriefingError
                                    ? "The briefing could not be generated. Your task list is still available."
                                    : dailyBriefing || "No briefing is available yet."}
                        </p>
                    </section>
                </section>
            )}

            <section className="todo-create-row">
                <label className="todo-create-field" htmlFor="todo-new-title">
                    <span>New task title</span>
                    <input
                        id="todo-new-title"
                        type="text"
                        placeholder="What needs to happen?"
                        value={newTitle}
                        onChange={(event) => setNewTitle(event.target.value)}
                        onKeyDown={(event) => {
                            if (event.key === "Enter") {
                                event.preventDefault();
                                void handleAddTask();
                            }
                        }}
                    />
                </label>
                <label className="todo-create-field todo-type-select-wrap" htmlFor="todo-new-type">
                    <span>Task type</span>
                    <select
                        id="todo-new-type"
                        value={newType}
                        onChange={(event) => setNewType(event.target.value as TodoType)}
                    >
                        <option value="Todo">To-do</option>
                        <option value="Reminder">Reminder</option>
                        <option value="Followup">Follow-up</option>
                    </select>
                </label>
                <button
                    className="ui-action-btn todo-add-btn"
                    type="button"
                    onClick={() => void handleAddTask()}
                    disabled={creating || !newTitle.trim()}
                >
                    {creating ? "Adding..." : "Add"}
                </button>
            </section>

            {actionNotice && (
                <div
                    className={`todo-action-notice is-${actionNotice.kind}`}
                    role={actionNotice.kind === "error" ? "alert" : "status"}
                >
                    <span>{actionNotice.text}</span>
                    <button type="button" onClick={() => setActionNotice(null)} aria-label="Dismiss task message">×</button>
                </div>
            )}

            {loadError && tasks.length > 0 && (
                <div className="todo-action-notice is-error" role="alert">
                    <span>Tasks could not be refreshed. Showing the last loaded list. {loadError}</span>
                    <button type="button" onClick={() => void loadTasks(false)}>Try again</button>
                </div>
            )}

            <div className="todo-page-body">
                {loading && tasks.length === 0 && (
                    <div className="todo-page-state" role="status">
                        <ThinkingIndicator state="working" size="md" />
                        <p>Loading tasks...</p>
                    </div>
                )}

                {!loading && loadError && tasks.length === 0 && (
                    <div className="todo-page-state" role="alert">
                        <p>{loadError}</p>
                        <button type="button" className="ui-action-btn" onClick={() => void loadTasks(true)}>
                            Try again
                        </button>
                    </div>
                )}

                {!loading && !loadError && tasks.length === 0 && (
                    <div className="todo-page-state">
                        <p>Nothing on your list. Add a task above when something needs to carry forward.</p>
                        <p>
                            When a mail, chat or note on screen asks something of you, FNDR suggests it here with
                            the words it read.
                        </p>
                    </div>
                )}

                {myTasks.length > 0 && (
                    <section className="todo-section" aria-labelledby="todo-mine-heading">
                        <h3 id="todo-mine-heading" className="todo-section-title">
                            My tasks <span className="todo-section-count">{myTasks.length}</span>
                        </h3>
                        <div className="todo-page-list">
                        {myTasks.map((task) => (
                            <article key={task.id} className="todo-row">
                                <div className="todo-row-main">
                                    <h4>{task.title}</h4>
                                    <p className="todo-row-meta">{metaLine(task)}</p>
                                    {editingTaskId === task.id && (
                                        <div className="todo-edit-row">
                                            <input
                                                value={editingTitle}
                                                onChange={(event) => setEditingTitle(event.target.value)}
                                                placeholder="Edit task title"
                                                aria-label={`Task title for ${task.title}`}
                                                disabled={savingTaskId === task.id}
                                                onKeyDown={(event) => {
                                                    if (event.key === "Enter") {
                                                        event.preventDefault();
                                                        void handleSaveTask(task);
                                                    }
                                                }}
                                            />
                                            <div className="todo-edit-actions">
                                                <button
                                                    className="ui-action-btn"
                                                    type="button"
                                                    disabled={savingTaskId === task.id}
                                                    onClick={() => {
                                                        setEditingTaskId(null);
                                                        setEditingTitle("");
                                                    }}
                                                >
                                                    Cancel
                                                </button>
                                                <button
                                                    className="ui-action-btn"
                                                    type="button"
                                                    onClick={() => void handleSaveTask(task)}
                                                    disabled={!editingTitle.trim() || savingTaskId === task.id}
                                                    aria-label={`Save task ${task.title}`}
                                                >
                                                    {savingTaskId === task.id ? "Saving…" : "Save"}
                                                </button>
                                            </div>
                                        </div>
                                    )}
                                </div>
                                <div className="todo-row-actions">
                                    <button
                                        className="ui-action-btn todo-row-btn"
                                        type="button"
                                        aria-label={`Edit ${task.title}`}
                                        disabled={pendingTaskId === task.id || savingTaskId === task.id}
                                        onClick={() => {
                                            setEditingTaskId(task.id);
                                            setEditingTitle(task.title);
                                        }}
                                    >
                                        Edit
                                    </button>
                                    <button
                                        className="ui-action-btn todo-row-btn todo-done-btn"
                                        type="button"
                                        aria-label={`Mark ${task.title} done`}
                                        disabled={pendingTaskId === task.id || savingTaskId === task.id}
                                        onClick={() => void handleComplete(task)}
                                    >
                                        {pendingTaskId === task.id ? "Saving…" : "Done"}
                                    </button>
                                </div>
                            </article>
                        ))}
                        </div>
                    </section>
                )}

                {suggestedTasks.length > 0 && (
                    <section className="todo-section" aria-labelledby="todo-suggested-heading">
                        <h3 id="todo-suggested-heading" className="todo-section-title">
                            Suggested <span className="todo-section-count">{suggestedTasks.length}</span>
                        </h3>
                        <p className="todo-section-hint">
                            Noticed on your screen. Nothing here is a task until you add it.
                        </p>
                        <div className="todo-page-list">
                        {suggestedTasks.map((task) => (
                            <article key={task.id} className="todo-row todo-row--suggested">
                                <div className="todo-row-main">
                                    <h4>{task.title}</h4>
                                    {task.description.trim() && (
                                        <p className="todo-row-quote">“{task.description.trim()}”</p>
                                    )}
                                    <p className="todo-row-meta">{metaLine(task)}</p>
                                </div>
                                <div className="todo-row-actions">
                                    <button
                                        className="ui-action-btn todo-row-btn"
                                        type="button"
                                        aria-label={`${task.title} is not a task`}
                                        disabled={pendingTaskId === task.id}
                                        onClick={() => void handleNotATask(task)}
                                    >
                                        Not a task
                                    </button>
                                    <button
                                        className="ui-action-btn todo-row-btn todo-done-btn"
                                        type="button"
                                        aria-label={`Add ${task.title} to my tasks`}
                                        disabled={pendingTaskId === task.id}
                                        onClick={() => void handleAccept(task)}
                                    >
                                        Add
                                    </button>
                                </div>
                            </article>
                        ))}
                        </div>
                    </section>
                )}
            </div>
        </div>
    );
}
