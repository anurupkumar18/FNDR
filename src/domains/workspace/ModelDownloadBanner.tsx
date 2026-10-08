import { useCallback, useEffect, useRef, useState } from "react";
import {
    downloadModel,
    listAvailableModels,
    refreshAiModels,
} from "@/shared/ipc/onboarding";
import type { ModelInfo } from "@/shared/ipc/onboarding";
import { useModelDownloadStatus } from "@/shared/hooks/useModelDownloadStatus";
import {
    beginActivityTrace,
    recordActivityStep,
} from "@/shared/activity/activityTrace";
import type { ActivityTraceSnapshot, ActivityTraceStep } from "@/shared/activity/activityTrace";
import { formatBytes } from "@/shared/utils/format";
import { Icon } from "@/shared/components/atoms";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import { observedModelDownloadStep } from "./modelDownloadActivity";
import "./ModelDownloadBanner.css";

export function ModelDownloadBanner() {
    const [selected, setSelected] = useState<ModelInfo | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [pendingModelId, setPendingModelId] = useState<string | null>(null);
    const [isActivatingModel, setIsActivatingModel] = useState(false);
    const [activityTrace, setActivityTrace] = useState<ActivityTraceSnapshot | null>(null);
    const downloadStatus = useModelDownloadStatus();
    const mountedRef = useRef(true);
    const activatingModelIdRef = useRef<string | null>(null);
    const statusGenerationFloorRef = useRef<number | null>(null);

    useEffect(() => {
        mountedRef.current = true;
        return () => {
            mountedRef.current = false;
        };
    }, []);

    const startActivity = useCallback((step: ActivityTraceStep) => {
        setActivityTrace(recordActivityStep(
            beginActivityTrace({
                id: `local-model-${step.atMs}`,
                title: "Local model activity",
                startedAtMs: step.atMs,
            }),
            step,
        ));
    }, []);

    const recordActivity = useCallback((step: ActivityTraceStep) => {
        setActivityTrace((current) => recordActivityStep(
            current ?? beginActivityTrace({
                id: `local-model-${step.atMs}`,
                title: "Local model activity",
                startedAtMs: step.atMs,
            }),
            step,
        ));
    }, []);

    const loadModels = useCallback(async () => {
        const ms = await listAvailableModels();
        setSelected((currentSelected) => {
            const preferred = ms.find((m) => m.recommended) ?? ms[0];
            if (currentSelected) {
                return ms.find((model) => model.id === currentSelected.id) ?? preferred ?? null;
            }
            return preferred ?? null;
        });
    }, []);

    useEffect(() => {
        loadModels().catch((loadError) => {
            setError(`Failed to load models: ${String(loadError)}`);
        });
    }, [loadModels]);

    useEffect(() => {
        if (
            pendingModelId
            || !downloadStatus.model_id
            || !["preparing", "downloading", "finalizing"].includes(downloadStatus.state)
        ) {
            return;
        }
        const modelName = selected?.id === downloadStatus.model_id
            ? selected.name
            : "Local AI model";
        const observedStep = observedModelDownloadStep(downloadStatus, modelName);
        if (!observedStep) return;
        statusGenerationFloorRef.current = null;
        setPendingModelId(downloadStatus.model_id);
        startActivity(observedStep);
    }, [downloadStatus, pendingModelId, selected, startActivity]);

    useEffect(() => {
        if (!pendingModelId || downloadStatus.model_id !== pendingModelId) {
            return;
        }
        if (
            statusGenerationFloorRef.current !== null
            && downloadStatus.updated_at_ms <= statusGenerationFloorRef.current
        ) {
            return;
        }

        const modelName = selected?.id === pendingModelId ? selected.name : "Local AI model";
        const observedStep = observedModelDownloadStep(downloadStatus, modelName);
        if (observedStep) {
            recordActivity(observedStep);
        }

        if (downloadStatus.state === "failed") {
            statusGenerationFloorRef.current = null;
            setError("The model download failed. Retry it or check Privacy Activity for diagnostic details.");
            setPendingModelId(null);
            void loadModels();
            return;
        }

        if (downloadStatus.state !== "completed" || downloadStatus.error) {
            return;
        }

        const completedModelId = downloadStatus.model_id ?? pendingModelId;
        if (activatingModelIdRef.current === completedModelId) {
            return;
        }
        activatingModelIdRef.current = completedModelId;
        statusGenerationFloorRef.current = null;
        setPendingModelId(null);
        setIsActivatingModel(true);
        recordActivity({
            id: "activation",
            label: "Loading the model into FNDR",
            actor: "Local inference",
            status: "running",
            evidence: "ipc-boundary",
            atMs: Date.now(),
        });

        void (async () => {
            try {
                const runtime = await refreshAiModels();
                if (!mountedRef.current || activatingModelIdRef.current !== completedModelId) {
                    return;
                }
                if (!runtime.ai_model_available) {
                    setError("The download finished, but FNDR could not activate the local model. Retry loading it or check Privacy Activity.");
                    recordActivity({
                        id: "activation",
                        label: "Local model was not available after refresh",
                        actor: "Local inference",
                        status: "degraded",
                        evidence: "result-metadata",
                        atMs: Date.now(),
                    });
                } else {
                    recordActivity({
                        id: "activation",
                        label: "Local model is ready",
                        actor: "Local inference",
                        status: "completed",
                        evidence: "result-metadata",
                        atMs: Date.now(),
                    });
                }
            } catch {
                if (mountedRef.current && activatingModelIdRef.current === completedModelId) {
                    setError("The model downloaded, but FNDR could not refresh the local inference runtime. Retry loading it.");
                    recordActivity({
                        id: "activation",
                        label: "Local inference refresh failed",
                        actor: "Local inference",
                        status: "degraded",
                        evidence: "ipc-boundary",
                        atMs: Date.now(),
                    });
                }
            } finally {
                if (activatingModelIdRef.current === completedModelId) {
                    activatingModelIdRef.current = null;
                }
                if (mountedRef.current) {
                    setIsActivatingModel(false);
                    void loadModels();
                }
            }
        })();
    }, [downloadStatus, loadModels, pendingModelId, recordActivity, selected]);

    async function handleDownload() {
        if (!selected) return;
        setError(null);

        if (selected.download_url === "already_downloaded") {
            startActivity({
                id: "activation",
                label: `Loading ${selected.name} into FNDR`,
                actor: "Local inference",
                status: "running",
                evidence: "ipc-boundary",
                atMs: Date.now(),
            });
            setIsActivatingModel(true);
            try {
                const runtime = await refreshAiModels();
                if (!runtime.ai_model_available) {
                    setError("Qwen is supposed to be on disk, but FNDR could not find the local model files.");
                    recordActivity({
                        id: "activation",
                        label: "Local model was not available after refresh",
                        actor: "Local inference",
                        status: "degraded",
                        evidence: "result-metadata",
                        atMs: Date.now(),
                    });
                } else {
                    recordActivity({
                        id: "activation",
                        label: "Local model is ready",
                        actor: "Local inference",
                        status: "completed",
                        evidence: "result-metadata",
                        atMs: Date.now(),
                    });
                }
            } catch {
                setError("FNDR could not refresh the local inference runtime. Retry loading the model.");
                recordActivity({
                    id: "activation",
                    label: "Local inference refresh failed",
                    actor: "Local inference",
                    status: "degraded",
                    evidence: "ipc-boundary",
                    atMs: Date.now(),
                });
            } finally {
                setIsActivatingModel(false);
                void loadModels();
            }
            return;
        }

        setPendingModelId(selected.id);
        statusGenerationFloorRef.current = downloadStatus.updated_at_ms;
        startActivity({
            id: "download",
            label: `Requesting ${selected.name}`,
            actor: "FNDR",
            status: "running",
            evidence: "ipc-boundary",
            atMs: Date.now(),
        });
        try {
            await downloadModel(selected.id, selected.download_url, selected.filename);
            recordActivity({
                id: "download",
                label: `${selected.name} download accepted`,
                actor: "Model download service",
                status: "waiting",
                evidence: "ipc-boundary",
                atMs: Date.now(),
            });
        } catch {
            statusGenerationFloorRef.current = null;
            setError("FNDR could not start the model download. Retry it or check your network connection.");
            setPendingModelId(null);
            recordActivity({
                id: "download",
                label: "Model download request failed",
                actor: "Model download service",
                status: "failed",
                evidence: "ipc-boundary",
                atMs: Date.now(),
            });
        }
    }

    const activeDownloadStatus =
        pendingModelId && downloadStatus.model_id === pendingModelId ? downloadStatus : null;
    const isDownloading =
        isActivatingModel ||
        (activeDownloadStatus !== null &&
            ["preparing", "downloading", "finalizing"].includes(activeDownloadStatus.state));
    const alreadyDownloaded = selected?.download_url === "already_downloaded";
    const activeModelName =
        (activeDownloadStatus && selected?.id === activeDownloadStatus.model_id ? selected.name : selected?.name)
            ?? "AI model";

    return (
        <div className="model-download-banner">
            <div className="banner-header">
                <h3><Icon name="alert-triangle" size={15} /> Qwen Model Required</h3>
                <p>
                    FNDR is in OCR-only mode because the required local Qwen3-VL model is missing.
                    Search still works, but memory Q&A, summaries, and smarter indexing need the core model on disk.
                </p>
            </div>

            {error && <div className="banner-error">{error}</div>}

            {activityTrace && <ActivityTrace trace={activityTrace} className="model-download-trace" />}

            {isDownloading && activeDownloadStatus?.state === "downloading" ? (
                <div className="banner-progress-area">
                    <div className="banner-progress-details">
                        <span>Downloading {activeModelName}...</span>
                        <span>
                            {formatBytes(activeDownloadStatus.bytes_downloaded)} / {formatBytes(activeDownloadStatus.total_bytes)} ({activeDownloadStatus.percent.toFixed(0)}%)
                        </span>
                    </div>
                    <div className="banner-progress-bar">
                        <div className="banner-progress-fill" style={{ width: `${activeDownloadStatus.percent}%` }} />
                    </div>
                </div>
            ) : isDownloading ? (
                <div className="banner-progress-area" style={{ textAlign: "center", fontStyle: "italic", opacity: 0.8 }}>
                    <span className="ob-icon pulse" style={{ marginRight: 8 }}><Icon name="settings" size={16} /></span>
                    {isActivatingModel
                        ? "Loading model into FNDR"
                        : activeDownloadStatus?.state === "finalizing"
                            ? "Finalizing model on disk"
                            : "Preparing download and connecting to HuggingFace"}
                </div>
            ) : null}

            {!isDownloading && (
                <div className="banner-action-area">
                    <button className="banner-download-btn" onClick={handleDownload} disabled={!selected}>
                        {alreadyDownloaded
                            ? `Load ${selected?.name ?? "Qwen"}`
                            : `Download ${selected?.name ?? "Qwen"} (${selected?.size_label ?? ""})`}
                    </button>
                    <span className="banner-meta">Memory: ~{selected?.ram_gb} GB RAM</span>
                </div>
            )}
        </div>
    );
}
