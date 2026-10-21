import { invoke } from "@tauri-apps/api/core";

export type ImportKind = "browser" | "file" | "calendar" | "mail" | "slack" | "git";
export type ImportPermission = "none" | "files" | "full_disk_access" | "calendar";
export type ImportRunState = "idle" | "running" | "done" | "cancelled" | "failed";

export interface ImportStatus {
    source: string;
    state: ImportRunState;
    window_days: number;
    scanned: number;
    stored: number;
    updated: number;
    duplicates: number;
    redacted: number;
    skipped: number;
    skip_reasons: Record<string, number>;
    started_at_ms: number;
    finished_at_ms: number | null;
    error: string | null;
}

export interface ImportSourceInfo {
    id: string;
    kind: ImportKind;
    label: string;
    permissions: ImportPermission[];
    row_count: number;
    status: ImportStatus;
}

export function importListSources(): Promise<ImportSourceInfo[]> {
    return invoke("import_list_sources");
}

export function importGetConsent(source: string): Promise<boolean> {
    return invoke("import_get_consent", { source });
}

export function importSetConsent(source: string, enabled: boolean): Promise<void> {
    return invoke("import_set_consent", { source, enabled });
}

export function importStart(source: string, windowDays = 30): Promise<ImportStatus> {
    return invoke("import_start", { source, windowDays });
}

export function importStatus(source: string): Promise<ImportStatus> {
    return invoke("import_status", { source });
}

export function importCancel(source: string): Promise<boolean> {
    return invoke("import_cancel", { source });
}

export function importForget(source: string): Promise<number> {
    return invoke("import_forget", { source });
}

export function importRecordFirstAnswer(elapsedMs: number): Promise<void> {
    return invoke("import_record_first_answer", { elapsedMs });
}

export function importFirstAnswerMs(): Promise<number | null> {
    return invoke("import_first_answer_ms");
}
