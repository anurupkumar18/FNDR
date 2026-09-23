import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Task, addTodo, completeTodo, generateDailyBriefing, getTodos, updateTodo } from "@/shared/ipc/tauri";
import { useModalFocus } from "@/shared/hooks/useModalFocus";
import "./TodoPanel.css";
import { ThinkingIndicator } from "@/shared/components/ThinkingIndicator";
import { PanelHeader } from "@/shared/components/PanelHeader";
import { SegmentedControl } from "@/shared/components/SegmentedControl";

interface TodoPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

type TodoType = "Todo" | "Reminder" | "Followup";
type StageFilter = TodoType | "All";

function stageLabel(stage: StageFilter) {
    if (stage === "Todo") return "To-do";
    if (stage === "Followup") return "Follow-up";
    return stage;
}

export function TodoPanel({ isVisible, onClose }: TodoPanelProps) {
    const [tasks, setTasks] = useState<Task[]>([]);
    const [loading, setLoading] = useState(false);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [actionNotice, setActionNotice] = useState<{ kind: "error" | "success"; text: string } | null>(null);
    const [creating, setCreating] = useState(false);
    const [newTitle, setNewTitle] = useState("");
    const [newType, setNewType] = useState<TodoType>("Todo");
    const [activeStage, setActiveStage] = useState<StageFilter>("Todo");
    const [editingTaskId, setEditingTaskId] = useState<string | null>(null);
    const [editingTitle, setEditingTitle] = useState("");
    const [dailyBriefing, setDailyBriefing] = useState<string>("");
    const [dailyBriefingLoading, setDailyBriefingLoading] = useState(false);
    const [dailyBriefingError, setDailyBriefingError] = useState(false);
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
        setDailyBriefingError(false);
        setDailyBriefingLoading(true);
        generateDailyBriefing()
            .then((text) => {
                if (!mounted) {
                    return;
                }
                setDailyBriefing((text ?? "").trim());
            })
            .catch(() => {
                if (!mounted) {
                    return;
                }
                setDailyBriefing("");
                setDailyBriefingError(true);
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

    const sortedTasks = useMemo(() => tasks, [tasks]);

    const countsByType = useMemo(() => {
        return sortedTasks.reduce(
            (acc, task) => {
                acc[task.task_type] += 1;
                return acc;
            },
            { Todo: 0, Reminder: 0, Followup: 0 }
        );
    }, [sortedTasks]);

    const visibleTasks = useMemo(() => {
        if (activeStage === "All") {
            return sortedTasks;
        }
        return sortedTasks.filter((task) => task.task_type === activeStage);
    }, [sortedTasks, activeStage]);

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
            setActiveStage(created.task_type);
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
                subtitle="Create, classify, edit, and complete work carried forward from your day."
                closeLabel="Close To-dos"
                closeRef={closeButtonRef}
                onClose={onClose}
            />

            <section className="todo-briefing-row">
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

            <SegmentedControl
                className="todo-stage-toggle"
                ariaLabel="Task stages"
                value={activeStage}
                onChange={setActiveStage}
                options={(["Todo", "Reminder", "Followup", "All"] as StageFilter[]).map((stage) => {
                    const count = stage === "All" ? sortedTasks.length : countsByType[stage];
                    return {
                        value: stage,
                        ariaLabel: `${stageLabel(stage)} tasks, ${count}`,
                        label: (
                            <>
                                {stageLabel(stage)}
                                <span className="fndr-segment-count">{count}</span>
                            </>
                        ),
                    };
                })}
            />

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

                {!loading && !loadError && sortedTasks.length === 0 && (
                    <div className="todo-page-state">
                        <p>No active tasks yet. Add one above when something needs to carry forward.</p>
                    </div>
                )}

                {sortedTasks.length > 0 && visibleTasks.length === 0 && (
                    <div className="todo-page-state">
                        <p>No {stageLabel(activeStage).toLowerCase()}s right now.</p>
                    </div>
                )}

                {visibleTasks.length > 0 && (
                    <div className="todo-page-list">
                        {visibleTasks.map((task) => (
                            <article key={task.id} className="todo-page-item">
                                <div className="todo-page-item-main">
                                    <span className={`todo-pill ${task.task_type.toLowerCase()}`}>
                                        {stageLabel(task.task_type)}
                                    </span>
                                    <h3>{task.title}</h3>
                                    <p>
                                        {new Date(task.created_at).toLocaleString()}
                                        {task.linked_urls.length > 0
                                            ? ` · ${task.linked_urls.length} context links`
                                            : ""}
                                    </p>
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
                                <div className="todo-page-item-actions">
                                    <button
                                        className="ui-action-btn todo-edit-btn"
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
                                        className="ui-action-btn todo-done-btn"
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
                )}
            </div>
        </div>
    );
}
