import { invoke } from "@tauri-apps/api/core";

/** Files search (ADR 033): the files in folders the person chose, found by what is in them. */

export type FileKind = "screenshot" | "image" | "pdf" | "doc" | "audio" | "video" | "code";
export type SegmentKind =
    | "name" | "image" | "ocr" | "page" | "page_image" | "chunk" | "audio" | "frame" | "lines" | "cells";

export interface FileHit {
    path: string;
    kind: FileKind;
    score: number;
    z: number | null;
    cosine: number | null;
    keyword_rank: number | null;
    segment: SegmentKind;
    segment_id: number;
    /** PDF page, or seconds into a recording or video. */
    loc: number | null;
    loc_end: number | null;
    excerpt: string | null;
    /** Seconds since 1970. */
    modified: number;
    size: number;
    info: Record<string, unknown> | null;
    /** Code: the line to open at (where a name is defined, or the query's words are). */
    focus: number | null;
}

export interface FilesSearchResult {
    hits: FileHit[];
    /** The first `strong` hits are strong; the rest are weaker matches. */
    strong: number;
    /** False when the search used words only. */
    meaning: boolean;
}

export type FilesState =
    | "off" | "needs_model" | "indexing" | "paused" | "waiting_for_hours" | "held" | "idle" | "error";

export interface FilesCounts {
    files: number;
    done: number;
    pending: number;
    failed: number;
    vectors: number;
}

export interface FilesStatus {
    state: FilesState;
    detail: string | null;
    /** Files and code together. */
    counts: FilesCounts;
    /** Code alone. */
    code_counts: FilesCounts;
    /** The code index on disk. */
    code_bytes: number;
    model_ready: boolean;
    helper_found: boolean;
    reader_found: boolean;
}

/** `start` and `end` in minutes after midnight; a range may wrap midnight. */
export type IndexHours = { kind: "always" } | { kind: "between"; start: number; end: number };

export interface FilesSettings {
    enabled: boolean;
    folders: string[];
    excluded_folders: string[];
    excluded_types: string;
    code_folders: string[];
    /** An editor's bundle id; "" each file's default app; null the first installed editor. */
    code_editor: string | null;
    include_datasets: boolean;
    paused: boolean;
    hours: IndexHours;
    /** Big catch-ups run on battery too; off, they wait for power. */
    index_on_battery: boolean;
}

export type FolderReach = "ok" | "not_connected" | "missing" | "no_access";

export interface FolderInfo {
    folder: string;
    reach: FolderReach;
    files: number;
    done: number;
    kinds: Partial<Record<FileKind, number>>;
}

export interface FolderEstimate {
    folder: string;
    files: Partial<Record<FileKind, number>>;
    seconds: number;
    later_seconds: number;
    skipped: Record<string, number>;
}

export interface FilesEstimate {
    folders: FolderEstimate[];
    seconds: number;
    later_seconds: number;
}

export interface FilesDownloadProgress {
    bytes?: number;
    total: number;
    done: boolean;
    error?: string | null;
}

export const FILES_STATUS_EVENT = "files://status";
export const FILES_DOWNLOAD_EVENT = "files://download";
/** Quick Find handing a search to Search and Ask. */
export const FILES_OPEN_SEARCH_EVENT = "files://open-search";

export function filesStatus(): Promise<FilesStatus> {
    return invoke("files_status");
}

export function filesSettingsGet(): Promise<FilesSettings> {
    return invoke("files_settings_get");
}

export function filesSettingsSet(settings: FilesSettings): Promise<FilesStatus> {
    return invoke("files_settings_set", { settings });
}

/** `wordsOnly` answers at once while typing; the full search follows on a pause. */
export function filesSearch(
    query: string,
    kinds?: FileKind[],
    limit?: number,
    wordsOnly?: boolean,
): Promise<FilesSearchResult> {
    return invoke("files_search", { query, kinds, limit, wordsOnly });
}

/** Quick Look's thumbnails as data URLs by path, `pixels` wide at most; a file it can't draw is left out. */
export function filesThumbnails(paths: string[], pixels?: number): Promise<Record<string, string>> {
    return invoke("files_thumbnails", { paths, pixels });
}

/** Quick Look for a found file, or closes the one showing (no path). True when it shows. */
export function filesQuickLook(path?: string): Promise<boolean> {
    return invoke("files_quick_look", { path });
}

/** The same search in Search and Ask's Files section, for a bigger look. */
export function filesOpenInWindow(query: string, kinds?: FileKind[]): Promise<void> {
    return invoke("files_open_in_window", { query, kinds });
}

export interface FilesOpenSearch {
    query: string;
    kinds: FileKind[] | null;
}

export function filesModelDownload(): Promise<void> {
    return invoke("files_model_download");
}

export function filesOpen(path: string, page?: number, seconds?: number, words?: string[]): Promise<string> {
    return invoke("files_open", { path, page, seconds, words });
}

export interface CodeEditor {
    id: string;
    name: string;
}

export function filesOpenCode(path: string, line: number): Promise<string> {
    return invoke("files_open_code", { path, line });
}

export function filesEditors(): Promise<CodeEditor[]> {
    return invoke("files_editors");
}

export function filesReveal(path: string): Promise<void> {
    return invoke("files_reveal", { path });
}

export function filesCopy(path: string): Promise<void> {
    return invoke("files_copy", { path });
}

/** A data URL: the picture, or the matched page or frame. */
export function filesPreview(path: string, kind: FileKind, page?: number, seconds?: number): Promise<string> {
    return invoke("files_preview", { path, kind, page, seconds });
}

export function filesForget(model: boolean): Promise<FilesStatus> {
    return invoke("files_forget", { model });
}

/** Documents, Desktop, Downloads, Pictures, Movies and Music, where they exist. */
export function filesSuggestedFolders(): Promise<string[]> {
    return invoke("files_suggested_folders");
}

export function filesPickFolder(): Promise<string | null> {
    return invoke("files_pick_folder");
}

export function filesEstimate(folders: string[]): Promise<FilesEstimate> {
    return invoke("files_estimate", { folders });
}

/** The chosen folders: whether each can be read, its files by kind, how many are read. */
export function filesFolders(): Promise<FolderInfo[]> {
    return invoke("files_folders");
}

/** The folders one level inside a chosen folder. */
export function filesSubfolders(folder: string): Promise<string[]> {
    return invoke("files_subfolders", { folder });
}

/** Folders holding code, to offer for code search. */
export function filesSuggestedCodeFolders(searched: string[]): Promise<string[]> {
    return invoke("files_suggested_code_folders", { searched });
}

/** The model's two download links, for a network that blocks the download. */
export function filesModelLinks(): Promise<string[]> {
    return invoke("files_model_links");
}

/** The person picks model files downloaded elsewhere; returns the names taken. */
export function filesModelImport(): Promise<string[]> {
    return invoke("files_model_import");
}

/** System Settings at Full Disk Access. */
export function filesOpenPrivacySettings(): Promise<void> {
    return invoke("files_open_privacy_settings");
}

/** The text a hit matched in: a page, a stretch of a document, the lines of code. */
export function filesPassage(path: string, segmentId: number): Promise<string | null> {
    return invoke("files_passage", { path, segmentId });
}

/** A stretch of a recording or video from `seconds`, as a data URL to play. */
export function filesClip(path: string, kind: FileKind, seconds: number): Promise<string> {
    return invoke("files_clip", { path, kind, seconds });
}

/** Drags a found file out to Finder, Mail or a chat; `icon` is its thumbnail as a PNG data URL. */
export function filesDrag(path: string, icon?: string): Promise<void> {
    return invoke("files_drag", { path, icon });
}
