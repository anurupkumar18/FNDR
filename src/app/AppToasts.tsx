import type { AppToast } from "./types";

interface AppToastsProps {
    toasts: AppToast[];
    onAction: (toast: AppToast) => void;
    onDismiss: (toastId: string) => void;
}

export function AppToasts({ toasts, onAction, onDismiss }: AppToastsProps) {
    if (!toasts.length) {
        return null;
    }

    return (
        <div className="app-toast-stack" aria-live="polite" aria-atomic="false">
            {toasts.map((toast) => (
                <div
                    key={toast.id}
                    className={`app-toast app-toast-${toast.kind.replace(/[^a-z0-9_-]/gi, "-")}`}
                    role="status"
                >
                    <button
                        className={`app-toast-card ${toast.targetPanel ? "is-clickable" : ""}`}
                        type="button"
                        onClick={() => onAction(toast)}
                        disabled={!toast.targetPanel}
                    >
                        <div className="app-toast-copy">
                            <span className="app-toast-title">{toast.title}</span>
                            <p className="app-toast-body">{toast.body}</p>
                        </div>
                    </button>
                    <div className="app-toast-actions">
                        {toast.actionLabel && toast.targetPanel && (
                            <button
                                className="app-toast-action"
                                type="button"
                                onClick={() => onAction(toast)}
                            >
                                {toast.actionLabel}
                            </button>
                        )}
                        <button
                            className="app-toast-close"
                            type="button"
                            onClick={() => onDismiss(toast.id)}
                            aria-label={`Dismiss ${toast.title}`}
                        >
                            Close
                        </button>
                    </div>
                </div>
            ))}
        </div>
    );
}
