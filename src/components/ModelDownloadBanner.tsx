import { useState, useEffect } from "react";
import { DownloadProgress, ModelInfo, listAvailableModels, downloadModel, onDownloadProgress, onDownloadLog } from "../api/onboarding";
import "./ModelDownloadBanner.css";

export function ModelDownloadBanner() {
    const [selected, setSelected] = useState<ModelInfo | null>(null);
    const [progress, setProgress] = useState<DownloadProgress | null>(null);
    const [isDownloading, setIsDownloading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [logs, setLogs] = useState<string[]>([]);

    useEffect(() => {
        listAvailableModels().then((ms) => {
            const preferred = ms.find((m) => m.recommended) ?? ms[0];
            setSelected(preferred ?? null);
        });
    }, []);

    useEffect(() => {
        let unlistenProgress: (() => void) | null = null;
        let unlistenLogs: (() => void) | null = null;

        onDownloadLog((msg) => {
            setLogs((prev) => [...prev, msg].slice(-10));
        }).then((u) => { unlistenLogs = u; });

        onDownloadProgress((p) => {
            setProgress(p);
            if (p.done && !p.error) {
                setIsDownloading(false);
                alert("AI Model downloaded successfully! Please completely quit and restart FNDR to enable AI intelligence.");
            }
            if (p.error) {
                setError(p.error);
                setIsDownloading(false);
            }
        }).then((u) => { unlistenProgress = u; });

        return () => {
            unlistenProgress?.();
            unlistenLogs?.();
        };
    }, []);

    async function handleDownload() {
        if (!selected) return;
        setError(null);
        setLogs([]);
        setIsDownloading(true);
        try {
            await downloadModel(selected.id, selected.download_url, selected.filename);
        } catch (e: unknown) {
            setError(String(e));
            setIsDownloading(false);
        }
    }

    function fmtBytes(b: number) {
        return b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${(b / 1e6).toFixed(0)} MB`;
    }

    return (
        <div className="model-download-banner">
            <div className="banner-header">
                <h3>⚠️ AI Intelligence is Disabled</h3>
                <p>
                    FNDR is currently running in OCR-only mode because the local AI model is missing. 
                    Search works, but semantic questioning and meeting summaries require the model.
                </p>
            </div>
            
            {error && <div className="banner-error">{error}</div>}

            {isDownloading && progress ? (
                <div className="banner-progress-area">
                    <div className="banner-progress-details">
                        <span>Downloading {selected?.name}...</span>
                        <span>{fmtBytes(progress.bytes_downloaded)} / {fmtBytes(progress.total_bytes)} ({progress.percent.toFixed(0)}%)</span>
                    </div>
                    <div className="banner-progress-bar">
                        <div className="banner-progress-fill" style={{ width: `${progress.percent}%` }} />
                    </div>
                </div>
            ) : isDownloading && !progress ? (
                <div className="banner-progress-area" style={{ textAlign: "center", fontStyle: "italic", opacity: 0.8 }}>
                    <span className="ob-icon pulse" style={{ marginRight: 8 }}>⚙️</span>
                    Preparing Download... Connecting to HuggingFace
                </div>
            ) : null}

            {isDownloading && logs.length > 0 && (
                <div style={{
                    marginTop: 12,
                    background: "rgba(0,0,0,0.3)",
                    padding: "8px 12px",
                    borderRadius: 6,
                    fontFamily: "monospace",
                    fontSize: 10,
                    color: "rgba(255,255,255,0.6)",
                    height: 80,
                    overflowY: "auto"
                }}>
                    {logs.map((L, i) => <div key={i}>{L}</div>)}
                </div>
            )}

            {!isDownloading && (
                <div className="banner-action-area">
                    <button className="banner-download-btn" onClick={handleDownload} disabled={!selected}>
                        Download {selected?.name} ({selected?.size_label})
                    </button>
                    <span className="banner-meta">Memory: ~{selected?.ram_gb} GB RAM</span>
                </div>
            )}
        </div>
    );
}
