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

export interface Task {
    id: string;
    title: string;
    description: string;
    source_app: string;
    source_memory_id: string | null;
    created_at: number;
    due_date: number | null;
    is_completed: boolean;
    is_dismissed: boolean;
    task_type: "Todo" | "Reminder" | "Followup";
}

// Search functions
export async function search(
    query: string,
    timeFilter?: string,
    appFilter?: string,
    limit?: number
): Promise<SearchResult[]> {
    return invoke<SearchResult[]>("search", {
        query,
        timeFilter,
        appFilter,
        limit,
    });
}

export async function askFndr(query: string): Promise<string> {
    return invoke<string>("ask_fndr", { query });
}

export async function summarizeMemory(
    appName: string,
    windowTitle: string,
    text: string
): Promise<string> {
    return invoke<string>("summarize_memory", { appName, windowTitle, text });
}

// Capture control
export async function getStatus(): Promise<CaptureStatus> {
    return invoke<CaptureStatus>("get_status");
}

export async function pauseCapture(): Promise<void> {
    return invoke("pause_capture");
}

export async function resumeCapture(): Promise<void> {
    return invoke("resume_capture");
}

// Privacy
export async function getBlocklist(): Promise<string[]> {
    return invoke<string[]>("get_blocklist");
}

export async function setBlocklist(apps: string[]): Promise<void> {
    return invoke("set_blocklist", { apps });
}

export async function deleteAllData(): Promise<void> {
    return invoke("delete_all_data");
}

// Stats
export async function getStats(): Promise<Stats> {
    return invoke<Stats>("get_stats");
}

export async function getRetentionDays(): Promise<number> {
    return invoke<number>("get_retention_days");
}

export async function setRetentionDays(days: number): Promise<void> {
    return invoke("set_retention_days", { days });
}

export async function deleteOlderThan(days: number): Promise<number> {
    return invoke<number>("delete_older_than", { days });
}

export async function getAppNames(): Promise<string[]> {
    return invoke<string[]>("get_app_names");
}

// Task functions
export async function getTodos(): Promise<Task[]> {
    return invoke<Task[]>("get_todos");
}

export async function dismissTodo(taskId: string): Promise<boolean> {
    return invoke<boolean>("dismiss_todo", { taskId });
}

export async function executeTodo(taskId: string): Promise<Task> {
    return invoke<Task>("execute_todo", { taskId });
}
