import { type KeyboardEvent as ReactKeyboardEvent, useEffect, useRef } from "react";
import { EngineMetricsCard } from "./EngineMetricsCard";
import "./PipelineInspectorPanel.css";

interface EngineMetricsPanelProps {
    isVisible: boolean;
    onClose: () => void;
    /** Optional: jump to full pipeline inspector from this screen. */
    onOpenPipelineInspector?: () => void;
}

export function EngineMetricsPanel({ isVisible, onClose, onOpenPipelineInspector }: EngineMetricsPanelProps) {
    const closeButtonRef = useRef<HTMLButtonElement | null>(null);
    const invokerRef = useRef<HTMLElement | null>(null);

    useEffect(() => {
        if (!isVisible) return;
        const activeElement = document.activeElement;
        invokerRef.current = activeElement instanceof HTMLElement ? activeElement : null;
        closeButtonRef.current?.focus();

        return () => {
            const invoker = invokerRef.current;
            invokerRef.current = null;
            if (invoker?.isConnected) invoker.focus();
        };
    }, [isVisible]);

    if (!isVisible) {
        return null;
    }

    const handleKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
        if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            onClose();
            return;
        }
        if (event.key !== "Tab") return;

        const focusable = Array.from(
            event.currentTarget.querySelectorAll<HTMLElement>(
                'button:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
            ),
        ).filter((element) => !element.hasAttribute("hidden"));
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (!first || !last) return;
        if (event.shiftKey && document.activeElement === first) {
            event.preventDefault();
            last.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
            event.preventDefault();
            first.focus();
        }
    };

    return (
        <div
            className="pipeline-panel pipeline-panel--engine"
            role="dialog"
            aria-modal="true"
            aria-labelledby="engine-diagnostics-title"
            aria-describedby="engine-diagnostics-description"
            onKeyDown={handleKeyDown}
        >
            <header className="pipeline-header">
                <div>
                    <span className="pipeline-header-kicker">DEVELOPER TOOL</span>
                    <h2 id="engine-diagnostics-title">Engine diagnostics</h2>
                    <p id="engine-diagnostics-description">
                        A developer diagnostic for troubleshooting FNDR performance. It observes local
                        runtime counters and does not change capture or model behavior.
                    </p>
                </div>
                <button
                    ref={closeButtonRef}
                    type="button"
                    className="ui-action-btn pipeline-close-btn"
                    onClick={onClose}
                    aria-label="Close engine diagnostics"
                >
                    Close
                </button>
            </header>

            <div className="pipeline-body">
                <EngineMetricsCard enabled={isVisible} title="Live engine snapshot" />

                {onOpenPipelineInspector && (
                    <section className="pipeline-panel-card pipeline-deep-dive-card">
                        <h3>Developer deep dive</h3>
                        <p className="pipeline-muted">
                            Run a query trace and compare raw retrieval hits with user-facing memory cards.
                        </p>
                        <button type="button" className="ui-action-btn pipeline-run-btn" onClick={onOpenPipelineInspector}>
                            Open Pipeline Inspector
                        </button>
                    </section>
                )}
            </div>
        </div>
    );
}
