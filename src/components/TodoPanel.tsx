import { useEffect, useMemo, useState } from "react";
import { Task, addTodo, dismissTodo, getTodos } from "../api/tauri";
import "./TodoPanel.css";

interface TodoPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

type TodoType = "Todo" | "Reminder" | "Followup";

export function TodoPanel({ isVisible, onClose }: TodoPanelProps) {
    const [tasks, setTasks] = useState<Task[]>([]);
    const [loading, setLoading] = useState(false);
    const [refreshing, setRefreshing] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [creating, setCreating] = useState(false);
    const [newTitle, setNewTitle] = useState("");
    const [newType, setNewType] = useState<TodoType>("Todo");

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
            setNewType("Todo");
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
                setTasks((prev) => prev.filter((task) => task.id !== taskId));
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
                    <p>Manual tasks plus smart extractions from recent memories.</p>
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

                {!loading && !error && sortedTasks.length > 0 && (
                    <div className="todo-page-list">
                        {sortedTasks.map((task) => (
                            <article key={task.id} className="todo-page-item">
                                <div className="todo-page-item-main">
                                    <span className={`todo-pill ${task.task_type.toLowerCase()}`}>
                                        {task.task_type}
                                    </span>
                                    <h3>{task.title}</h3>
                                    <p>
                                        {new Date(task.created_at).toLocaleString()} · {task.source_app}
                                    </p>
                                </div>
                                <div className="todo-page-item-actions">
                                    <button
                                        className="ui-action-btn todo-done-btn"
                                        onClick={() => void handleDismiss(task.id)}
                                    >
                                        Done
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
