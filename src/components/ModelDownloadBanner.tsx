import { useState, useEffect } from "react";
import { DownloadProgress, ModelInfo, listAvailableModels, downloadModel, onDownloadProgress } from "../api/onboarding";
import "./ModelDownloadBanner.css";

export function ModelDownloadBanner() {
    const [selected, setSelected] = useState<ModelInfo | null>(null);
    const [progress, setProgress] = useState<DownloadProgress | null>(null);
    const [isDownloading, setIsDownloading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        listAvailableModels().then((ms) => {
            const preferred = ms.find((m) => m.recommended) ?? ms[0];
            setSelected(preferred ?? null);
        });
    }, []);

    useEffect(() => {
        let unlisten: (() => void) | null = null;
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
        }).then((u) => { unlisten = u; });
        return () => { unlisten?.(); };
    }, []);

    async function handleDownload() {
        if (!selected) return;
        setError(null);
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
            ) : (
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
