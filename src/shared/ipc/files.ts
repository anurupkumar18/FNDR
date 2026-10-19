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
    counts: FilesCounts;
    model_ready: boolean;
    helper_found: boolean;
    reader_found: boolean;
}

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

export function filesStatus(): Promise<FilesStatus> {
    return invoke("files_status");
}

export function filesSettingsGet(): Promise<FilesSettings> {
    return invoke("files_settings_get");
}

export function filesSettingsSet(settings: FilesSettings): Promise<FilesStatus> {
    return invoke("files_settings_set", { settings });
}

export function filesSearch(query: string, kinds?: FileKind[], limit?: number): Promise<FilesSearchResult> {
    return invoke("files_search", { query, kinds, limit });
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

export function filesEstimate(folders: string[]): Promise<FilesEstimate> {
    return invoke("files_estimate", { folders });
}
