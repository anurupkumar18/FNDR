import type { ActivityTraceStep } from "@/shared/activity/activityTrace";
import type { ActivityTraceEvidence } from "@/shared/activity/activityTrace";
import type { ModelDownloadStatus } from "@/shared/ipc/onboarding";
import { formatBytes } from "@/shared/utils/format";

/** Convert one observed backend download status into privacy-safe UI evidence. */
export function observedModelDownloadStep(
    status: ModelDownloadStatus & { activity_evidence?: ActivityTraceEvidence },
    modelName: string,
): ActivityTraceStep | null {
    const atMs = status.updated_at_ms > 0 ? status.updated_at_ms : Date.now();
    const evidence = status.activity_evidence ?? "backend-snapshot";
    switch (status.state) {
        case "preparing":
            return {
                id: "download",
                label: `Preparing ${modelName}`,
                actor: "Model download service",
                status: "running",
                evidence,
                atMs,
            };
        case "downloading":
            return {
                id: "download",
                label: `Downloading ${modelName}`,
                actor: "Model download service",
                status: "running",
                evidence,
                atMs,
                detail: `${status.percent.toFixed(0)}% · ${formatBytes(status.bytes_downloaded)} of ${formatBytes(status.total_bytes)}`,
            };
        case "finalizing":
            return {
                id: "download",
                label: "Verifying downloaded model",
                actor: "Model download service",
                status: "running",
                evidence,
                atMs,
            };
        case "completed":
            return {
                id: "download",
                label: `${modelName} downloaded`,
                actor: "Model download service",
                status: "completed",
                evidence,
                atMs,
            };
        case "failed":
            return {
                id: "download",
                label: "Model download failed",
                actor: "Model download service",
                status: "failed",
                evidence,
                atMs,
            };
        case "idle":
            return null;
    }
}
