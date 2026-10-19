import { useCallback, useEffect, useState } from "react";
import {
    FILES_DOWNLOAD_EVENT,
    FILES_STATUS_EVENT,
    filesEditors,
    filesEstimate,
    filesForget,
    filesModelDownload,
    filesPickFolder,
    filesSettingsGet,
    filesSettingsSet,
    filesStatus,
    filesSuggestedFolders,
    type CodeEditor,
    type FilesDownloadProgress,
    type FilesEstimate,
    type FilesSettings,
    type FilesState,
    type FilesStatus,
    type IndexHours,
} from "@/shared/ipc/files";
import { useTauriEvent } from "@/shared/hooks/useTauriEvent";
import "./FilesSetup.css";

/** Preselected in onboarding, when they exist. */
const PRESELECTED = ["Documents", "Desktop", "Downloads"];

const STATE_LABEL: Record<FilesState, string> = {
    off: "Off",
    needs_model: "Waiting for the model",
    indexing: "Reading files",
    paused: "Paused",
    waiting_for_hours: "Waiting for the indexing hours",
    held: "Waiting",
    idle: "Up to date",
    error: "Stopped",
};

const HOURS: { label: string; hours: IndexHours }[] = [
    { label: "Any time", hours: { kind: "always" } },
    { label: "Overnight, 10 pm to 7 am", hours: { kind: "between", start: 22, end: 7 } },
    { label: "Outside working hours, 6 pm to 9 am", hours: { kind: "between", start: 18, end: 9 } },
];

/** "under a minute", "about 6 min", "about 1 h 20 min". */
export function duration(seconds: number): string {
    if (seconds < 60) return "under a minute";
    const minutes = Math.round(seconds / 60);
    if (minutes < 60) return `about ${minutes} min`;
    return `about ${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

function folderName(path: string): string {
    return path.split("/").filter(Boolean).pop() ?? path;
}

function countFiles(estimate: FilesEstimate["folders"][number] | undefined): number {
    return Object.values(estimate?.files ?? {}).reduce((sum, count) => sum + (count ?? 0), 0);
}

function sameHours(a: IndexHours, b: IndexHours): boolean {
    return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * Files search setup (ADR 033): which folders FNDR searches by what is in
 * them. Onboarding offers it once, opt in; Settings keeps every choice.
 * The 865 MB model downloads only after the person says yes.
 */
export function FilesSetup({ mode, onContinue }: { mode: "onboarding" | "settings"; onContinue?: () => void }) {
    const [settings, setSettings] = useState<FilesSettings | null>(null);
    const [status, setStatus] = useState<FilesStatus | null>(null);
    const [suggested, setSuggested] = useState<string[]>([]);
    const [chosen, setChosen] = useState<string[]>([]);
    const [estimate, setEstimate] = useState<FilesEstimate | null>(null);
    const [download, setDownload] = useState<FilesDownloadProgress | null>(null);
    const [editors, setEditors] = useState<CodeEditor[]>([]);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [confirmForget, setConfirmForget] = useState(false);

    useEffect(() => {
        let live = true;
        Promise.all([filesSettingsGet(), filesStatus(), filesSuggestedFolders()])
            .then(([loaded, current, folders]) => {
                if (!live) return;
                if (!loaded) throw new Error("no settings");
                setSettings(loaded);
                setStatus(current);
                setSuggested(folders);
                setChosen(
                    mode === "onboarding" && !loaded.folders.length
                        ? folders.filter((folder) => PRESELECTED.includes(folderName(folder)))
                        : loaded.folders,
                );
            })
            .catch(() => live && setError("Files search is not available right now."));
        if (mode === "settings") {
            filesEditors()
                .then((found) => live && setEditors(found))
                .catch(() => undefined);
        }
        return () => {
            live = false;
        };
    }, [mode]);

    // What the first pass involves, from metadata only.
    useEffect(() => {
        let live = true;
        if (!chosen.length) {
            setEstimate(null);
            return;
        }
        filesEstimate(chosen)
            .then((found) => live && setEstimate(found))
            .catch(() => live && setEstimate(null));
        return () => {
            live = false;
        };
    }, [chosen]);

    useTauriEvent<FilesStatus>(FILES_STATUS_EVENT, setStatus);
    useTauriEvent<FilesDownloadProgress>(FILES_DOWNLOAD_EVENT, (progress) => {
        setDownload(progress.done && !progress.error ? null : progress);
        if (progress.done) {
            void filesStatus().then(setStatus).catch(() => undefined);
        }
    });

    const save = useCallback(async (next: FilesSettings) => {
        setBusy(true);
        setError(null);
        try {
            setStatus(await filesSettingsSet(next));
            setSettings(next);
        } catch {
            setError("FNDR couldn't save that. Try again.");
        } finally {
            setBusy(false);
        }
    }, []);

    const startDownload = useCallback(() => {
        setDownload({ bytes: 0, total: 1, done: false });
        filesModelDownload().catch(() => setError("The model could not start downloading. Try again."));
    }, []);

    const addFolder = useCallback(async (into: "folders" | "code_folders" | "excluded_folders") => {
        const folder = await filesPickFolder().catch(() => null);
        if (!folder) return;
        if (into === "folders" && mode === "onboarding") {
            setSuggested((current) => (current.includes(folder) ? current : [...current, folder]));
            setChosen((current) => (current.includes(folder) ? current : [...current, folder]));
            return;
        }
        if (settings && !settings[into].includes(folder)) {
            if (into === "folders") setChosen((current) => [...current, folder]);
            await save({ ...settings, [into]: [...settings[into], folder] });
        }
    }, [mode, save, settings]);

    if (!settings) {
        if (mode === "onboarding" && error) {
            // Setup never gets stuck on an optional step.
            return (
                <div className="files-setup">
                    <h2>Search your files</h2>
                    <p className="files-setup-error" role="alert">{error} You can turn it on later in Settings.</p>
                    <div className="files-setup-actions">
                        <button type="button" className="files-setup-primary" onClick={() => onContinue?.()}>
                            Continue
                        </button>
                    </div>
                </div>
            );
        }
        return error ? <p className="files-setup-error" role="alert">{error}</p> : null;
    }

    const folderRows = (folders: string[], onToggle: (folder: string, on: boolean) => void, isOn: (folder: string) => boolean) => (
        <ul className="files-setup-folders">
            {folders.map((folder) => {
                const found = estimate?.folders.find((item) => item.folder === folder);
                const files = countFiles(found);
                return (
                    <li key={folder}>
                        <label>
                            <input type="checkbox" checked={isOn(folder)} onChange={(event) => onToggle(folder, event.target.checked)} />
                            <span className="files-setup-folder-name">{folderName(folder)}</span>
                            {found && isOn(folder) && (
                                <small>
                                    {files.toLocaleString()} {files === 1 ? "file" : "files"} · {duration(found.seconds)}
                                    {found.later_seconds > 60 ? `, then ${duration(found.later_seconds)} for long files` : ""}
                                </small>
                            )}
                        </label>
                    </li>
                );
            })}
        </ul>
    );

    if (mode === "onboarding") {
        const folders = [...new Set([...suggested, ...chosen])];
        return (
            <div className="files-setup">
                <h2>Search your files</h2>
                <p className="files-setup-lede">
                    FNDR can also find files in the folders you choose by what is in them: a photo by what it shows, the
                    page of a PDF, the minute of a recording where something is said. It reads them on this Mac and never
                    changes, moves or uploads them. Keys, password files and hidden folders are never read.
                </p>
                {folderRows(
                    folders,
                    (folder, on) => setChosen((current) => (on ? [...current, folder] : current.filter((item) => item !== folder))),
                    (folder) => chosen.includes(folder),
                )}
                <button type="button" className="files-setup-link" onClick={() => void addFolder("folders")}>
                    Add a folder…
                </button>
                <p className="files-setup-note">
                    {estimate && chosen.length ? `The first pass takes ${duration(estimate.seconds)} on this Mac. ` : ""}
                    It needs EmbeddingGemma 2 (865 MB, Apache 2.0), downloaded once. Indexing pauses while FNDR's own model
                    is busy or memory is short.
                </p>
                {error && <p className="files-setup-error" role="alert">{error}</p>}
                <div className="files-setup-actions">
                    <button
                        type="button"
                        className="files-setup-primary"
                        disabled={busy || !chosen.length}
                        onClick={() => {
                            void save({ ...settings, enabled: true, folders: chosen }).then(() => {
                                if (status && !status.model_ready) startDownload();
                                onContinue?.();
                            });
                        }}
                    >
                        Search these folders
                    </button>
                    <button type="button" className="files-setup-secondary" onClick={() => onContinue?.()}>
                        Not now
                    </button>
                </div>
            </div>
        );
    }

    const counts = status?.counts;
    return (
        <section className="files-setup" aria-labelledby="files-setup-title">
            <h3 id="files-setup-title">Files search</h3>
            <p className="files-setup-lede">
                Find files in chosen folders by what is in them, from Quick Find, the notch and Search and Ask. Start with
                file: for files only, or code: for code.
            </p>
            <label className="files-setup-switch">
                <input
                    type="checkbox"
                    checked={settings.enabled}
                    disabled={busy}
                    onChange={(event) => void save({ ...settings, enabled: event.target.checked })}
                />
                Search files in the folders below
            </label>

            {status && settings.enabled && (
                <p className="files-setup-status" role="status">
                    {status.state === "held" || status.state === "error" ? status.detail ?? STATE_LABEL[status.state] : STATE_LABEL[status.state]}
                    {counts && counts.files > 0 ? ` · ${counts.done.toLocaleString()} of ${counts.files.toLocaleString()} files read` : ""}
                    {counts && counts.failed > 0 ? `, ${counts.failed} could not be read` : ""}
                </p>
            )}
            {download && !download.done && (
                <p className="files-setup-status" role="status">
                    Downloading the model: {Math.floor(((download.bytes ?? 0) / Math.max(download.total, 1)) * 100)}%
                </p>
            )}
            {download?.error && <p className="files-setup-error" role="alert">The download failed: {download.error}</p>}
            {status && settings.enabled && !status.model_ready && !download && (
                <button type="button" className="files-setup-secondary" onClick={startDownload}>
                    Download the model (865 MB)
                </button>
            )}

            <h4>Folders</h4>
            {folderRows(
                settings.folders,
                (folder, on) => {
                    if (!on) void save({ ...settings, folders: settings.folders.filter((item) => item !== folder) });
                },
                () => true,
            )}
            <button type="button" className="files-setup-link" onClick={() => void addFolder("folders")}>
                Add a folder…
            </button>

            <h4>Leave out</h4>
            {folderRows(
                settings.excluded_folders,
                (folder, on) => {
                    if (!on) void save({ ...settings, excluded_folders: settings.excluded_folders.filter((item) => item !== folder) });
                },
                () => true,
            )}
            <button type="button" className="files-setup-link" onClick={() => void addFolder("excluded_folders")}>
                Leave out a folder…
            </button>
            <label className="files-setup-field">
                File types to leave out
                <input
                    type="text"
                    placeholder="heic, mov"
                    defaultValue={settings.excluded_types}
                    onBlur={(event) => {
                        if (event.target.value !== settings.excluded_types) {
                            void save({ ...settings, excluded_types: event.target.value });
                        }
                    }}
                />
            </label>
            <label className="files-setup-switch">
                <input
                    type="checkbox"
                    checked={settings.include_datasets}
                    onChange={(event) => void save({ ...settings, include_datasets: event.target.checked })}
                />
                Read folders that look like datasets (hundreds of machine-named pictures or clips)
            </label>

            <label className="files-setup-field">
                Big first passes run
                <select
                    value={Math.max(0, HOURS.findIndex((item) => sameHours(item.hours, settings.hours)))}
                    onChange={(event) => void save({ ...settings, hours: HOURS[Number(event.target.value)].hours })}
                >
                    {HOURS.map((item, index) => (
                        <option key={item.label} value={index}>
                            {item.label}
                        </option>
                    ))}
                </select>
            </label>
            <button
                type="button"
                className="files-setup-secondary"
                onClick={() => void save({ ...settings, paused: !settings.paused })}
            >
                {settings.paused ? "Resume indexing" : "Pause indexing"}
            </button>

            <h4>Code</h4>
            <p className="files-setup-note">
                Search code with code: and say what it does, or type a name. Only what git keeps is read, never vendored or
                built code, data, or anything that looks like a key.
            </p>
            {folderRows(
                settings.code_folders,
                (folder, on) => {
                    if (!on) void save({ ...settings, code_folders: settings.code_folders.filter((item) => item !== folder) });
                },
                () => true,
            )}
            <button type="button" className="files-setup-link" onClick={() => void addFolder("code_folders")}>
                Add a code folder…
            </button>
            <label className="files-setup-field">
                Open code in
                <select
                    value={settings.code_editor ?? "auto"}
                    onChange={(event) =>
                        void save({ ...settings, code_editor: event.target.value === "auto" ? null : event.target.value })
                    }
                >
                    <option value="auto">The editor in use</option>
                    {editors.map((editor) => (
                        <option key={editor.id} value={editor.id}>
                            {editor.name}
                        </option>
                    ))}
                    <option value="">Each file's default app</option>
                </select>
            </label>

            <h4>Forget</h4>
            {!confirmForget ? (
                <button type="button" className="files-setup-secondary" onClick={() => setConfirmForget(true)}>
                    Turn off and forget the index…
                </button>
            ) : (
                <div className="files-setup-actions" role="group" aria-label="Forget the Files index">
                    <button
                        type="button"
                        className="files-setup-secondary"
                        onClick={() => {
                            setConfirmForget(false);
                            void filesForget(false).then(setStatus).then(() => filesSettingsGet().then(setSettings));
                        }}
                    >
                        Forget the index
                    </button>
                    <button
                        type="button"
                        className="files-setup-secondary"
                        onClick={() => {
                            setConfirmForget(false);
                            void filesForget(true).then(setStatus).then(() => filesSettingsGet().then(setSettings));
                        }}
                    >
                        Forget the index and delete the model
                    </button>
                    <button type="button" className="files-setup-link" onClick={() => setConfirmForget(false)}>
                        Keep it
                    </button>
                </div>
            )}
            {error && <p className="files-setup-error" role="alert">{error}</p>}
        </section>
    );
}
