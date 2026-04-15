import { useEffect, useMemo, useState } from "react";
import { Task, addTodo, dismissTodo, getTodos } from "../api/tauri";
import "./TodoPanel.css";

interface TodoPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

type TodoType = "Todo" | "Reminder" | "Followup";
type StageFilter = TodoType | "All";

const STAGE_ORDER: TodoType[] = ["Todo", "Reminder", "Followup"];

export function TodoPanel({ isVisible, onClose }: TodoPanelProps) {
    const [tasks, setTasks] = useState<Task[]>([]);
    const [loading, setLoading] = useState(false);
    const [refreshing, setRefreshing] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [creating, setCreating] = useState(false);
    const [newTitle, setNewTitle] = useState("");
    const [newType, setNewType] = useState<TodoType>("Todo");
    const [activeStage, setActiveStage] = useState<StageFilter>("Todo");

    const loadTasks = async (showLoading = false) => {
        if (showLoading) {
            setLoading(true);
        } else {
            setRefreshing(true);
        }
        setError(null);
        try {
            const data = await getTodos();
            setTasks(data);
        } catch (err) {
            setError(err instanceof Error ? err.message : "Unable to load tasks.");
        } finally {
            setLoading(false);
            setRefreshing(false);
        }
    };

    useEffect(() => {
        if (!isVisible) {
            return;
        }
        void loadTasks(true);
        const timer = window.setInterval(() => {
            void loadTasks(false);
        }, 20_000);
        return () => window.clearInterval(timer);
    }, [isVisible]);

    const sortedTasks = useMemo(
        () => [...tasks].sort((a, b) => b.created_at - a.created_at),
        [tasks]
    );

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

    useEffect(() => {
        if (activeStage === "All" || visibleTasks.length > 0 || sortedTasks.length === 0) {
            return;
        }
        const nextStage = STAGE_ORDER.find((stage) => countsByType[stage] > 0);
        if (nextStage) {
            setActiveStage(nextStage);
        } else {
            setActiveStage("All");
        }
    }, [activeStage, visibleTasks.length, sortedTasks.length, countsByType]);

    const handleAddTask = async () => {
        const title = newTitle.trim();
        if (!title || creating) {
            return;
        }

        setCreating(true);
        setError(null);
        try {
            const created = await addTodo(title, "", newType);
            setTasks((prev) => [created, ...prev]);
            setNewTitle("");
            setActiveStage(created.task_type);
        } catch (err) {
            setError(err instanceof Error ? err.message : "Unable to add task.");
        } finally {
            setCreating(false);
        }
    };

    const handleDismiss = async (taskId: string) => {
        try {
            const dismissed = await dismissTodo(taskId);
            if (dismissed) {
                await loadTasks(false);
            }
        } catch (err) {
            setError(err instanceof Error ? err.message : "Unable to dismiss task.");
        }
    };

    if (!isVisible) {
        return null;
    }

    return (
        <div className="todo-page">
            <header className="todo-page-header">
                <div>
                    <h2>To‑Do List</h2>
                    <p>Stage through Todos, Reminders, and Follow-ups pulled from distinct memories.</p>
                </div>
                <div className="todo-page-actions">
                    <button
                        className="ui-action-btn todo-refresh-btn"
                        onClick={() => void loadTasks(false)}
                        disabled={loading || refreshing}
                    >
                        {refreshing ? "Refreshing..." : "Refresh"}
                    </button>
                    <button className="ui-action-btn todo-close-btn" onClick={onClose}>
                        ✕ Close
                    </button>
                </div>
            </header>

            <section className="todo-create-row">
                <input
                    type="text"
                    placeholder="Add a task..."
                    value={newTitle}
                    onChange={(event) => setNewTitle(event.target.value)}
                    onKeyDown={(event) => {
                        if (event.key === "Enter") {
                            event.preventDefault();
                            void handleAddTask();
                        }
                    }}
                />
                <div className="todo-type-toggle" role="tablist" aria-label="Task type">
                    {(["Todo", "Reminder", "Followup"] as TodoType[]).map((type) => (
                        <button
                            key={type}
                            type="button"
                            className={`ui-action-btn todo-type-btn ${newType === type ? "active" : ""}`}
                            onClick={() => setNewType(type)}
                        >
                            {type === "Followup" ? "Follow-up" : type}
                        </button>
                    ))}
                </div>
                <button
                    className="ui-action-btn todo-add-btn"
                    onClick={() => void handleAddTask()}
                    disabled={creating || !newTitle.trim()}
                >
                    {creating ? "Adding..." : "Add"}
                </button>
            </section>

            <section className="todo-stage-toggle" aria-label="Task stages">
                {(["Todo", "Reminder", "Followup", "All"] as StageFilter[]).map((stage) => (
                    <button
                        key={stage}
                        type="button"
                        className={`ui-action-btn todo-stage-btn ${activeStage === stage ? "active" : ""}`}
                        onClick={() => setActiveStage(stage)}
                    >
                        {stage === "Followup" ? "Follow-up" : stage}
                        <strong>
                            {stage === "All"
                                ? sortedTasks.length
                                : countsByType[stage]}
                        </strong>
                    </button>
                ))}
            </section>

            <div className="todo-page-body">
                {loading && (
                    <div className="todo-page-state">
                        <div className="spinner" />
                        <p>Loading tasks...</p>
                    </div>
                )}

                {!loading && error && (
                    <div className="todo-page-state">
                        <p>{error}</p>
                    </div>
                )}

                {!loading && !error && sortedTasks.length === 0 && (
                    <div className="todo-page-state">
                        <p>No active tasks yet.</p>
                    </div>
                )}

                {!loading && !error && sortedTasks.length > 0 && visibleTasks.length === 0 && (
                    <div className="todo-page-state">
                        <p>No tasks in this stage right now.</p>
                    </div>
                )}

                {!loading && !error && visibleTasks.length > 0 && (
                    <div className="todo-page-list">
                        {visibleTasks.map((task) => (
                            <article key={task.id} className="todo-page-item">
                                <div className="todo-page-item-main">
                                    <span className={`todo-pill ${task.task_type.toLowerCase()}`}>
                                        {task.task_type}
                                    </span>
                                    <h3>{task.title}</h3>
                                    <p>
                                        {new Date(task.created_at).toLocaleString()}
                                        {task.linked_memory_ids.length > 0
                                            ? ` · ${task.linked_memory_ids.length} linked memories`
                                            : ""}
                                        {task.linked_urls.length > 0
                                            ? ` · ${task.linked_urls.length} context links`
                                            : ""}
                                    </p>
                                </div>
                                <div className="todo-page-item-actions">
                                    <button
                                        className="ui-action-btn todo-done-btn"
                                        onClick={() => void handleDismiss(task.id)}
                                    >
                                        Done · Next
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
