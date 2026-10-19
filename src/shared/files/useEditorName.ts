import { useEffect, useState } from "react";
import { filesEditors, type FilesSettings } from "@/shared/ipc/files";

/**
 * The editor code opens in, by name, for "Open in Cursor": the chosen one,
 * else the first installed (as the reader picks). Null for each file's
 * default app, or until it is known. Asked once, when code is first shown.
 */
export function useEditorName(active: boolean, settings: FilesSettings | null): string | null {
    const [editors, setEditors] = useState<{ id: string; name: string }[] | null>(null);
    useEffect(() => {
        if (!active || editors) return;
        let live = true;
        filesEditors()
            .then((found) => live && setEditors(found))
            .catch(() => live && setEditors([]));
        return () => {
            live = false;
        };
    }, [active, editors]);
    if (!editors || settings?.code_editor === "") return null;
    const chosen = settings?.code_editor;
    return (chosen ? editors.find((editor) => editor.id === chosen) : editors[0])?.name ?? null;
}
