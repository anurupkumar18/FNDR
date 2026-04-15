import { useEffect, useMemo, useRef, useState } from "react";
import {
    MeetingRecorderStatus,
    MeetingSession,
    MeetingTranscript,
    deleteMeeting,
    getMeetingStatus,
    getMeetingTranscript,
    listMeetings,
    onMeetingStatus,
    startMeetingRecording,
    stopMeetingRecording,
} from "../api/tauri";
import "./MeetingRecorderPanel.css";

interface MeetingRecorderPanelProps {
    isVisible: boolean;
    onClose: () => void;
}

export function MeetingRecorderPanel({ isVisible, onClose }: MeetingRecorderPanelProps) {
    const [status, setStatus] = useState<MeetingRecorderStatus | null>(null);
    const [meetings, setMeetings] = useState<MeetingSession[]>([]);
    const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(null);
    const [transcript, setTranscript] = useState<MeetingTranscript | null>(null);
    const [titleInput, setTitleInput] = useState("");
    const [loading, setLoading] = useState(false);
    const [starting, setStarting] = useState(false);
    const [stopping, setStopping] = useState(false);
    const [deletingId, setDeletingId] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);
    const selectedMeetingIdRef = useRef<string | null>(null);

    useEffect(() => {
        selectedMeetingIdRef.current = selectedMeetingId;
    }, [selectedMeetingId]);

    const selectedMeeting = useMemo(
        () => meetings.find((meeting) => meeting.id === selectedMeetingId) ?? null,
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

            const currentSelection = selectedMeetingIdRef.current;
            const selectionStillExists = currentSelection
                ? meetingList.some((meeting) => meeting.id === currentSelection)
                : false;
            const nextId = selectionStillExists
                ? currentSelection
                : preferCurrent
                    ? meetingStatus.current_meeting_id ?? meetingList[0]?.id ?? null
                    : meetingList[0]?.id ?? null;

            selectedMeetingIdRef.current = nextId;
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
        let unlisten: (() => void) | null = null;

        const run = async () => {
            if (!mounted) return;
            await refresh(true);
        };

        const subscribe = async () => {
            try {
                unlisten = await onMeetingStatus((nextStatus) => {
                    if (!mounted) return;
                    setStatus(nextStatus);
                });
            } catch {
                // Ignore listener errors; manual refresh and actions still work.
            }
        };

        void run();
        void subscribe();

        return () => {
            mounted = false;
            if (unlisten) {
                unlisten();
            }
        };
    }, [isVisible]);

    const handleStart = async () => {
        if (starting || status?.is_recording) return;
        setStarting(true);
        setError(null);
        try {
            const title = titleInput.trim() || `Meeting ${new Date().toLocaleString()}`;
            await startMeetingRecording(title, []);
            setTitleInput("");
            await refresh(true);
        } catch (err) {
            setError(String(err));
        } finally {
            setStarting(false);
        }
    };

    const handleStop = async () => {
        if (stopping || !status?.is_recording) return;
        setStopping(true);
        setError(null);
        try {
            await stopMeetingRecording();
            await refresh(true);
        } catch (err) {
            setError(String(err));
        } finally {
            setStopping(false);
        }
    };

    const handleSelectMeeting = async (meetingId: string) => {
        selectedMeetingIdRef.current = meetingId;
        setSelectedMeetingId(meetingId);
        try {
            const data = await getMeetingTranscript(meetingId);
            setTranscript(data);
        } catch (err) {
            setError(String(err));
        }
    };

    const handleDeleteMeeting = async (meetingId: string) => {
        if (deletingId) return;
        setDeletingId(meetingId);
        setError(null);
        try {
            await deleteMeeting(meetingId);
            await refresh(false);
        } catch (err) {
            setError(String(err));
        } finally {
            setDeletingId(null);
        }
    };

    if (!isVisible) return null;

    return (
        <div className="meeting-panel">
            <header className="meeting-panel-header">
                <div className="meeting-headline">
                    <h2>Meetings</h2>
                    <p>Manual recording only. Start, stop, and review transcripts.</p>
                </div>
                <div className="meeting-header-actions">
                    <button className="ui-action-btn meeting-ghost-btn" onClick={() => void refresh(true)} disabled={loading}>
                        {loading ? "Refreshing..." : "Refresh"}
                    </button>
                    <button className="ui-action-btn meeting-close-btn" onClick={onClose}>
                        ✕ Close
                    </button>
                </div>
            </header>

            <div className="meeting-panel-body">
                <aside className="meeting-sidebar">
                    <section className="meeting-card meeting-card-primary">
                        <div className="meeting-live-row">
                            <div>
                                <span className={`meeting-dot ${status?.is_recording ? "live" : "idle"}`} />
                                <strong>{status?.is_recording ? "Recording in progress" : "Ready to record"}</strong>
                            </div>
                            {status?.is_recording && (
                                <button className="ui-action-btn meeting-ghost-btn" onClick={() => void handleStop()} disabled={stopping}>
                                    {stopping ? "Stopping..." : "Stop"}
                                </button>
                            )}
                        </div>
                        <p className="meeting-subtle">
                            {status?.is_recording
                                ? `Recording: ${status.current_title ?? "Meeting"}`
                                : "Click Start to begin recording your current meeting audio."}
                        </p>
                        <div className="meeting-start-row">
                            <input
                                className="meeting-title-input"
                                value={titleInput}
                                onChange={(event) => setTitleInput(event.target.value)}
                                placeholder="Meeting title (optional)"
                                disabled={Boolean(status?.is_recording)}
                            />
                            <button
                                className="ui-action-btn meeting-primary-btn"
                                onClick={() => void handleStart()}
                                disabled={Boolean(status?.is_recording) || starting || !status?.ffmpeg_available}
                            >
                                {starting ? "Starting..." : "Start"}
                            </button>
                        </div>
                        <p className="meeting-subtle">
                            Segments: {status?.segment_count ?? 0} • Audio: {status?.ffmpeg_available ? "ready" : "missing"}
                        </p>
                    </section>

                    <section className="meeting-card">
                        <h3>Recent Meetings</h3>
                        <div className="meeting-list">
                            {meetings.length === 0 && <p className="meeting-empty">No meetings captured yet.</p>}
                            {meetings.map((meeting) => (
                                <div key={meeting.id} className="meeting-list-row">
                                    <button
                                        className={`meeting-list-item ${selectedMeetingId === meeting.id ? "active" : ""}`}
                                        onClick={() => void handleSelectMeeting(meeting.id)}
                                    >
                                        <span className="meeting-title">{meeting.title}</span>
                                        <span className="meeting-sub">
                                            {new Date(meeting.start_timestamp).toLocaleString()} • {meeting.segment_count} segments
                                        </span>
                                    </button>
                                    <button
                                        className="ui-action-btn meeting-delete-btn"
                                        onClick={() => void handleDeleteMeeting(meeting.id)}
                                        disabled={deletingId === meeting.id}
                                        title="Delete this meeting"
                                        aria-label="Delete meeting"
                                    >
                                        {deletingId === meeting.id ? "Deleting..." : "Delete"}
                                    </button>
                                </div>
                            ))}
                        </div>
                    </section>
                </aside>

                <section className="meeting-main">
                    <div className="meeting-main-header">
                        <div>
                            <h3>{selectedMeeting?.title ?? "Transcript"}</h3>
                            <p>
                                {selectedMeeting
                                    ? `${selectedMeeting.segment_count} segments • ${formatDuration(selectedMeeting.duration_seconds)}`
                                    : "Select a meeting"}
                            </p>
                        </div>
                    </div>

                    <div className="meeting-transcript">
                        {!transcript && <p className="meeting-empty">No transcript selected.</p>}
                        {transcript && transcript.segments.length === 0 && (
                            <p className="meeting-empty">No transcript segments yet for this meeting.</p>
                        )}
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

function formatTime(ts: number): string {
    return new Date(ts).toLocaleTimeString(undefined, {
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
    });
}

function formatDuration(durationSeconds: number): string {
    const minutes = Math.max(1, Math.round(durationSeconds / 60));
    return `${minutes} min`;
}
