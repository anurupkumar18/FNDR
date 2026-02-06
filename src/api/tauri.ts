import { invoke } from "@tauri-apps/api/core";

export interface SearchResult {
    id: string;
    timestamp: number;
    app_name: string;
    window_title: string;
    text: string;
    snippet: string;
    score: number;
}

export interface CaptureStatus {
    is_capturing: boolean;
    is_paused: boolean;
    is_incognito: boolean;
    frames_captured: number;
    frames_dropped: number;
    last_capture_time: number;
}

export interface Stats {
    total_records: number;
    total_days: number;
    apps: { name: string; count: number }[];
    today_count: number;
}

export async function search(
    query: string,
    timeFilter: string | null,
    appFilter: string | null,
    limit?: number
): Promise<SearchResult[]> {
    return invoke("search", {
        query,
        timeFilter,
        appFilter,
        limit: limit ?? 20,
    });
}

export async function getStatus(): Promise<CaptureStatus> {
    return invoke("get_status");
}

export async function pauseCapture(): Promise<void> {
    return invoke("pause_capture");
}

export async function resumeCapture(): Promise<void> {
    return invoke("resume_capture");
}

export async function getBlocklist(): Promise<string[]> {
    return invoke("get_blocklist");
}

export async function setBlocklist(apps: string[]): Promise<void> {
    return invoke("set_blocklist", { apps });
}

export async function deleteAllData(): Promise<void> {
    return invoke("delete_all_data");
}

export async function getStats(): Promise<Stats> {
    return invoke("get_stats");
}

export async function getAppNames(): Promise<string[]> {
    return invoke("get_app_names");
}

export async function getRetentionDays(): Promise<number> {
    return invoke("get_retention_days");
}

export async function setRetentionDays(days: number): Promise<void> {
    return invoke("set_retention_days", { days });
}

export async function deleteOlderThan(days: number): Promise<number> {
    return invoke("delete_older_than", { days });
}

export async function askFndr(query: string): Promise<string> {
    return invoke("ask_fndr", { query });
}

export async function summarizeMemory(
    appName: string,
    windowTitle: string,
    text: string
): Promise<string> {
    return invoke("summarize_memory", { appName, windowTitle, text });
}
