import { useEffect, useMemo, useState } from "react";
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
    const [starting, setStarting] = useState(false);
    const [stopping, setStopping] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const selectedMeeting = useMemo(
        () => meetings.find((m) => m.id === selectedMeetingId) ?? null,
        [meetings, selectedMeetingId]
    );

    const refresh = async (preferCurrent = true) => {
        if (!isVisible) return;
        try {
            const [meetingStatus, meetingList] = await Promise.all([
                getMeetingStatus(),
                listMeetings(),
            ]);

            setStatus(meetingStatus);
            setMeetings(meetingList);

            if (preferCurrent) {
                const nextId = meetingStatus.current_meeting_id ?? meetingList[0]?.id ?? null;
                setSelectedMeetingId(nextId);
                if (nextId) {
                    const data = await getMeetingTranscript(nextId);
                    setTranscript(data);
                }
            } else if (selectedMeetingId) {
                const stillExists = meetingList.some(m => m.id === selectedMeetingId);
                if (!stillExists) {
                    setSelectedMeetingId(meetingList[0]?.id ?? null);
                    if (meetingList[0]) {
                        const data = await getMeetingTranscript(meetingList[0].id);
                        setTranscript(data);
                    }
                }
            }
            setError(null);
        } catch (err) {
            setError(String(err));
        }
    };

    useEffect(() => {
        if (!isVisible) return;
        refresh(true);

        const unlistenPromise = onMeetingStatus((nextStatus) => {
            setStatus(nextStatus);
            if (!nextStatus.is_recording) {
                refresh(false);
            }
        });

        return () => {
             unlistenPromise.then(unlisten => unlisten());
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
        setSelectedMeetingId(meetingId);
        try {
            const data = await getMeetingTranscript(meetingId);
            setTranscript(data);
        } catch (err) {
            setError(String(err));
        }
    };

    const handleDelete = async (meetingId: string) => {
        try {
            await deleteMeeting(meetingId);
            await refresh(false);
        } catch (err) {
            setError(String(err));
        }
    };

    if (!isVisible) return null;

    return (
        <div className="meeting-panel simplified">
            <header className="meeting-panel-header">
                <div className="meeting-headline">
                    <h2>Meeting Intelligence</h2>
                    <p>Audio recording and post-meeting analysis.</p>
                </div>
                <button className="ui-action-btn meeting-btn meeting-close-btn" onClick={onClose}>
                    ✕ Close
                </button>
            </header>

            <div className="meeting-panel-content">
                {/* 1. Main Recording Control */}
                <section className="meeting-main-ctrl">
                    {!status?.is_recording && !stopping && (
                        <div className="meeting-start-form">
                            <input
                                className="meeting-title-input"
                                value={titleInput}
                                onChange={(e) => setTitleInput(e.target.value)}
                                placeholder="Enter meeting title..."
                            />
                            <button 
                                className="ui-action-btn meeting-btn meeting-hero-btn"
                                onClick={handleStart}
                                disabled={starting || !status?.ffmpeg_available}
                            >
                                {starting ? "Starting..." : "● START RECORDING"}
                            </button>
                        </div>
                    )}

                    {status?.is_recording && (
                        <div className="meeting-recording-state">
                            <div className="recording-pulse">
                                <span className="pulse-dot" />
                                <span>Recording: {status.current_title}</span>
                            </div>
                            <button className="ui-action-btn meeting-btn meeting-hero-btn" onClick={handleStop} disabled={stopping}>
                                {stopping ? "Analyzing..." : "■ STOP & ANALYZE"}
                            </button>
                        </div>
                    )}

                    {stopping && (
                        <div className="meeting-analyzing-state">
                            <div className="spinner" />
                            <p>Transcribing and extracting action items...</p>
                        </div>
                    )}
                </section>

                {/* 2. Breakdown Results */}
                {selectedMeeting && !status?.is_recording && !stopping && (
                    <section className="meeting-breakdown">
                        <div className="breakdown-header">
                            <h3>{selectedMeeting.title}</h3>
                            <span className="breakdown-meta">
                                {new Date(selectedMeeting.start_timestamp).toLocaleDateString()} • {Math.round(selectedMeeting.duration_seconds / 60)} min
                            </span>
                            <button
                                className="ui-action-btn meeting-btn delete-session-btn"
                                onClick={() => handleDelete(selectedMeeting.id)}
                            >
                                Delete
                            </button>
                        </div>

                        {selectedMeeting.breakdown ? (
                            <div className="breakdown-grids">
                                {selectedMeeting.breakdown.summary && (
                                    <div className="breakdown-item summary-box">
                                        <h4>Summary</h4>
                                        <p>{selectedMeeting.breakdown.summary}</p>
                                    </div>
                                )}
                                
                                <div className="breakdown-grid-row">
                                    <div className="breakdown-item todo-box">
                                        <h4>To-dos</h4>
                                        {selectedMeeting.breakdown.todos.length > 0 ? (
                                            <ul>{selectedMeeting.breakdown.todos.map((item, i) => <li key={i}>{item}</li>)}</ul>
                                        ) : <p className="empty-sub">No tasks detected.</p>}
                                    </div>
                                    <div className="breakdown-item reminder-box">
                                        <h4>Reminders</h4>
                                        {selectedMeeting.breakdown.reminders.length > 0 ? (
                                            <ul>{selectedMeeting.breakdown.reminders.map((item, i) => <li key={i}>{item}</li>)}</ul>
                                        ) : <p className="empty-sub">No reminders detected.</p>}
                                    </div>
                                    <div className="breakdown-item followup-box">
                                        <h4>Follow-ups</h4>
                                        {selectedMeeting.breakdown.followups.length > 0 ? (
                                            <ul>{selectedMeeting.breakdown.followups.map((item, i) => <li key={i}>{item}</li>)}</ul>
                                        ) : <p className="empty-sub">No follow-ups detected.</p>}
                                    </div>
                                </div>
                            </div>
                        ) : (
                            <p className="empty-results">No analysis results yet. Process transcript if needed.</p>
                        )}

                        <details className="raw-transcript-details">
                            <summary>View Full Transcript</summary>
                            <div className="transcript-text">
                                {transcript?.full_text || "Transcript not loaded."}
                            </div>
                        </details>
                    </section>
                )}

                {!selectedMeeting && !status?.is_recording && !stopping && (
                    <div className="meeting-empty-state">
                        <p>No meeting active. Start recording to capture one.</p>
                    </div>
                )}
            </div>

            {/* 3. Minimalist History Row */}
            {meetings.length > 1 && !status?.is_recording && !stopping && (
                <footer className="meeting-history-footer">
                    <span>Recent:</span>
                    <div className="history-pills">
                        {meetings.slice(0, 5).map(m => (
                            <button 
                                key={m.id} 
                                className={`ui-action-btn meeting-btn history-pill ${selectedMeetingId === m.id ? "active" : ""}`}
                                onClick={() => handleSelectMeeting(m.id)}
                            >
                                {m.title}
                            </button>
                        ))}
                    </div>
                </footer>
            )}

            {error && <div className="meeting-error">{error}</div>}
        </div>
    );
}
