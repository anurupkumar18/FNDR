import { invoke } from "@tauri-apps/api/core";

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

export interface CaptureStatus {
    is_capturing: boolean;
    is_paused: boolean;
    is_incognito: boolean;
    frames_captured: number;
    frames_dropped: number;
    last_capture_time: number;
}

export interface McpServerStatus {
    running: boolean;
    host: string;
    port: number;
    endpoint: string;
    last_error?: string | null;
}

export interface MeetingSession {
    id: string;
    title: string;
    participants: string[];
    model: string;
    status: "recording" | "stopped" | "error";
    start_timestamp: number;
    end_timestamp?: number | null;
    created_at: number;
    updated_at: number;
    segment_count: number;
    duration_seconds: number;
    meeting_dir: string;
    audio_dir: string;
    transcript_path?: string | null;
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
    segment_count: number;
    ffmpeg_available: boolean;
    transcription_backend: string;
    last_error?: string | null;
}

export interface MeetingTranscript {
    meeting: MeetingSession;
    segments: MeetingSegment[];
    full_text: string;
}

export interface MeetingSearchResult {
    meeting_id: string;
    meeting_title: string;
    segment_id: string;
    index: number;
    text: string;
    score: number;
    start_timestamp: number;
    end_timestamp: number;
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
    linked_urls: string[];
    linked_memory_ids: string[];
}

export interface MemoryCard {
    id: string;
    timestamp: number;
    app_name: string;
    window_title: string;
    snippet: string;
    url?: string;
    screenshot_path?: string;
    score: number;
    related_tasks: string[];
}

export interface MemoryReconstruction {
    answer: string;
    cards: MemoryCard[];
    structural_context: string[];
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

export async function reconstructMemory(query: string, limit?: number): Promise<MemoryReconstruction> {
    return invoke<MemoryReconstruction>("reconstruct_memory", { query, limit });
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

export interface AppConfigPayload {
    experimental_ui_enabled: boolean;
    use_demo_data_only: boolean;
    use_vlm: boolean;
}

export async function getAppConfig(): Promise<AppConfigPayload> {
    return invoke<AppConfigPayload>("get_app_config");
}

export interface SystemReadiness {
    screen_capture_permission_granted: boolean;
    screen_capture_permission_detail: string;
    ocr_available: boolean;
    ocr_detail: string;
    inference_ready: boolean;
    embedder_ready: boolean;
    vector_store_ready: boolean;
    data_dir_writable: boolean;
    data_dir_detail: string;
    capture_status: CaptureStatus;
    total_records: number;
    vlm_active: boolean;
    use_demo_data_only: boolean;
    ready_for_search: boolean;
    fixes: string[];
}

export async function getReadiness(): Promise<SystemReadiness> {
    return invoke<SystemReadiness>("get_readiness");
}

export async function setUseDemoDataOnly(enabled: boolean): Promise<AppConfigPayload> {
    return invoke<AppConfigPayload>("set_use_demo_data_only", { enabled });
}

export async function seedDemoDataset(): Promise<number> {
    return invoke<number>("seed_demo_dataset");
}

export async function resetDemoData(): Promise<number> {
    return invoke<number>("reset_demo_data");
}

export async function injectTestMemory(): Promise<string> {
    return invoke<string>("inject_test_memory");
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

export async function getMeetingTranscript(meetingId: string): Promise<MeetingTranscript> {
    return invoke<MeetingTranscript>("get_meeting_transcript", { meetingId });
}

export async function searchMeetingTranscripts(
    query: string,
    limit?: number
): Promise<MeetingSearchResult[]> {
    return invoke<MeetingSearchResult[]>("search_meeting_transcripts", { query, limit });
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

// ========== Graph Visualization Functions ==========

export interface GraphNodeData {
    id: string;
    label: string;
    node_type: string;
    created_at: number;
    metadata: Record<string, unknown>;
}

export interface GraphEdgeData {
    id: string;
    source: string;
    target: string;
    edge_type: string;
    label: string;
    timestamp: number;
}

export interface GraphData {
    nodes: GraphNodeData[];
    edges: GraphEdgeData[];
}

export async function getGraphData(): Promise<GraphData> {
    return invoke<GraphData>("get_graph_data");
}

export async function searchGraph(query: string, limit?: number): Promise<SearchResult[]> {
    return invoke<SearchResult[]>("search_graph", { query, limit });
}
