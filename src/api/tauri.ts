import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface SearchResult {
    id: string;
    timestamp: number;
    app_name: string;
    bundle_id?: string;
    window_title: string;
    session_id: string;
    text: string;
    snippet: string;
    score: number;
    screenshot_path?: string;
    url?: string;
}

export interface MemoryCard {
    id: string;
    title: string;
    summary: string;
    action: string;
    context: string[];
    timestamp: number;
    app_name: string;
    window_title: string;
    url?: string;
    score: number;
    source_count: number;
    continuity?: boolean;
    raw_snippets: string[];
    evidence_ids?: string[];
    confidence?: number;
}

export interface CaptureStatus {
    is_capturing: boolean;
    is_paused: boolean;
    is_incognito: boolean;
    frames_captured: number;
    frames_dropped: number;
    last_capture_time: number;
    ai_model_available: boolean;
    ai_model_loaded: boolean;
    loaded_model_id: string | null;
    embedding_backend: string;
    embedding_degraded: boolean;
    embedding_detail: string;
}

export interface McpServerStatus {
    running: boolean;
    host: string;
    port: number;
    endpoint: string;
    last_error?: string | null;
}

export interface AppMergeCount {
    app_name: string;
    merged: number;
}

export interface MemoryRepairSummary {
    total_before: number;
    total_after: number;
    merged_count: number;
    anchor_merges: number;
    task_reference_updates: number;
    screenshots_cleaned: number;
    spotify_merges: number;
    youtube_merges: number;
    codex_merges: number;
    discord_merges: number;
    gitlab_merges: number;
    antigravity_merges: number;
    app_merges: AppMergeCount[];
}

export interface MemoryRepairProgress {
    is_running: boolean;
    phase: string;
    processed: number;
    total: number;
    merged_count: number;
    anchor_merges: number;
    timestamp_ms: number;
}

export interface MeetingBreakdown {
    todos: string[];
    reminders: string[];
    followups: string[];
    summary: string;
}

export interface MeetingSession {
    id: string;
    title: string;
    participants: string[];
    model: string;
    status: "recording" | "stopped" | "error" | "analyzing";
    start_timestamp: number;
    end_timestamp?: number | null;
    created_at: number;
    updated_at: number;
    segment_count: number;
    duration_seconds: number;
    meeting_dir: string;
    audio_dir: string;
    transcript_path?: string | null;
    breakdown?: MeetingBreakdown | null;
}

export interface MeetingSegment {
    id: string;
    meeting_id: string;
    index: number;
    start_timestamp: number;
    end_timestamp: number;
    text: string;
    audio_chunk_path: string;
    model: string;
    created_at: number;
}

export interface MeetingRecorderStatus {
    is_recording: boolean;
    current_meeting_id?: string | null;
    current_title?: string | null;
    model?: string | null;
    started_at?: number | null;
    ffmpeg_available: boolean;
    transcription_backend: string;
    is_analyzing: boolean;
    last_error?: string | null;
}

export interface MeetingTranscript {
    meeting: MeetingSession;
    segments: MeetingSegment[];
    full_text: string;
}


export interface Stats {
    total_records: number;
    total_days: number;
    apps: { name: string; count: number }[];
    today_count: number;
    unique_apps: number;
    unique_sessions: number;
    unique_window_titles: number;
    unique_urls: number;
    unique_domains: number;
    records_with_url: number;
    records_with_screenshot: number;
    records_with_clean_text: number;
    records_last_hour: number;
    records_last_24h: number;
    records_last_7d: number;
    avg_records_per_active_day: number;
    avg_records_per_hour: number;
    focus_app_share_pct: number;
    app_switches: number;
    app_switch_rate_per_hour: number;
    avg_gap_minutes: number;
    longest_gap_minutes: number;
    first_capture_ts: number | null;
    last_capture_ts: number | null;
    capture_span_hours: number;
    current_streak_days: number;
    longest_streak_days: number;
    avg_ocr_confidence: number;
    low_confidence_records: number;
    avg_noise_score: number;
    high_noise_records: number;
    avg_ocr_blocks: number;
    llm_count: number;
    vlm_count: number;
    fallback_count: number;
    other_summary_count: number;
    top_domains: { domain: string; count: number }[];
    busiest_day: { day: string; count: number } | null;
    quietest_day: { day: string; count: number } | null;
    busiest_hour: { hour: number; count: number } | null;
    hourly_distribution: { hour: number; count: number }[];
    weekday_distribution: { weekday: string; count: number }[];
    daypart_distribution: { daypart: string; count: number }[];
}

const DEFAULT_STATS: Stats = {
    total_records: 0,
    total_days: 0,
    apps: [],
    today_count: 0,
    unique_apps: 0,
    unique_sessions: 0,
    unique_window_titles: 0,
    unique_urls: 0,
    unique_domains: 0,
    records_with_url: 0,
    records_with_screenshot: 0,
    records_with_clean_text: 0,
    records_last_hour: 0,
    records_last_24h: 0,
    records_last_7d: 0,
    avg_records_per_active_day: 0,
    avg_records_per_hour: 0,
    focus_app_share_pct: 0,
    app_switches: 0,
    app_switch_rate_per_hour: 0,
    avg_gap_minutes: 0,
    longest_gap_minutes: 0,
    first_capture_ts: null,
    last_capture_ts: null,
    capture_span_hours: 0,
    current_streak_days: 0,
    longest_streak_days: 0,
    avg_ocr_confidence: 0,
    low_confidence_records: 0,
    avg_noise_score: 0,
    high_noise_records: 0,
    avg_ocr_blocks: 0,
    llm_count: 0,
    vlm_count: 0,
    fallback_count: 0,
    other_summary_count: 0,
    top_domains: [],
    busiest_day: null,
    quietest_day: null,
    busiest_hour: null,
    hourly_distribution: [],
    weekday_distribution: [],
    daypart_distribution: [],
};

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
    linked_urls: string[];
    linked_memory_ids: string[];
}



export interface VoiceTranscriptionResult {
    text: string;
    backend: string;
}

export interface SpeechSynthesisResult {
    audio_path: string;
    voice_id: string;
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

// Debug-only raw retrieval path (no grouping/synthesis).
export async function searchRawResults(
    query: string,
    timeFilter?: string,
    appFilter?: string,
    limit?: number
): Promise<SearchResult[]> {
    return invoke<SearchResult[]>("search_raw_results", {
        query,
        timeFilter,
        appFilter,
        limit,
    });
}

export async function searchMemoryCards(
    query: string,
    timeFilter?: string,
    appFilter?: string,
    limit?: number
): Promise<MemoryCard[]> {
    return invoke<MemoryCard[]>("search_memory_cards", {
        query,
        timeFilter,
        appFilter,
        limit,
    });
}

export async function listMemoryCards(
    limit?: number,
    appFilter?: string
): Promise<MemoryCard[]> {
    return invoke<MemoryCard[]>("list_memory_cards", {
        limit,
        appFilter,
    });
}

export async function deleteMemory(memoryId: string): Promise<boolean> {
    return invoke<boolean>("delete_memory", { memoryId });
}

export async function generateDailyBriefing(mode?: "morning" | "evening"): Promise<string> {
    return invoke<string>("generate_daily_briefing", { mode });
}

export async function getFunGreeting(name?: string | null): Promise<string> {
    return invoke<string>("get_fun_greeting", { name });
}



// Capture control
export async function getStatus(): Promise<CaptureStatus> {
    return invoke<CaptureStatus>("get_status");
}

export async function getMcpServerStatus(): Promise<McpServerStatus> {
    return invoke<McpServerStatus>("get_mcp_server_status");
}

export async function startMcpServer(port?: number): Promise<McpServerStatus> {
    return invoke<McpServerStatus>("start_mcp_server", { port });
}

export async function stopMcpServer(): Promise<McpServerStatus> {
    return invoke<McpServerStatus>("stop_mcp_server");
}

// Meeting Recorder
export async function getMeetingStatus(): Promise<MeetingRecorderStatus> {
    return invoke<MeetingRecorderStatus>("get_meeting_status");
}

export function onMeetingStatus(handler: (status: MeetingRecorderStatus) => void): Promise<() => void> {
    return listen<MeetingRecorderStatus>("meeting://status", (event) => {
        handler(event.payload);
    });
}

export async function startMeetingRecording(
    title: string,
    participants: string[],
    model?: string
): Promise<MeetingRecorderStatus> {
    return invoke<MeetingRecorderStatus>("start_meeting_recording", { title, participants, model });
}

export async function stopMeetingRecording(): Promise<MeetingRecorderStatus> {
    return invoke<MeetingRecorderStatus>("stop_meeting_recording");
}

export async function listMeetings(): Promise<MeetingSession[]> {
    return invoke<MeetingSession[]>("list_meetings");
}

export async function deleteMeeting(meetingId: string): Promise<boolean> {
    return invoke<boolean>("delete_meeting", { meetingId });
}

export async function getMeetingTranscript(meetingId: string): Promise<MeetingTranscript> {
    return invoke<MeetingTranscript>("get_meeting_transcript", { meetingId });
}


export async function transcribeVoiceInput(
    audioBytes: number[],
    mimeType?: string
): Promise<VoiceTranscriptionResult> {
    return invoke<VoiceTranscriptionResult>("transcribe_voice_input", { audioBytes, mimeType });
}

export async function speakText(
    text: string,
    voiceId?: string
): Promise<SpeechSynthesisResult> {
    return invoke<SpeechSynthesisResult>("speak_text", { text, voiceId });
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
    const raw = await invoke<Partial<Stats>>("get_stats");
    return {
        ...DEFAULT_STATS,
        ...raw,
        apps: raw.apps ?? [],
        top_domains: raw.top_domains ?? [],
        busiest_day: raw.busiest_day ?? null,
        quietest_day: raw.quietest_day ?? null,
        busiest_hour: raw.busiest_hour ?? null,
        hourly_distribution: raw.hourly_distribution ?? [],
        weekday_distribution: raw.weekday_distribution ?? [],
        daypart_distribution: raw.daypart_distribution ?? [],
    };
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

export async function addTodo(
    title: string,
    taskType?: "Todo" | "Reminder" | "Followup"
): Promise<Task> {
    return invoke<Task>("add_todo", { title, taskType });
}

export async function dismissTodo(taskId: string): Promise<boolean> {
    return invoke<boolean>("dismiss_todo", { taskId });
}

export async function executeTodo(taskId: string): Promise<Task> {
    return invoke<Task>("execute_todo", { taskId });
}

export async function updateTodo(
    taskId: string,
    title: string,
    taskType?: "Todo" | "Reminder" | "Followup"
): Promise<Task> {
    return invoke<Task>("update_todo", { taskId, title, taskType });
}

// ========== Agent SDK Functions ==========

export interface AgentStatus {
    is_running: boolean;
    task_title: string | null;
    last_message: string | null;
    status: "idle" | "running" | "completed" | "error";
}

export async function startAgentTask(
    taskTitle: string,
    contextUrls?: string[],
    contextNotes?: string[]
): Promise<AgentStatus> {
    return invoke<AgentStatus>("start_agent_task", { taskTitle, contextUrls, contextNotes });
}

export async function getAgentStatus(): Promise<AgentStatus> {
    return invoke<AgentStatus>("get_agent_status");
}

export async function stopAgent(): Promise<AgentStatus> {
    return invoke<AgentStatus>("stop_agent");
}

export async function summarizeSearch(query: string, snippets: string[]): Promise<string> {
    return invoke<string>("summarize_search", { query, resultsSnippets: snippets });
}

export async function runMemoryRepairBackfill(): Promise<MemoryRepairSummary> {
    return invoke<MemoryRepairSummary>("run_memory_repair_backfill");
}

export async function getMemoryRepairProgress(): Promise<MemoryRepairProgress> {
    return invoke<MemoryRepairProgress>("get_memory_repair_progress");
}

export interface ChatMessage {
    role: "user" | "assistant";
    content: string;
}


export async function chatWithGemma(messages: ChatMessage[]): Promise<string> {
    const last = messages[messages.length - 1];
    if (!last) return "";
    const snippets = messages.filter((m) => m.role === "user").map((m) => m.content);
    return invoke<string>("summarize_search", { query: last.content, resultsSnippets: snippets });
}

