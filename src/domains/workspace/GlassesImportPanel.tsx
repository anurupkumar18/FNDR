import { useState, useEffect } from "react";
import { importMetaGlassesPhoto } from "@/shared/ipc/tauri";
import "./PipelineInspectorPanel.css";

interface GlassesImportPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

export function GlassesImportPanel({ isVisible, onClose }: GlassesImportPanelProps) {
    const [busy, setBusy] = useState(false);
    const [message, setMessage] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (isVisible) {
            setMessage(null);
            setError(null);
            setBusy(false);
        }
    }, [isVisible]);

    if (!isVisible) {
        return null;
    }

    const runImport = async () => {
        setBusy(true);
        setError(null);
        setMessage(null);
        try {
            const id = await importMetaGlassesPhoto(null);
            setMessage(
                `Imported as memory ${id}. Try searching for visible text from the photo (OCR). ` +
                    "CLIP vision runs for image embedding; hybrid search still uses text first."
            );
        } catch (e) {
            setError(e instanceof Error ? e.message : String(e));
        } finally {
            setBusy(false);
        }
    };

    return (
        <div className="pipeline-panel">
            <header className="pipeline-header">
                <div>
                    <h2>Import a glasses photo</h2>
                    <p>
                        Meta Ray-Ban / AI glasses: export to Mac, then index here (JPEG, PNG, or HEIC). Uses{" "}
                        <strong>on-device Qwen3-VL vision</strong> (pixels) plus gated OCR and CLIP — install the GGUF
                        and matching <code>mmproj</code> in your models folder (see README).
                    </p>
                </div>
                <button type="button" className="ui-action-btn pipeline-close-btn" onClick={onClose}>
                    Close
                </button>
            </header>

            <div className="pipeline-body">
                <section className="pipeline-panel-card">
                    <h3>How it works</h3>
                    <ol className="glasses-import-steps">
                        <li>Sync or AirDrop the photo from your phone to this Mac.</li>
                        <li>Ensure models are installed: run <code>./scripts/download_model.sh</code> (BGE + CLIP ONNX).</li>
                        <li>Tap <strong>Choose photo…</strong> — macOS will ask which file to import.</li>
                        <li>After import, search the main bar for words that appear in the image.</li>
                    </ol>
                    <button
                        type="button"
                        className="ui-action-btn pipeline-run-btn glasses-import-primary"
                        onClick={() => void runImport()}
                        disabled={busy}
                    >
                        {busy ? "Importing…" : "Choose photo…"}
                    </button>
                    {error && <div className="pipeline-error" style={{ marginTop: 12 }}>{error}</div>}
                    {message && (
                        <div className="glasses-import-success" role="status">
                            {message}
                        </div>
                    )}
                </section>
            </div>
        </div>
    );
}
