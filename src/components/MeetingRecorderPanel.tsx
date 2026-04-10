import { useCallback, useEffect, useRef, useState } from "react";
import {
    MeetingRecorderStatus,
    MeetingSession,
    MeetingTranscript,
    getMeetingStatus,
    listMeetings,
    getMeetingTranscript,
    startMeetingRecording,
    stopMeetingRecording,
    startAgentTask,
} from "../api/tauri";
import "./MeetingRecorderPanel.css";

interface MeetingRecorderPanelProps {
    isVisible: boolean;
    onClose: () => void;
    onOpenAgent: () => void;
}

export function MeetingRecorderPanel({ isVisible, onClose, onOpenAgent }: MeetingRecorderPanelProps) {
    const [status, setStatus] = useState<MeetingRecorderStatus | null>(null);
    const [meetings, setMeetings] = useState<MeetingSession[]>([]);
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [transcript, setTranscript] = useState<MeetingTranscript | null>(null);
    const [transcriptLoading, setTranscriptLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    // New-recording form
    const [showNewForm, setShowNewForm] = useState(false);
    const [newTitle, setNewTitle] = useState("");
    const [startBusy, setStartBusy] = useState(false);

    // Live elapsed timer while recording
    const [elapsed, setElapsed] = useState(0);

    // View mode for transcript pane
    const [viewSegments, setViewSegments] = useState(false);

    const selectedMeeting = meetings.find((m) => m.id === selectedId) ?? null;

    // ── Load transcript ───────────────────────────────────────────────────────
    const loadTranscript = useCallback(async (id: string) => {
        setTranscriptLoading(true);
        try {
            const data = await getMeetingTranscript(id);
            setTranscript(data);
        } catch (err) {
            setError(String(err));
        } finally {
            setTranscriptLoading(false);
        }
    }, []);

    // ── Refresh ──────────────────────────────────────────────────────────────
    const selectedIdRef = useRef(selectedId);
    useEffect(() => { selectedIdRef.current = selectedId; }, [selectedId]);

    const refresh = useCallback(async (autoSelectCurrent = true) => {
        try {
            const [s, list] = await Promise.all([getMeetingStatus(), listMeetings()]);
            setStatus(s);
            setMeetings(list);
            setError(null);

            // Auto-select: prefer current recording, then keep existing selection, then first.
            // Use a ref for selectedId so this callback never goes stale.
            if (autoSelectCurrent) {
                const current = selectedIdRef.current;
                const next =
                    s.current_meeting_id ??
                    (list.find((m) => m.id === current)?.id) ??
                    list[0]?.id ??
                    null;
                if (next && next !== current) {
                    setSelectedId(next);
                    loadTranscript(next);
                }
            }
        } catch (err) {
            setError(String(err));
        }
    }, [loadTranscript]);

    // ── Polling ───────────────────────────────────────────────────────────────
    useEffect(() => {
        if (!isVisible) return;
        refresh(true);
        const id = window.setInterval(() => refresh(true), 4000);
        return () => window.clearInterval(id);
    }, [isVisible, refresh]);

    // ── Elapsed timer while recording ─────────────────────────────────────────
    useEffect(() => {
        if (!status?.is_recording || !status.started_at) {
            setElapsed(0);
            return;
        }
        const tick = () => setElapsed(Math.floor((Date.now() - status.started_at!) / 1000));
        tick();
        const id = window.setInterval(tick, 1000);
        return () => window.clearInterval(id);
    }, [status?.is_recording, status?.started_at]);

    const handleSelectMeeting = (id: string) => {
        setSelectedId(id);
        loadTranscript(id);
    };

    // ── Start / Stop ──────────────────────────────────────────────────────────
    const handleStart = async () => {
        if (!newTitle.trim()) return;
        setStartBusy(true);
        setError(null);
        try {
            await startMeetingRecording(newTitle.trim(), []);
            setNewTitle("");
            setShowNewForm(false);
            await refresh(true);
        } catch (err) {
            setError(String(err));
        } finally {
            setStartBusy(false);
        }
    };

    const handleStop = async () => {
        setError(null);
        try {
            await stopMeetingRecording();
            await refresh(true);
        } catch (err) {
            setError(String(err));
        }
    };

    // ── Export / Agent ────────────────────────────────────────────────────────
    const handleCopy = async () => {
        if (!transcript?.full_text) return;
        await navigator.clipboard.writeText(transcript.full_text);
    };

    const handleExportMd = () => {
        if (!transcript) return;
        const lines = [
            `# ${transcript.meeting.title}`,
            "",
            `Started: ${new Date(transcript.meeting.start_timestamp).toLocaleString()}`,
            transcript.meeting.end_timestamp
                ? `Ended: ${new Date(transcript.meeting.end_timestamp).toLocaleString()}`
                : "",
            `Duration: ${formatDuration(transcript.meeting.duration_seconds)}`,
            "",
            "## Transcript",
            "",
            transcript.full_text || "(No transcript yet)",
        ].filter((l) => l !== null);
        downloadText(`${safeName(transcript.meeting.title)}.md`, lines.join("\n"), "text/markdown");
    };

    const handleExportJson = () => {
        if (!transcript) return;
        downloadText(
            `${safeName(transcript.meeting.title)}.json`,
            JSON.stringify(transcript, null, 2),
            "application/json"
        );
    };

    const handleAttachToAgent = async () => {
        if (!transcript?.full_text) return;
        await startAgentTask(
            `Summarize meeting and extract action items: ${transcript.meeting.title}`,
            [],
            [`Meeting transcript:\n${transcript.full_text.slice(0, 9000)}`]
        );
        onOpenAgent();
    };

    if (!isVisible) return null;

    return (
        <div className="mp-overlay">
            {/* ── Header ──────────────────────────────────────────────────── */}
            <header className="mp-header">
                <div className="mp-header-left">
                    <h2 className="mp-title">Meeting Notes</h2>
                    <p className="mp-subtitle">
                        Whisper large-v3 turbo runs locally on your Mac and downloads on first use · transcripts save to Documents/FNDR Meetings
                    </p>
                </div>
                <div className="mp-header-actions">
                    {!status?.is_recording && (
                        <button
                            className="mp-btn-primary"
                            onClick={() => setShowNewForm((v) => !v)}
                        >
                            {showNewForm ? "Cancel" : "＋ New Recording"}
                        </button>
                    )}
                    {status?.is_recording && (
                        <button className="mp-btn-stop" onClick={handleStop}>
                            ⏹ Stop Recording
                        </button>
                    )}
                    <button className="mp-btn-ghost" onClick={onClose}>✕ Close</button>
                </div>
            </header>

            {/* ── New Recording Form ───────────────────────────────────────── */}
            {showNewForm && (
                <div className="mp-new-form">
                    <input
                        className="mp-new-input"
                        type="text"
                        placeholder="Meeting title (e.g. Q2 Sync, Design Review…)"
                        value={newTitle}
                        onChange={(e) => setNewTitle(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && handleStart()}
                        autoFocus
                    />
                    <button
                        className="mp-btn-primary"
                        onClick={handleStart}
                        disabled={startBusy || !newTitle.trim()}
                    >
                        {startBusy ? "Starting…" : "Start"}
                    </button>
                </div>
            )}

            {/* ── Error banner ─────────────────────────────────────────────── */}
            {error && (
                <div className="mp-error-bar">
                    {error}
                    <button className="mp-error-close" onClick={() => setError(null)}>✕</button>
                </div>
            )}

            {/* ── Body ─────────────────────────────────────────────────────── */}
            <div className="mp-body">
                {/* Sidebar */}
                <aside className="mp-sidebar">
                    {/* Live status card */}
                    <div className={`mp-status-card ${status?.is_recording ? "recording" : "idle"}`}>
                        <div className="mp-status-top">
                            <span className={`mp-dot ${status?.is_recording ? "live" : "idle"}`} />
                            <strong>
                                {status?.is_recording
                                    ? status.current_title ?? "Recording…"
                                    : "No active recording"}
                            </strong>
                        </div>
                        {status?.is_recording && (
                            <div className="mp-status-meta">
                                {formatDuration(elapsed)} elapsed · {status.segment_count} segments
                            </div>
                        )}
                        {!status?.is_recording && (
                            <p className="mp-status-hint">
                                {status?.ffmpeg_available
                                    ? "Click \u201c\uff0b New Recording\u201d to start, or FNDR will auto-detect Zoom / Meet / Teams."
                                    : "ffmpeg not found \u2014 audio capture unavailable. Install via Homebrew: brew install ffmpeg"}
                            </p>
                        )}
                        {!status?.ffmpeg_available && (
                            <div className="mp-ffmpeg-warning">
                                ffmpeg missing · install with: <code>brew install ffmpeg</code>
                            </div>
                        )}
                    </div>

                    {/* Meeting list */}
                    <div className="mp-meetings-section">
                        <h3 className="mp-section-label">Recent Meetings</h3>
                        {meetings.length === 0 ? (
                            <div className="mp-empty-list">
                                <span className="mp-empty-icon">🎙️</span>
                                <p>No meetings yet.<br />Start a recording above.</p>
                            </div>
                        ) : (
                            <div className="mp-meeting-list">
                                {meetings.map((m) => (
                                    <button
                                        key={m.id}
                                        className={`mp-meeting-item ${selectedId === m.id ? "active" : ""} ${m.status === "recording" ? "live" : ""}`}
                                        onClick={() => handleSelectMeeting(m.id)}
                                    >
                                        <div className="mp-meeting-item-top">
                                            {m.status === "recording" && <span className="mp-dot live small" />}
                                            <span className="mp-meeting-name">{m.title}</span>
                                        </div>
                                        <span className="mp-meeting-detail">
                                            {new Date(m.start_timestamp).toLocaleDateString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}
                                            {m.duration_seconds > 0 && ` · ${formatDuration(m.duration_seconds)}`}
                                            {m.segment_count > 0 && ` · ${m.segment_count} seg`}
                                        </span>
                                        <span className={`mp-meeting-status-badge ${m.status}`}>
                                            {m.status === "recording" ? "Live" : m.status === "stopped" ? "Done" : "Error"}
                                        </span>
                                    </button>
                                ))}
                            </div>
                        )}
                    </div>
                </aside>

                {/* Main transcript pane */}
                <main className="mp-main">
                    {!selectedMeeting ? (
                        <div className="mp-transcript-empty">
                            <span className="mp-empty-icon large">📝</span>
                            <p>Select a meeting from the list to view its transcript.</p>
                        </div>
                    ) : (
                        <>
                            {/* Transcript header */}
                            <div className="mp-transcript-header">
                                <div className="mp-transcript-title-group">
                                    <h3>{selectedMeeting.title}</h3>
                                    <p className="mp-transcript-meta">
                                        {new Date(selectedMeeting.start_timestamp).toLocaleString()}
                                        {selectedMeeting.duration_seconds > 0 && ` · ${formatDuration(selectedMeeting.duration_seconds)}`}
                                        {selectedMeeting.segment_count > 0 && ` · ${selectedMeeting.segment_count} segments`}
                                    </p>
                                </div>
                                <div className="mp-transcript-actions">
                                    <div className="mp-view-toggle">
                                        <button
                                            className={`mp-view-btn ${!viewSegments ? "active" : ""}`}
                                            onClick={() => setViewSegments(false)}
                                        >
                                            Full text
                                        </button>
                                        <button
                                            className={`mp-view-btn ${viewSegments ? "active" : ""}`}
                                            onClick={() => setViewSegments(true)}
                                        >
                                            Segments
                                        </button>
                                    </div>
                                    <button className="mp-btn-ghost" onClick={handleCopy} disabled={!transcript?.full_text}>
                                        Copy
                                    </button>
                                    <button className="mp-btn-ghost" onClick={handleExportMd} disabled={!transcript}>
                                        .md
                                    </button>
                                    <button className="mp-btn-ghost" onClick={handleExportJson} disabled={!transcript}>
                                        .json
                                    </button>
                                    <button
                                        className="mp-btn-primary"
                                        onClick={handleAttachToAgent}
                                        disabled={!transcript?.full_text}
                                    >
                                        Ask AI
                                    </button>
                                </div>
                            </div>

                            {/* Transcript body */}
                            <div className="mp-transcript-body">
                                {transcriptLoading ? (
                                    <div className="mp-transcript-loading">Loading transcript…</div>
                                ) : !transcript ? (
                                    <div className="mp-transcript-empty-inner">No transcript data.</div>
                                ) : !viewSegments ? (
                                    /* Full text view */
                                    <div className="mp-full-text">
                                        {transcript.full_text ? (
                                            <p>{transcript.full_text}</p>
                                        ) : (
                                            <div className="mp-transcript-empty-inner">
                                                {selectedMeeting.status === "recording"
                                                    ? "Transcription in progress — segments will appear as the recording progresses."
                                                    : "No transcript available yet. Transcription runs after the meeting ends."}
                                            </div>
                                        )}
                                    </div>
                                ) : (
                                    /* Segments view */
                                    <div className="mp-segments">
                                        {transcript.segments.length === 0 ? (
                                            <div className="mp-transcript-empty-inner">No segments yet.</div>
                                        ) : (
                                            transcript.segments.map((seg) => (
                                                <div key={seg.id} className="mp-segment">
                                                    <span className="mp-segment-time">
                                                        {formatTimestamp(seg.start_timestamp)} – {formatTimestamp(seg.end_timestamp)}
                                                    </span>
                                                    <p className="mp-segment-text">
                                                        {seg.text || <em>(empty)</em>}
                                                    </p>
                                                </div>
                                            ))
                                        )}
                                    </div>
                                )}
                            </div>
                        </>
                    )}
                </main>
            </div>
        </div>
    );
}

// ── Helpers ──────────────────────────────────────────────────────────────────
function safeName(title: string): string {
    return title.toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "") || "meeting";
}

function downloadText(filename: string, content: string, mime: string) {
    const blob = new Blob([content], { type: mime });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
}

function formatDuration(seconds: number): string {
    if (seconds < 60) return `${seconds}s`;
    const m = Math.floor(seconds / 60);
    const s = seconds % 60;
    if (m < 60) return s > 0 ? `${m}m ${s}s` : `${m}m`;
    const h = Math.floor(m / 60);
    const rm = m % 60;
    return `${h}h ${rm}m`;
}

function formatTimestamp(ts: number): string {
    return new Date(ts).toLocaleTimeString(undefined, {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
    });
}
