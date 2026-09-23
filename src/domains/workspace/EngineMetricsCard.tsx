import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
    getMemoryReviewStatus,
    getRuntimeMetrics,
    type MemoryReviewWorkerStatus,
    type RuntimeMetricsSnapshot,
} from "@/shared/ipc/tauri";
import { usePolling } from "@/shared/hooks/usePolling";
import "./PipelineInspectorPanel.css";

const BYTES_PER_MIB = 1024 * 1024;
const BYTES_PER_GIB = 1024 * 1024 * 1024;

function formatBytes(bytes: number | null | undefined): string {
    if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return "—";
    if (bytes === 0) return "0 B";
    if (bytes >= BYTES_PER_GIB) return `${(bytes / BYTES_PER_GIB).toFixed(2)} GiB`;
    if (bytes >= BYTES_PER_MIB) return `${(bytes / BYTES_PER_MIB).toFixed(0)} MiB`;
    if (bytes >= 1024) return `${(bytes / 1024).toFixed(0)} KiB`;
    return `${bytes.toFixed(0)} B`;
}

function formatRate(bps: number): string {
    if (!Number.isFinite(bps) || bps <= 0) return "0 B/s";
    if (bps >= BYTES_PER_MIB) return `${(bps / BYTES_PER_MIB).toFixed(2)} MiB/s`;
    if (bps >= 1024) return `${(bps / 1024).toFixed(1)} KiB/s`;
    return `${bps.toFixed(0)} B/s`;
}

function pressureClass(label: string): string {
    if (label === "high") return "pipeline-pressure-high";
    if (label === "moderate") return "pipeline-pressure-moderate";
    return "pipeline-pressure-low";
}

function formatSnapshotTime(timestampMs: number): string {
    if (!Number.isFinite(timestampMs) || timestampMs <= 0) return "time unavailable";
    return new Date(timestampMs).toLocaleTimeString([], {
        hour: "numeric",
        minute: "2-digit",
        second: "2-digit",
    });
}

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
    const [runtimeMetricsLoading, setRuntimeMetricsLoading] = useState(enabled);
    const [reviewStatus, setReviewStatus] = useState<MemoryReviewWorkerStatus | null>(null);
    const [reviewStatusError, setReviewStatusError] = useState<string | null>(null);
    const mountedRef = useRef(true);
    const runtimeMetricsRef = useRef(runtimeMetrics);
    runtimeMetricsRef.current = runtimeMetrics;

    useEffect(() => {
        mountedRef.current = true;
        return () => {
            mountedRef.current = false;
        };
    }, []);

    useEffect(() => {
        if (!enabled) {
            setRuntimeMetricsLoading(false);
        } else if (!runtimeMetricsRef.current && !runtimeMetricsError) {
            setRuntimeMetricsLoading(true);
        }
    }, [enabled, runtimeMetricsError]);

    const loadRuntimeMetrics = useCallback(async (isMounted: () => boolean) => {
        if (isMounted() && !runtimeMetricsRef.current) setRuntimeMetricsLoading(true);
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
        } finally {
            if (isMounted()) setRuntimeMetricsLoading(false);
        }
    }, []);

    const loadReviewStatus = useCallback(async (isMounted: () => boolean) => {
        try {
            const status = await getMemoryReviewStatus();
            if (isMounted()) {
                setReviewStatus(status);
                setReviewStatusError(null);
            }
        } catch (reason) {
            if (isMounted()) {
                setReviewStatusError(reason instanceof Error ? reason.message : String(reason));
            }
        }
    }, []);

    usePolling(loadRuntimeMetrics, 3000, enabled);
    usePolling(loadReviewStatus, 5000, enabled);

    const system = runtimeMetrics?.system;
    const modelMemory = useMemo(() => {
        return (system?.model_memory ?? []).slice().sort((a, b) => b.estimated_bytes - a.estimated_bytes);
    }, [system?.model_memory]);

    return (
        <section className="pipeline-panel-card pipeline-engine-metrics" aria-busy={runtimeMetricsLoading}>
            {title ? <h3>{title}</h3> : <h3>Engine diagnostics</h3>}
            <p className="pipeline-muted">
                Best-effort local measurements for troubleshooting CPU, memory, models, disk I/O,
                and pipeline latency. Values can differ from Activity Monitor. Query text is not
                included in this view.
            </p>

            {runtimeMetricsLoading && !runtimeMetrics && !runtimeMetricsError && (
                <div className="pipeline-diagnostic-state" role="status" aria-live="polite">
                    Loading live engine diagnostics…
                </div>
            )}

            {runtimeMetricsError && (
                <div className="pipeline-error pipeline-diagnostic-error" role="alert">
                    <div>
                        <strong>
                            {runtimeMetrics
                                ? "Live refresh failed. Showing the last successful snapshot."
                                : "Engine diagnostics could not load."}
                        </strong>
                        <p>{runtimeMetricsError}</p>
                    </div>
                    <button
                        type="button"
                        className="ui-action-btn"
                        aria-label="Retry engine diagnostics"
                        onClick={() => void loadRuntimeMetrics(() => mountedRef.current)}
                    >
                        Retry
                    </button>
                </div>
            )}

            {runtimeMetrics && (
                <>
                    <p className="pipeline-snapshot-meta" role="status">
                        Snapshot captured {formatSnapshotTime(runtimeMetrics.generated_at_ms)} · refreshes
                        every 3 seconds while this view is open
                    </p>

                    <dl className="pipeline-engine-kv">
                        <dt>Resident memory (RSS)</dt>
                        <dd>{formatBytes(runtimeMetrics.process_rss_bytes)}</dd>
                        <dt>Captured frames</dt>
                        <dd>
                            {runtimeMetrics.capture.frames_captured} captured ·{" "}
                            {runtimeMetrics.capture.frames_dropped} dropped
                        </dd>
                        <dt>CLIP vision</dt>
                        <dd>
                            {runtimeMetrics.embedding.clip_session_loaded ? "loaded" : "idle"} · last{" "}
                            {runtimeMetrics.embedding.last_clip_infer_ms} ms
                        </dd>
                        <dt>Text embedding</dt>
                        <dd>
                            {runtimeMetrics.embedding.backend}
                            {runtimeMetrics.embedding.degraded ? " (degraded)" : ""}
                        </dd>
                        <dt>LLM / VLM</dt>
                        <dd>
                            {runtimeMetrics.inference.ai_model_loaded
                                ? runtimeMetrics.inference.loaded_model_id ?? "loaded"
                                : "not loaded"}
                        </dd>
                        <dt>Memory review</dt>
                        {reviewStatus ? (
                            <dd
                                data-testid="memory-review-status"
                                title={
                                    reviewStatus.last_error_kind
                                        ? `last error: ${reviewStatus.last_error_kind}`
                                        : reviewStatus.pressure_blocked
                                          ? "blocked by system pressure or inference availability"
                                          : reviewStatus.worker_enabled
                                            ? "running"
                                            : "disabled"
                                }
                            >
                                {reviewStatus.worker_enabled
                                    ? reviewStatus.pressure_blocked
                                        ? "deferred"
                                        : "running"
                                    : "off"}
                                {" · queue "}
                                {reviewStatus.queue_depth}
                            </dd>
                        ) : reviewStatusError ? (
                            <dd aria-describedby="engine-review-status-error">
                                unavailable
                                <span id="engine-review-status-error" className="pipeline-sr-only">
                                    {reviewStatusError}
                                </span>
                            </dd>
                        ) : (
                            <dd>checking…</dd>
                        )}
                    </dl>

                    {system && (
                        <>
                            <h4>FNDR process</h4>
                            <dl className="pipeline-engine-kv pipeline-engine-kv--metrics">
                                <dt>CPU</dt>
                                <dd>
                                    {system.process_cpu.cpu_percent.toFixed(1)}% ·{" "}
                                    {system.process_cpu.threads} threads
                                </dd>
                                <dt>Memory (resident)</dt>
                                <dd>{formatBytes(system.process_memory.rss_bytes)}</dd>
                                <dt>Physical footprint</dt>
                                <dd>
                                    {formatBytes(system.process_memory.phys_footprint_bytes)} · peak{" "}
                                    {formatBytes(system.process_memory.lifetime_max_phys_footprint_bytes)}
                                </dd>
                                <dt>Disk I/O rate</dt>
                                <dd>
                                    Read {formatRate(system.process_io.disk_read_rate_bps)} · write{" "}
                                    {formatRate(system.process_io.disk_write_rate_bps)}
                                </dd>
                                <dt>Energy estimate</dt>
                                <dd className={pressureClass(system.process_energy.label)}>
                                    {system.process_energy.label} · {system.process_energy.idle_wakeups} idle
                                    wakeups
                                </dd>
                            </dl>

                            <h4>Host system</h4>
                            <dl className="pipeline-engine-kv pipeline-engine-kv--metrics">
                                <dt>CPU (all cores)</dt>
                                <dd>{system.host_cpu.cpu_percent_total.toFixed(1)}%</dd>
                                <dt>Memory pressure</dt>
                                <dd className={pressureClass(system.host_memory.pressure_label)}>
                                    {system.host_memory.pressure_label} · {formatBytes(system.host_memory.free_bytes)}{" "}
                                    free
                                </dd>
                                <dt>Memory breakdown</dt>
                                <dd className="pipeline-memory-breakdown">
                                    wired {formatBytes(system.host_memory.wired_bytes)} · active{" "}
                                    {formatBytes(system.host_memory.active_bytes)} · inactive{" "}
                                    {formatBytes(system.host_memory.inactive_bytes)} · compressed{" "}
                                    {formatBytes(system.host_memory.compressed_bytes)}
                                </dd>
                            </dl>

                            {system.host_cpu.cpu_percent_per_core.length > 0 && (
                                <div className="pipeline-cpu-cores" aria-label="Per-core CPU usage">
                                    {system.host_cpu.cpu_percent_per_core.map((pct, idx) => {
                                        const clamped = Math.max(0, Math.min(100, pct));
                                        return (
                                            <div
                                                className="pipeline-cpu-core"
                                                key={`core-${idx}`}
                                                role="meter"
                                                aria-label={`CPU core ${idx} usage`}
                                                aria-valuemin={0}
                                                aria-valuemax={100}
                                                aria-valuenow={clamped}
                                                aria-valuetext={`${pct.toFixed(1)} percent`}
                                            >
                                                <div className="pipeline-cpu-core-bar" aria-hidden="true">
                                                    <div
                                                        className="pipeline-cpu-core-fill"
                                                        style={{ height: `${clamped}%` }}
                                                    />
                                                </div>
                                                <div className="pipeline-cpu-core-label" aria-hidden="true">{idx}</div>
                                            </div>
                                        );
                                    })}
                                </div>
                            )}

                            <h4>GPU</h4>
                            <dl className="pipeline-engine-kv pipeline-engine-kv--metrics">
                                <dt>Device utilization</dt>
                                <dd>
                                    {system.gpu.device_utilization_percent != null
                                        ? `${system.gpu.device_utilization_percent.toFixed(0)}%`
                                        : "Unavailable"}
                                </dd>
                                <dt>Renderer</dt>
                                <dd>
                                    {system.gpu.renderer_utilization_percent != null
                                        ? `${system.gpu.renderer_utilization_percent.toFixed(0)}%`
                                        : "Unavailable"}
                                </dd>
                                <dt>In-use system memory</dt>
                                <dd>{formatBytes(system.gpu.in_use_system_memory_bytes)}</dd>
                                <dt>Recoveries</dt>
                                <dd>{system.gpu.recovery_count ?? "Unavailable"}</dd>
                            </dl>

                            <h4>Model memory</h4>
                            <ul className="pipeline-model-list">
                                {modelMemory.length === 0 ? (
                                    <li className="pipeline-muted">No model memory is currently tracked.</li>
                                ) : (
                                    modelMemory.map((model) => (
                                        <li key={`${model.kind}-${model.id}`}>
                                            <span
                                                className={`pipeline-model-dot pipeline-model-dot--${model.kind}`}
                                                aria-hidden="true"
                                            />
                                            <code>{model.id}</code>
                                            <span className="pipeline-model-kind">{model.kind}</span>
                                            <span className="pipeline-model-state">
                                                {model.loaded ? "loaded" : "idle"}
                                            </span>
                                            <strong>
                                                {model.loaded ? formatBytes(model.estimated_bytes) : "—"}
                                            </strong>
                                        </li>
                                    ))
                                )}
                            </ul>
                        </>
                    )}

                    <h4>Latency aggregates</h4>
                    <p className="pipeline-muted pipeline-section-help">
                        Samples appear after searches, capture flushes, or model work completes.
                    </p>
                    <div className="pipeline-metrics-table-wrap">
                        <table className="pipeline-metrics-table">
                            <caption className="pipeline-sr-only">Recorded engine latency aggregates</caption>
                            <thead>
                                <tr>
                                    <th scope="col">Operation</th>
                                    <th scope="col">Samples</th>
                                    <th scope="col"><abbr title="Exponentially weighted moving average">EWMA</abbr> ms</th>
                                    <th scope="col">p50 ms</th>
                                    <th scope="col">p95 ms</th>
                                    <th scope="col">Max ms</th>
                                    <th scope="col">Average ms</th>
                                </tr>
                            </thead>
                            <tbody>
                                {Object.keys(runtimeMetrics.aggregates).length === 0 ? (
                                    <tr>
                                        <td colSpan={7} className="pipeline-table-empty">
                                            No latency samples yet.
                                        </td>
                                    </tr>
                                ) : (
                                    Object.keys(runtimeMetrics.aggregates)
                                        .sort()
                                        .map((key) => {
                                            const row = runtimeMetrics.aggregates[key];
                                            return (
                                                <tr key={key}>
                                                    <td><code>{key}</code></td>
                                                    <td>{row.n}</td>
                                                    <td>{row.ewma_ms.toFixed(1)}</td>
                                                    <td>{row.p50_ms}</td>
                                                    <td>{row.p95_ms}</td>
                                                    <td>{row.max_ms}</td>
                                                    <td>{row.avg_ms.toFixed(1)}</td>
                                                </tr>
                                            );
                                        })
                                )}
                            </tbody>
                        </table>
                    </div>
                    {Object.keys(runtimeMetrics.counters).length > 0 && (
                        <>
                            <h4>Timeouts and events</h4>
                            <ul className="pipeline-counter-list">
                                {Object.keys(runtimeMetrics.counters)
                                    .sort()
                                    .map((key) => (
                                        <li key={key}>
                                            <code>{key}</code><strong>{runtimeMetrics.counters[key]}</strong>
                                        </li>
                                    ))}
                            </ul>
                        </>
                    )}
                    <h4>Recent operations</h4>
                    {runtimeMetrics.recent.length === 0 ? (
                        <p className="pipeline-muted pipeline-empty-state">No recent operations yet.</p>
                    ) : (
                        <ul className="pipeline-recent-list">
                            {runtimeMetrics.recent.slice(0, 20).map((sample, index) => (
                                <li key={`${sample.ts_ms}-${index}-${sample.op}`}>
                                    <code>{sample.op}</code> {sample.ms} ms
                                    {sample.meta ? ` · ${sample.meta}` : ""}
                                </li>
                            ))}
                        </ul>
                    )}
                </>
            )}
        </section>
    );
}
