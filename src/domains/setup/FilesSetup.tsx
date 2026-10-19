import { useCallback, useEffect, useState } from "react";
import {
    FILES_DOWNLOAD_EVENT,
    FILES_STATUS_EVENT,
    filesEditors,
    filesEstimate,
    filesFolders,
    filesForget,
    filesModelDownload,
    filesModelImport,
    filesModelLinks,
    filesOpenPrivacySettings,
    filesPickFolder,
    filesSettingsGet,
    filesSettingsSet,
    filesStatus,
    filesSubfolders,
    filesSuggestedCodeFolders,
    filesSuggestedFolders,
    type CodeEditor,
    type FileKind,
    type FilesDownloadProgress,
    type FilesEstimate,
    type FilesSettings,
    type FilesState,
    type FilesStatus,
    type FolderInfo,
    type FolderReach,
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

const REACH_LABEL: Record<Exclude<FolderReach, "ok">, string> = {
    not_connected: "Not connected",
    missing: "Can’t be found",
    no_access: "No access",
};

/** DigUp's default when someone holds indexing to set hours: overnight. */
const NIGHT: IndexHours = { kind: "between", start: 22 * 60, end: 7 * 60 };

const KIND_WORDS: Record<FileKind, [string, string]> = {
    image: ["picture", "pictures"],
    screenshot: ["screenshot", "screenshots"],
    pdf: ["PDF", "PDFs"],
    doc: ["document", "documents"],
    audio: ["recording", "recordings"],
    video: ["video", "videos"],
    code: ["code file", "code files"],
};

/** "under a minute", "about 6 min", "about 1 h 20 min". */
export function duration(seconds: number): string {
    if (seconds < 60) return "under a minute";
    const minutes = Math.round(seconds / 60);
    if (minutes < 60) return `about ${minutes} min`;
    return `about ${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

/** "120 pictures, 10 PDFs, 2 recordings": the biggest kinds first, three at most. */
export function kindCounts(kinds: Partial<Record<FileKind, number>>): string {
    return (Object.entries(kinds) as [FileKind, number][])
        .filter(([, count]) => count > 0)
        .sort((a, b) => b[1] - a[1])
        .slice(0, 3)
        .map(([kind, count]) => `${count.toLocaleString("en-US")} ${KIND_WORDS[kind][count === 1 ? 0 : 1]}`)
        .join(", ");
}

/** 1320 → "22:00". */
export function timeText(minutes: number): string {
    return `${String(Math.floor(minutes / 60) % 24).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
}

function minutesOf(text: string): number | null {
    const match = /^(\d{1,2}):(\d{2})$/.exec(text);
    if (!match) return null;
    return (Number(match[1]) % 24) * 60 + (Number(match[2]) % 60);
}

function folderName(path: string): string {
    return path.split("/").filter(Boolean).pop() ?? path;
}

function megabytes(bytes: number): string {
    return bytes < 1_000_000 ? "under 1 MB" : `${Math.round(bytes / 1_000_000)} MB`;
}

/** The status line: what the worker does, and how far along it is. */
export function statusText(status: FilesStatus): string {
    const what =
        status.state === "waiting_for_hours" && status.detail
            ? `Indexing ${status.detail}`
            : (status.state === "held" || status.state === "error") && status.detail
              ? status.detail.charAt(0).toUpperCase() + status.detail.slice(1)
              : STATE_LABEL[status.state];
    const { files, done, failed } = status.counts;
    return (
        what +
        (files > 0 ? ` · ${done.toLocaleString("en-US")} of ${files.toLocaleString("en-US")} files read` : "") +
        (failed > 0 ? `, ${failed} could not be read` : "")
    );
}

/**
 * Files search setup (ADR 033), DigUp's onboarding and Settings window:
 * which folders FNDR searches by what is in them, what each costs and how
 * far along it is, what to leave out, code, hours and power. Onboarding
 * offers it once, opt in; Settings keeps every choice. The 865 MB model
 * downloads only after the person says yes.
 */
export function FilesSetup({ mode, onContinue }: { mode: "onboarding" | "settings"; onContinue?: () => void }) {
    const [settings, setSettings] = useState<FilesSettings | null>(null);
    const [status, setStatus] = useState<FilesStatus | null>(null);
    const [suggested, setSuggested] = useState<string[]>([]);
    const [chosen, setChosen] = useState<string[]>([]);
    const [codeSuggested, setCodeSuggested] = useState<string[]>([]);
    const [chosenCode, setChosenCode] = useState<string[]>([]);
    const [onBattery, setOnBattery] = useState(true);
    const [estimate, setEstimate] = useState<FilesEstimate | null>(null);
    const [folders, setFolders] = useState<FolderInfo[]>([]);
    const [open, setOpen] = useState<Record<string, string[]>>({});
    const [removing, setRemoving] = useState<{ folder: string; into: "folders" | "code_folders" } | null>(null);
    const [download, setDownload] = useState<FilesDownloadProgress | null>(null);
    const [editors, setEditors] = useState<CodeEditor[]>([]);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [note, setNote] = useState<string | null>(null);
    const [confirmForget, setConfirmForget] = useState(false);

    useEffect(() => {
        let live = true;
        Promise.all([filesSettingsGet(), filesStatus(), filesSuggestedFolders()])
            .then(([loaded, current, found]) => {
                if (!live) return;
                if (!loaded) throw new Error("no settings");
                setSettings(loaded);
                setStatus(current);
                setSuggested(found);
                setOnBattery(loaded.index_on_battery ?? true);
                setChosen(
                    mode === "onboarding" && !loaded.folders.length
                        ? found.filter((folder) => PRESELECTED.includes(folderName(folder)))
                        : loaded.folders,
                );
                setChosenCode(loaded.code_folders);
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

    // Onboarding offers the folders that hold code (inside a chosen Documents too).
    useEffect(() => {
        if (mode !== "onboarding") return;
        let live = true;
        filesSuggestedCodeFolders(chosen)
            .then((found) => live && setCodeSuggested(found))
            .catch(() => undefined);
        return () => {
            live = false;
        };
    }, [mode, chosen]);

    // Settings shows each folder's progress; it moves with the status.
    const done = status?.counts.done;
    const folderList = settings?.folders.join("\n");
    useEffect(() => {
        if (mode !== "settings" || !settings?.enabled) return;
        let live = true;
        filesFolders()
            .then((found) => live && setFolders(found))
            .catch(() => undefined);
        return () => {
            live = false;
        };
    }, [mode, settings?.enabled, folderList, done]);

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
        if (mode === "onboarding") {
            if (into === "folders") {
                setSuggested((current) => (current.includes(folder) ? current : [...current, folder]));
                setChosen((current) => (current.includes(folder) ? current : [...current, folder]));
            } else if (into === "code_folders") {
                setCodeSuggested((current) => (current.includes(folder) ? current : [...current, folder]));
                setChosenCode((current) => (current.includes(folder) ? current : [...current, folder]));
            }
            return;
        }
        if (settings && !settings[into].includes(folder)) {
            if (into === "folders") setChosen((current) => [...current, folder]);
            await save({ ...settings, [into]: [...settings[into], folder] });
        }
    }, [mode, save, settings]);

    const toggleSubfolders = useCallback((folder: string) => {
        setOpen((current) => {
            if (current[folder]) {
                const { [folder]: _closed, ...rest } = current;
                return rest;
            }
            void filesSubfolders(folder)
                .then((found) => setOpen((now) => (folder in now ? { ...now, [folder]: found } : now)))
                .catch(() => undefined);
            return { ...current, [folder]: [] };
        });
    }, []);

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

    const choiceRows = (list: string[], isOn: (folder: string) => boolean, onToggle: (folder: string, on: boolean) => void, withEstimate: boolean) => (
        <ul className="files-setup-folders">
            {list.map((folder) => {
                const found = withEstimate ? estimate?.folders.find((item) => item.folder === folder) : undefined;
                return (
                    <li key={folder}>
                        <label>
                            <input type="checkbox" checked={isOn(folder)} onChange={(event) => onToggle(folder, event.target.checked)} />
                            <span className="files-setup-folder-name">{folderName(folder)}</span>
                            {found && isOn(folder) && (
                                <small>
                                    {kindCounts(found.files) || "No files to read"} · {duration(found.seconds)}
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
        const offered = [...new Set([...suggested, ...chosen])];
        const codeOffered = [...new Set([...codeSuggested, ...chosenCode])];
        return (
            <div className="files-setup">
                <h2>Search your files</h2>
                <p className="files-setup-lede">
                    FNDR can also find files in the folders you choose by what is in them: a photo by what it shows, the
                    page of a PDF, the minute of a recording where something is said. It reads them on this Mac and never
                    changes, moves or uploads them. Keys, password files and hidden folders are never read.
                </p>
                {choiceRows(
                    offered,
                    (folder) => chosen.includes(folder),
                    (folder, on) => setChosen((current) => (on ? [...current, folder] : current.filter((item) => item !== folder))),
                    true,
                )}
                <button type="button" className="files-setup-link" onClick={() => void addFolder("folders")}>
                    Add a folder…
                </button>

                <h4>Code</h4>
                <p className="files-setup-note">
                    Search code with code: and say what it does, or type a name. Only what git keeps is read.
                </p>
                {codeOffered.length > 0 &&
                    choiceRows(
                        codeOffered,
                        (folder) => chosenCode.includes(folder),
                        (folder, on) =>
                            setChosenCode((current) => (on ? [...current, folder] : current.filter((item) => item !== folder))),
                        false,
                    )}
                <button type="button" className="files-setup-link" onClick={() => void addFolder("code_folders")}>
                    Add a code folder…
                </button>

                <label className="files-setup-switch">
                    <input type="checkbox" checked={onBattery} onChange={(event) => setOnBattery(event.target.checked)} />
                    Index on battery power too
                </label>
                <p className="files-setup-note">
                    {estimate && chosen.length ? `The first pass takes ${duration(estimate.seconds)} on this Mac. ` : ""}
                    It needs EmbeddingGemma 2 (865 MB, Apache 2.0), downloaded once. Indexing waits while FNDR’s own model
                    is busy, memory is short, the Mac is warm or Low Power Mode is on.
                </p>
                {error && <p className="files-setup-error" role="alert">{error}</p>}
                <div className="files-setup-actions">
                    <button
                        type="button"
                        className="files-setup-primary"
                        disabled={busy || !chosen.length}
                        onClick={() => {
                            void save({
                                ...settings,
                                enabled: true,
                                folders: chosen,
                                code_folders: chosenCode,
                                index_on_battery: onBattery,
                            }).then(() => {
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

    const removeRow = (folder: string, into: "folders" | "code_folders", what: string) =>
        removing?.folder === folder && removing.into === into ? (
            <div className="files-setup-confirm" role="group" aria-label={`Remove ${folderName(folder)}`}>
                <span>{what}</span>
                <button
                    type="button"
                    className="files-setup-secondary"
                    onClick={() => {
                        setRemoving(null);
                        void save({ ...settings, [into]: settings[into].filter((item) => item !== folder) });
                    }}
                >
                    Remove
                </button>
                <button type="button" className="files-setup-link" onClick={() => setRemoving(null)}>
                    Keep it
                </button>
            </div>
        ) : null;

    const hours = settings.hours;
    const codeCounts = status?.code_counts;
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
                    {statusText(status)}
                </p>
            )}
            {download && !download.done && (
                <p className="files-setup-status" role="status">
                    Downloading the model: {Math.floor(((download.bytes ?? 0) / Math.max(download.total, 1)) * 100)}%
                </p>
            )}
            {download?.error && <p className="files-setup-error" role="alert">The download failed: {download.error}</p>}
            {status && settings.enabled && !status.model_ready && !download && (
                <div className="files-setup-actions">
                    <button type="button" className="files-setup-secondary" onClick={startDownload}>
                        Download the model (865 MB)
                    </button>
                    <button
                        type="button"
                        className="files-setup-link"
                        onClick={() =>
                            void filesModelImport()
                                .then((taken) => taken.length && setNote(`Using ${taken.join(" and ")}.`))
                                .catch((reason) => setError(String(reason)))
                        }
                    >
                        Use files downloaded elsewhere…
                    </button>
                    <button
                        type="button"
                        className="files-setup-link"
                        onClick={() =>
                            void filesModelLinks()
                                .then((links) => navigator.clipboard.writeText(links.join("\n")))
                                .then(() => setNote("Copied the two download links."))
                                .catch(() => setError("FNDR couldn’t copy the links."))
                        }
                    >
                        Copy the download links
                    </button>
                </div>
            )}
            {note && <p className="files-setup-status">{note}</p>}

            <h4>Folders</h4>
            <ul className="files-setup-folders">
                {settings.folders.map((folder) => {
                    const info = folders.find((item) => item.folder === folder);
                    const subfolders = open[folder];
                    const left = settings.excluded_folders.filter((item) => item.startsWith(`${folder}/`)).length;
                    return (
                        <li key={folder}>
                            <div className="files-setup-folder-row">
                                <span className="files-setup-folder-name" title={folder}>
                                    {folderName(folder)}
                                </span>
                                {info && info.reach !== "ok" ? (
                                    <small className="files-setup-reach" data-reach={info.reach}>
                                        {REACH_LABEL[info.reach]}
                                        {info.reach === "no_access" && (
                                            <button
                                                type="button"
                                                className="files-setup-link"
                                                onClick={() => void filesOpenPrivacySettings().catch(() => undefined)}
                                            >
                                                Allow in Settings…
                                            </button>
                                        )}
                                    </small>
                                ) : info && info.files > 0 ? (
                                    <small>
                                        {info.done.toLocaleString("en-US")} of {info.files.toLocaleString("en-US")} read ·{" "}
                                        {kindCounts(info.kinds)}
                                        {left > 0 ? ` · ${left} left out` : ""}
                                    </small>
                                ) : (
                                    <small>{left > 0 ? `${left} left out` : ""}</small>
                                )}
                                <button
                                    type="button"
                                    className="files-setup-link"
                                    aria-expanded={Boolean(subfolders)}
                                    onClick={() => toggleSubfolders(folder)}
                                >
                                    Subfolders
                                </button>
                                <button
                                    type="button"
                                    className="files-setup-link"
                                    onClick={() => setRemoving({ folder, into: "folders" })}
                                >
                                    Remove…
                                </button>
                            </div>
                            {removeRow(
                                folder,
                                "folders",
                                `Stop searching ${folderName(folder)}? ${info?.files ? `Its ${info.files.toLocaleString("en-US")} files leave the index.` : ""}`,
                            )}
                            {subfolders && (
                                <ul className="files-setup-subfolders" aria-label={`Folders in ${folderName(folder)}`}>
                                    {subfolders.map((inner) => {
                                        const excluded = settings.excluded_folders.includes(inner);
                                        return (
                                            <li key={inner}>
                                                <label>
                                                    <input
                                                        type="checkbox"
                                                        checked={!excluded}
                                                        onChange={(event) =>
                                                            void save({
                                                                ...settings,
                                                                excluded_folders: event.target.checked
                                                                    ? settings.excluded_folders.filter((item) => item !== inner)
                                                                    : [...settings.excluded_folders, inner],
                                                            })
                                                        }
                                                    />
                                                    {folderName(inner)}
                                                </label>
                                            </li>
                                        );
                                    })}
                                    {!subfolders.length && <li className="files-setup-note">No folders inside.</li>}
                                </ul>
                            )}
                        </li>
                    );
                })}
            </ul>
            <button type="button" className="files-setup-link" onClick={() => void addFolder("folders")}>
                Add a folder…
            </button>

            <h4>Leave out</h4>
            {choiceRows(
                settings.excluded_folders,
                () => true,
                (folder, on) => {
                    if (!on) void save({ ...settings, excluded_folders: settings.excluded_folders.filter((item) => item !== folder) });
                },
                false,
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

            <h4>When</h4>
            <div className="files-setup-field">
                <label className="files-setup-field">
                    Big first passes run
                    <select
                        value={hours.kind}
                        onChange={(event) =>
                            void save({ ...settings, hours: event.target.value === "always" ? { kind: "always" } : NIGHT })
                        }
                    >
                        <option value="always">Any time</option>
                        <option value="between">Only between</option>
                    </select>
                </label>
                {hours.kind === "between" && (
                    <>
                        <input
                            type="time"
                            aria-label="From"
                            value={timeText(hours.start)}
                            onChange={(event) => {
                                const start = minutesOf(event.target.value);
                                if (start !== null) void save({ ...settings, hours: { ...hours, start } });
                            }}
                        />
                        <span>and</span>
                        <input
                            type="time"
                            aria-label="Until"
                            value={timeText(hours.end)}
                            onChange={(event) => {
                                const end = minutesOf(event.target.value);
                                if (end !== null) void save({ ...settings, hours: { ...hours, end } });
                            }}
                        />
                    </>
                )}
            </div>
            <label className="files-setup-switch">
                <input
                    type="checkbox"
                    checked={settings.index_on_battery ?? true}
                    onChange={(event) => void save({ ...settings, index_on_battery: event.target.checked })}
                />
                Index on battery power too
            </label>
            <p className="files-setup-note">
                New files are always read within seconds. Big passes also wait while the Mac is warm or Low Power Mode is on.
            </p>
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
            {settings.enabled && codeCounts && settings.code_folders.length > 0 && (
                <p className="files-setup-status">
                    {codeCounts.pending > 0
                        ? `Reading code · ${codeCounts.done.toLocaleString("en-US")} of ${codeCounts.files.toLocaleString("en-US")} files`
                        : `${codeCounts.done.toLocaleString("en-US")} code files searchable`}
                </p>
            )}
            <ul className="files-setup-folders">
                {settings.code_folders.map((folder) => (
                    <li key={folder}>
                        <div className="files-setup-folder-row">
                            <span className="files-setup-folder-name" title={folder}>
                                {folderName(folder)}
                            </span>
                            <small />
                            <button
                                type="button"
                                className="files-setup-link"
                                onClick={() => setRemoving({ folder, into: "code_folders" })}
                            >
                                Remove…
                            </button>
                        </div>
                        {removeRow(
                            folder,
                            "code_folders",
                            settings.code_folders.length === 1
                                ? `Turn off code search? Its index (${megabytes(status?.code_bytes ?? 0)}) is cleared.`
                                : `Stop searching the code in ${folderName(folder)}?`,
                        )}
                    </li>
                ))}
            </ul>
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
