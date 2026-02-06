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
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (isVisible) {
            loadTasks();
        }
    }, [isVisible]);

    const loadTasks = async () => {
        setIsLoading(true);
        setError(null);
        try {
            const todos = await getTodos();
            setTasks(todos);
        } catch (err) {
            setError("Failed to load tasks");
            console.error(err);
        } finally {
            setIsLoading(false);
        }
    };

    const handleDismiss = async (taskId: string) => {
        try {
            await dismissTodo(taskId);
            setTasks(tasks.filter(t => t.id !== taskId));
        } catch (err) {
            console.error("Failed to dismiss task:", err);
        }
    };

    const handleExecute = async (task: Task) => {
        try {
            const taskToExecute = await executeTodo(task.id);
            onExecuteTask(taskToExecute);
        } catch (err) {
            console.error("Failed to execute task:", err);
        }
    };

    if (!isVisible) return null;

    const getTaskIcon = (type: string) => {
        switch (type) {
            case "Todo": return "📋";
            case "Reminder": return "⏰";
            case "Followup": return "📞";
            default: return "✅";
        }
    };

    const getTaskTypeClass = (type: string) => {
        return `task-type-${type.toLowerCase()}`;
    };

    return (
        <div className="todo-modal">
            <div className="todo-header">
                <h2>
                    <span className="todo-icon">✨</span>
                    Your Tasks
                </h2>
                <p className="todo-subtitle">
                    AI-extracted from your recent activity
                </p>
            </div>

            <div className="todo-content">
                {isLoading ? (
                    <div className="todo-loading">
                        <div className="todo-spinner" />
                        <span>Analyzing your memories...</span>
                    </div>
                ) : error ? (
                    <div className="todo-error">
                        <span>⚠️ {error}</span>
                        <button onClick={loadTasks}>Retry</button>
                    </div>
                ) : tasks.length === 0 ? (
                    <div className="todo-empty">
                        <span className="empty-icon">🎉</span>
                        <h3>All caught up!</h3>
                        <p>No pending tasks detected from your recent activity.</p>
                    </div>
                ) : (
                    <div className="todo-list">
                        {tasks.map(task => (
                            <div
                                key={task.id}
                                className={`todo-card ${getTaskTypeClass(task.task_type)}`}
                            >
                                <div className="todo-card-header">
                                    <span className="task-icon">
                                        {getTaskIcon(task.task_type)}
                                    </span>
                                    <span className="task-type-badge">
                                        {task.task_type}
                                    </span>
                                </div>
                                <h3 className="task-title">{task.title}</h3>
                                {task.description && (
                                    <p className="task-description">{task.description}</p>
                                )}
                                <div className="task-meta">
                                    <span className="task-source">
                                        From: {task.source_app}
                                    </span>
                                </div>
                                <div className="task-actions">
                                    <button
                                        className="btn-dismiss"
                                        onClick={() => handleDismiss(task.id)}
                                    >
                                        Dismiss
                                    </button>
                                    <button
                                        className="btn-execute"
                                        onClick={() => handleExecute(task)}
                                    >
                                        <span>🤖</span>
                                        Execute with CUA
                                    </button>
                                </div>
                            </div>
                        ))}
                    </div>
                )}
            </div>

            <button className="todo-refresh" onClick={loadTasks}>
                🔄 Refresh Tasks
            </button>
        </div>
    );
}
