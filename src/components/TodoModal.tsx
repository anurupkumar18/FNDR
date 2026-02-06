import { useState, useEffect } from "react";
import { Task, getTodos, dismissTodo, executeTodo } from "../api/tauri";
import "./TodoModal.css";

interface TodoModalProps {
    isVisible: boolean;
    onExecuteTask: (task: Task) => void;
}

export function TodoModal({ isVisible, onExecuteTask }: TodoModalProps) {
    const [tasks, setTasks] = useState<Task[]>([]);
    const [isLoading, setIsLoading] = useState(true);

    useEffect(() => {
        if (isVisible) {
            loadTasks();
        }
    }, [isVisible]);

    const loadTasks = async () => {
        setIsLoading(true);
        try {
            const todos = await getTodos();
            setTasks(todos.slice(0, 5)); // Max 5 tasks
        } catch (err) {
            console.error(err);
        } finally {
            setIsLoading(false);
        }
    };

    const handleDismiss = async (taskId: string) => {
        await dismissTodo(taskId);
        setTasks(tasks.filter(t => t.id !== taskId));
    };

    const handleExecute = async (task: Task) => {
        const t = await executeTodo(task.id);
        onExecuteTask(t);
    };

    if (!isVisible) return null;

    return (
        <section className="todo-section">
            <div className="todo-header">
                <span className="todo-icon">✨</span>
                <h2>Your Tasks</h2>
                {!isLoading && tasks.length > 0 && (
                    <span className="todo-count">{tasks.length}</span>
                )}
            </div>

            {isLoading ? (
                <div className="todo-loading">
                    <div className="spinner" />
                </div>
            ) : tasks.length === 0 ? (
                <p className="todo-empty">No pending tasks</p>
            ) : (
                <ul className="todo-list">
                    {tasks.map(task => (
                        <li key={task.id} className="todo-item">
                            <span className="todo-type">
                                {task.task_type === "Reminder" ? "⏰" : "📋"}
                            </span>
                            <span className="todo-title">{task.title}</span>
                            <div className="todo-actions">
                                <button
                                    className="btn-done"
                                    onClick={() => handleDismiss(task.id)}
                                    title="Mark done"
                                >
                                    ✓
                                </button>
                                <button
                                    className="btn-run"
                                    onClick={() => handleExecute(task)}
                                    title="Run with AI"
                                >
                                    ▶
                                </button>
                            </div>
                        </li>
                    ))}
                </ul>
            )}
        </section>
    );
}
