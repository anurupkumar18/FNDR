import { useEffect, useState } from "react";
import { FILES_STATUS_EVENT, filesSettingsGet, filesStatus, type FilesSettings, type FilesStatus } from "@/shared/ipc/files";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";

/** Files search's status and settings, kept current from the service's events. */
export function useFilesStatus(enabled = true) {
    const [status, setStatus] = useState<FilesStatus | null>(null);
    const [settings, setSettings] = useState<FilesSettings | null>(null);
    useEffect(() => {
        if (!enabled) return;
        let live = true;
        filesStatus()
            .then((next) => live && setStatus(next))
            .catch(() => undefined);
        filesSettingsGet()
            .then((next) => live && setSettings(next))
            .catch(() => undefined);
        return () => {
            live = false;
        };
    }, [enabled]);
    useTauriEvent<FilesStatus>(FILES_STATUS_EVENT, (next) => {
        setStatus(next);
        // Settings change with the status (folders added, Files turned on).
        void filesSettingsGet()
            .then(setSettings)
            .catch(() => undefined);
    });
    return { status, settings };
}
