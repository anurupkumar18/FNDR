import { EngineMetricsCard } from "./EngineMetricsCard";
import "./PipelineInspectorPanel.css";

interface EngineMetricsPanelProps {
    isVisible: boolean;
    onClose: () => void;
    /** Optional: jump to full pipeline inspector from this screen. */
    onOpenPipelineInspector?: () => void;
}

export function EngineMetricsPanel({ isVisible, onClose, onOpenPipelineInspector }: EngineMetricsPanelProps) {
    if (!isVisible) {
        return null;
    }

    return (
        <div className="pipeline-panel">
            <header className="pipeline-header">
                <div>
                    <h2>Engine metrics</h2>
                    <p>Live performance snapshot for search, capture, embeddings, and memory graph.</p>
                </div>
                <button type="button" className="ui-action-btn pipeline-close-btn" onClick={onClose}>
                    Close
                </button>
            </header>

            <div className="pipeline-body">
                <EngineMetricsCard enabled={isVisible} />

                {onOpenPipelineInspector && (
                    <section className="pipeline-panel-card" style={{ marginTop: 12 }}>
                        <h3>Need the full pipeline trace?</h3>
                        <p className="pipeline-muted">
                            Open Pipeline Inspector to run a query trace, inspect raw hits vs cards, and quality
                            tools.
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
