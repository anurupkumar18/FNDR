import { useCallback, useState } from "react";
import { getRuntimeMetrics, type RuntimeMetricsSnapshot } from "@/shared/ipc/tauri";
import { usePolling } from "@/shared/hooks/usePolling";
import "./PipelineInspectorPanel.css";

interface EngineMetricsCardProps {
    /** When false, polling is disabled. */
    enabled: boolean;
    /** If set, shown above the metrics blurb (e.g. standalone panel title). */
    title?: string;
}

/**
 * Live engine latency / RSS snapshot (from `get_runtime_metrics`). Reuses Pipeline Inspector styles.
 */
export function EngineMetricsCard({ enabled, title }: EngineMetricsCardProps) {
    const [runtimeMetrics, setRuntimeMetrics] = useState<RuntimeMetricsSnapshot | null>(null);
    const [runtimeMetricsError, setRuntimeMetricsError] = useState<string | null>(null);

    const loadRuntimeMetrics = useCallback(async (isMounted: () => boolean) => {
        try {
            const snap = await getRuntimeMetrics();
            if (isMounted()) {
                setRuntimeMetrics(snap);
                setRuntimeMetricsError(null);
            }
        } catch (e) {
            if (isMounted()) {
                setRuntimeMetricsError(e instanceof Error ? e.message : String(e));
            }
        }
    }, []);

    usePolling(loadRuntimeMetrics, 3000, enabled);

    return (
        <section className="pipeline-panel-card pipeline-engine-metrics">
            {title ? <h3>{title}</h3> : <h3>Engine metrics</h3>}
            <p className="pipeline-muted">
                Latency (EWMA), hybrid search branches, capture flush, ONNX, CLIP, LLM/VLM, graph commits.
                No query text stored. RSS is this FNDR process only (macOS).
            </p>
            {runtimeMetricsError && <div className="pipeline-error">{runtimeMetricsError}</div>}
            {runtimeMetrics && (
                <>
                    <div className="pipeline-engine-kv">
                        <span>RSS</span>
                        <strong>
                            {runtimeMetrics.process_rss_bytes != null
                                ? `${(runtimeMetrics.process_rss_bytes / (1024 * 1024)).toFixed(0)} MiB`
                                : "—"}
                        </strong>
                        <span>CLIP vision</span>
                        <strong>
                            {runtimeMetrics.embedding.clip_session_loaded ? "loaded" : "idle"} · last{" "}
                            {runtimeMetrics.embedding.last_clip_infer_ms} ms
                        </strong>
                        <span>Text embed (BGE)</span>
                        <strong>
                            {runtimeMetrics.embedding.backend}
                            {runtimeMetrics.embedding.degraded ? " (degraded)" : ""}
                        </strong>
                        <span>LLM / VLM</span>
                        <strong>
                            {runtimeMetrics.inference.ai_model_loaded
                                ? runtimeMetrics.inference.loaded_model_id ?? "loaded"
                                : "not loaded"}
                        </strong>
                    </div>
                    <h4>Latency aggregates</h4>
                    <p className="pipeline-muted" style={{ marginTop: "-6px" }}>
                        Run a few searches and wait for captures to flush to see non-zero rows.
                    </p>
                    <div className="pipeline-metrics-table-wrap">
                        <table className="pipeline-metrics-table">
                            <thead>
                                <tr>
                                    <th>Operation</th>
                                    <th>n</th>
                                    <th>ewma ms</th>
                                    <th>max ms</th>
                                    <th>avg ms</th>
                                </tr>
                            </thead>
                            <tbody>
                                {Object.keys(runtimeMetrics.aggregates)
                                    .sort()
                                    .map((key) => {
                                        const row = runtimeMetrics.aggregates[key];
                                        return (
                                            <tr key={key}>
                                                <td>
                                                    <code>{key}</code>
                                                </td>
                                                <td>{row.n}</td>
                                                <td>{row.ewma_ms.toFixed(1)}</td>
                                                <td>{row.max_ms}</td>
                                                <td>{row.avg_ms.toFixed(1)}</td>
                                            </tr>
                                        );
                                    })}
                            </tbody>
                        </table>
                    </div>
                    {Object.keys(runtimeMetrics.counters).length > 0 ? (
                        <>
                            <h4>Timeouts / events</h4>
                            <ul className="pipeline-counter-list">
                                {Object.keys(runtimeMetrics.counters)
                                    .sort()
                                    .map((k) => (
                                        <li key={k}>
                                            <code>{k}</code>: {runtimeMetrics.counters[k]}
                                        </li>
                                    ))}
                            </ul>
                        </>
                    ) : null}
                    <h4>Recent samples</h4>
                    <ul className="pipeline-recent-list">
                        {(runtimeMetrics.recent ?? []).slice(0, 20).map((r, i) => (
                            <li key={`${r.ts_ms}-${i}-${r.op}`}>
                                <code>{r.op}</code> {r.ms} ms
                                {r.meta ? ` · ${r.meta}` : ""}
                            </li>
                        ))}
                    </ul>
                </>
            )}
        </section>
    );
}
