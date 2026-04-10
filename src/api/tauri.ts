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
    ai_model_available: boolean;
    ai_model_loaded: boolean;
    loaded_model_id: string | null;
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

export interface GraphNodeData {
    id: string;
    node_type: string;
    label: string;
    created_at: number;
    metadata: Record<string, any>;
}

export interface GraphEdgeData {
    id: string;
    source: string;
    target: string;
    edge_type: string;
    timestamp: number;
    metadata: Record<string, any>;
}

export async function getGraphData(): Promise<{ nodes: GraphNodeData[]; edges: GraphEdgeData[] }> {
    return invoke<{ nodes: GraphNodeData[]; edges: GraphEdgeData[] }>("get_graph_data");
}


