import { SystemReadiness } from "../api/tauri";
import "./ReadinessPanel.css";

function StatusRow({ ok, label, detail }: { ok: boolean; label: string; detail: string }) {
    return (
        <div className={`readiness-row ${ok ? "ok" : "bad"}`}>
            <span className="readiness-icon" aria-hidden>
                {ok ? "✓" : "!"}
            </span>
            <div>
                <div className="readiness-label">{label}</div>
                <div className="readiness-detail">{detail}</div>
            </div>
        </div>
    );
}

interface ReadinessPanelProps {
    readiness: SystemReadiness | null;
}

export function ReadinessPanel({ readiness }: ReadinessPanelProps) {
    if (!readiness) {
        return (
            <div className="readiness-panel readiness-loading" role="status">
                <div className="spinner tiny" />
                <span>Checking system readiness…</span>
            </div>
        );
    }

    const cs = readiness.capture_status;

    return (
        <aside className="readiness-panel" aria-label="Startup health">
            <div className="readiness-header">
                <strong>Readiness</strong>
                <span className={`readiness-pill ${readiness.ready_for_search ? "ok" : "warn"}`}>
                    {readiness.ready_for_search ? "Search ready" : "Blocked"}
                </span>
            </div>
            <div className="readiness-grid">
                <StatusRow
                    ok={readiness.screen_capture_permission_granted}
                    label="Screen Recording"
                    detail={readiness.screen_capture_permission_detail}
                />
                <StatusRow ok={readiness.ocr_available} label="OCR (Vision)" detail={readiness.ocr_detail} />
                <StatusRow
                    ok={readiness.embedder_ready}
                    label="Embeddings"
                    detail={readiness.embedder_ready ? "Embedder OK" : "Embedder failed"}
                />
                <StatusRow
                    ok={readiness.vector_store_ready}
                    label="Vector store (LanceDB)"
                    detail={readiness.vector_store_ready ? "Database OK" : "Store error"}
                />
                <StatusRow
                    ok={readiness.data_dir_writable}
                    label="App data directory"
                    detail={readiness.data_dir_detail}
                />
                <StatusRow
                    ok={!readiness.use_demo_data_only || readiness.ready_for_search}
                    label="Capture mode"
                    detail={
                        readiness.use_demo_data_only
                            ? "Demo data only — live capture ingestion paused"
                            : cs.is_paused
                              ? "Capture paused"
                              : "Live capture active"
                    }
                />
            </div>
            <div className="readiness-stats">
                Indexed records: <strong>{readiness.total_records}</strong>
                {readiness.vlm_active ? " · VLM on" : " · VLM off"}
            </div>
            {readiness.fixes.length > 0 && (
                <ul className="readiness-fixes">
                    {readiness.fixes.map((f) => (
                        <li key={f}>{f}</li>
                    ))}
                </ul>
            )}
        </aside>
    );
}
