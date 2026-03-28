import { useEffect, useState } from "react";
import {
    ModelDownloadStatus,
    getModelDownloadStatus,
    onDownloadStatus,
} from "../api/onboarding";

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

export function useModelDownloadStatus(): ModelDownloadStatus {
    const [status, setStatus] = useState<ModelDownloadStatus>(EMPTY_DOWNLOAD_STATUS);

    useEffect(() => {
        let cancelled = false;
        let unlisten: (() => void) | null = null;

        getModelDownloadStatus()
            .then((snapshot) => {
                if (!cancelled) {
                    setStatus(snapshot);
                }
            })
            .catch(() => {});

        onDownloadStatus((snapshot) => {
            setStatus(snapshot);
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
