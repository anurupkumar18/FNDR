import { useEffect, useState } from "react";
import type { ActivityTraceEvidence } from "@/shared/activity/activityTrace";
import {
    ModelDownloadStatus,
    getModelDownloadStatus,
    onDownloadStatus,
} from "@/shared/ipc/onboarding";

const EMPTY_DOWNLOAD_STATUS: ModelDownloadStatus = {
    state: "idle",
    model_id: null,
    filename: null,
    download_url: null,
    destination_path: null,
    temp_path: null,
    bytes_downloaded: 0,
    total_bytes: 0,
    percent: 0,
    done: false,
    error: null,
    logs: [],
    updated_at_ms: 0,
};

export type ObservedModelDownloadStatus = ModelDownloadStatus & {
    activity_evidence: Extract<ActivityTraceEvidence, "backend-event" | "backend-snapshot">;
};

function observeStatus(
    status: ModelDownloadStatus,
    activityEvidence: ObservedModelDownloadStatus["activity_evidence"],
): ObservedModelDownloadStatus {
    return { ...status, activity_evidence: activityEvidence };
}

export function preferNewestModelDownloadStatus(
    current: ObservedModelDownloadStatus,
    next: ObservedModelDownloadStatus,
): ObservedModelDownloadStatus {
    if (next.updated_at_ms < current.updated_at_ms) return current;
    if (
        next.updated_at_ms === current.updated_at_ms
        && current.activity_evidence === "backend-event"
        && next.activity_evidence === "backend-snapshot"
    ) {
        return current;
    }
    return next;
}

export function useModelDownloadStatus(): ObservedModelDownloadStatus {
    const [status, setStatus] = useState<ObservedModelDownloadStatus>(() =>
        observeStatus(EMPTY_DOWNLOAD_STATUS, "backend-snapshot"));

    useEffect(() => {
        let cancelled = false;
        let unlisten: (() => void) | null = null;

        getModelDownloadStatus()
            .then((snapshot) => {
                if (!cancelled) {
                    setStatus((current) => preferNewestModelDownloadStatus(
                        current,
                        observeStatus(snapshot, "backend-snapshot"),
                    ));
                }
            })
            .catch(() => {});

        onDownloadStatus((snapshot) => {
            if (!cancelled) {
                setStatus((current) => preferNewestModelDownloadStatus(
                    current,
                    observeStatus(snapshot, "backend-event"),
                ));
            }
        }).then((dispose) => {
            if (cancelled) {
                dispose();
            } else {
                unlisten = dispose;
            }
        });

        return () => {
            cancelled = true;
            unlisten?.();
        };
    }, []);

    return status;
}
