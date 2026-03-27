import { useEffect, useMemo, useState } from "react";
import {
    MeetingRecorderStatus,
    MeetingSession,
    MeetingTranscript,
    getMeetingStatus,
    listMeetings,
    getMeetingTranscript,
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
    const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(null);
    const [transcript, setTranscript] = useState<MeetingTranscript | null>(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const selectedMeeting = useMemo(
        () => meetings.find((m) => m.id === selectedMeetingId) ?? null,
        [meetings, selectedMeetingId]
    );

    const refresh = async (preferCurrent = true) => {
        if (!isVisible) return;
        setLoading(true);
        try {
            const [meetingStatus, meetingList] = await Promise.all([
                getMeetingStatus(),
                listMeetings(),
            ]);

            setStatus(meetingStatus);
            setMeetings(meetingList);

            const nextId = preferCurrent
                ? meetingStatus.current_meeting_id ?? selectedMeetingId ?? meetingList[0]?.id ?? null
                : selectedMeetingId ?? meetingList[0]?.id ?? null;

            setSelectedMeetingId(nextId);
            if (nextId) {
                const data = await getMeetingTranscript(nextId);
                setTranscript(data);
            } else {
                setTranscript(null);
            }
            setError(null);
        } catch (err) {
            setError(String(err));
        } finally {
            setLoading(false);
        }
    };

    useEffect(() => {
        if (!isVisible) return;

        let mounted = true;
        const run = async () => {
            if (!mounted) return;
            await refresh(true);
        };

        run();
        const interval = window.setInterval(run, 4000);

        return () => {
            mounted = false;
            window.clearInterval(interval);
        };
    }, [isVisible]);

    const handleSelectMeeting = async (meetingId: string) => {
        setSelectedMeetingId(meetingId);
        try {
            const data = await getMeetingTranscript(meetingId);
            setTranscript(data);
        } catch (err) {
            setError(String(err));
        }
    };

    const handleCopyTranscript = async () => {
        if (!transcript?.full_text) return;
        await navigator.clipboard.writeText(transcript.full_text);
    };

    const handleExportMarkdown = () => {
        if (!transcript) return;
        const content = transcript.full_text
            ? transcript.full_text
            : "(No transcript yet)";
        const lines = [
            `# ${transcript.meeting.title}`,
            "",
            `- Model: ${transcript.meeting.model}`,
            `- Started: ${new Date(transcript.meeting.start_timestamp).toLocaleString()}`,
            "",
            "## Transcript",
            "",
            content,
            "",
        ];
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
        const clipped = transcript.full_text.slice(0, 9000);
        await startAgentTask(
            `Summarize meeting and generate action items: ${transcript.meeting.title}`,
            [],
            [`Meeting transcript:\n${clipped}`]
        );
        onOpenAgent();
    };

    const handleStopNow = async () => {
        try {
            await stopMeetingRecording();
            await refresh(true);
        } catch (err) {
            setError(String(err));
        }
    };

    if (!isVisible) return null;

    return (
        <div className="meeting-panel">
            <header className="meeting-panel-header">
                <div className="meeting-headline">
                    <h2>Meeting Notes</h2>
                    <p>FNDR auto-detects calls and records with Parakeet V3 Small.</p>
                    <p>Transcription runs right after a meeting ends, then is saved to Documents/FNDR Meetings.</p>
                </div>
                <div className="meeting-header-actions">
                    <button className="meeting-ghost-btn" onClick={() => refresh(true)} disabled={loading}>
                        {loading ? "Refreshing..." : "Refresh"}
                    </button>
                    <button className="meeting-close-btn" onClick={onClose}>Close</button>
                </div>
            </header>

            <div className="meeting-panel-body">
                <aside className="meeting-sidebar">
                    <section className="meeting-card meeting-card-primary">
                        <div className="meeting-live-row">
                            <div>
                                <span className={`meeting-dot ${status?.is_recording ? "live" : "idle"}`} />
                                <strong>{status?.is_recording ? "Meeting in progress" : "Auto monitor active"}</strong>
                            </div>
                            {status?.is_recording && (
                                <button className="meeting-ghost-btn" onClick={handleStopNow}>Stop Now</button>
                            )}
                        </div>
                        <p className="meeting-subtle">
                            {status?.is_recording
                                ? `Recording: ${status.current_title ?? "Detected meeting"}`
                                : "FNDR will auto-start when it detects Zoom/Meet/Teams/Webex sessions."}
                            <br/><br/>
                            <small><em>Note: If this is your first meeting, the FNDR background agent will silently download the 2.5GB Parakeet transcription model. It may take 1-3 minutes.</em></small>
                        </p>
                        <p className="meeting-subtle">
                            Segments: {status?.segment_count ?? 0} • Audio: {status?.ffmpeg_available ? "ready" : "missing"} • Model: Parakeet V3 Small
                        </p>
                    </section>

                    <section className="meeting-card">
                        <h3>Recent Meetings</h3>
                        <div className="meeting-list">
                            {meetings.length === 0 && <p className="meeting-empty">No meetings captured yet.</p>}
                            {meetings.map((meeting) => (
                                <button
                                    key={meeting.id}
                                    className={`meeting-list-item ${selectedMeetingId === meeting.id ? "active" : ""}`}
                                    onClick={() => handleSelectMeeting(meeting.id)}
                                >
                                    <span className="meeting-title">{meeting.title}</span>
                                    <span className="meeting-sub">
                                        {new Date(meeting.start_timestamp).toLocaleString()} • {meeting.segment_count} segments
                                    </span>
                                </button>
                            ))}
                        </div>
                    </section>
                </aside>

                <section className="meeting-main">
                    <div className="meeting-main-header">
                        <div>
                            <h3>{selectedMeeting?.title ?? "Transcript"}</h3>
                            <p>{selectedMeeting ? `${selectedMeeting.segment_count} segments` : "Select a meeting"}</p>
                        </div>
                        <div className="meeting-export-row">
                            <button className="meeting-ghost-btn" onClick={handleCopyTranscript} disabled={!transcript?.full_text}>
                                Copy
                            </button>
                            <button className="meeting-ghost-btn" onClick={handleExportMarkdown} disabled={!transcript}>
                                Markdown
                            </button>
                            <button className="meeting-ghost-btn" onClick={handleExportJson} disabled={!transcript}>
                                JSON
                            </button>
                            <button className="meeting-primary-btn" onClick={handleAttachToAgent} disabled={!transcript?.full_text}>
                                Attach to Run
                            </button>
                        </div>
                    </div>

                    <div className="meeting-transcript">
                        {!transcript && <p className="meeting-empty">No transcript selected.</p>}
                        {transcript?.segments.map((segment) => (
                            <article key={segment.id} className="segment-row">
                                <div className="segment-time">
                                    {formatTime(segment.start_timestamp)} - {formatTime(segment.end_timestamp)}
                                </div>
                                <div className="segment-text">{segment.text || "(empty segment)"}</div>
                            </article>
                        ))}
                    </div>
                </section>
            </div>

            {error && <div className="meeting-error">{error}</div>}
        </div>
    );
}

function safeName(title: string): string {
    return title.toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "") || "meeting";
}

function downloadText(filename: string, content: string, mime: string) {
    const blob = new Blob([content], { type: mime });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = filename;
    link.click();
    URL.revokeObjectURL(url);
}

function formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString(undefined, {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
    });
}
