import { useEffect, useMemo, useState } from "react";
import {
    MeetingRecorderStatus,
    MeetingSession,
    MeetingTranscript,
    addTodo,
    getMeetingStatus,
    getMeetingTranscript,
    listMeetings,
    startAgentTask,
    stopMeetingRecording,
} from "../api/tauri";
import "./MeetingRecorderPanel.css";

interface MeetingRecorderPanelProps {
    isVisible: boolean;
    onClose: () => void;
    onOpenAgent: () => void;
}

interface MeetingInsights {
    actionItems: string[];
    decisions: string[];
    reminders: string[];
}

export function MeetingRecorderPanel({ isVisible, onClose, onOpenAgent }: MeetingRecorderPanelProps) {
    const [status, setStatus] = useState<MeetingRecorderStatus | null>(null);
    const [meetings, setMeetings] = useState<MeetingSession[]>([]);
    const [selectedMeetingId, setSelectedMeetingId] = useState<string | null>(null);
    const [transcript, setTranscript] = useState<MeetingTranscript | null>(null);
    const [loading, setLoading] = useState(false);
    const [creatingFollowups, setCreatingFollowups] = useState(false);
    const [transcriptQuery, setTranscriptQuery] = useState("");
    const [error, setError] = useState<string | null>(null);

    const selectedMeeting = useMemo(
        () => meetings.find((m) => m.id === selectedMeetingId) ?? null,
        [meetings, selectedMeetingId]
    );

    const insights = useMemo(() => summarizeMeeting(transcript), [transcript]);
    const filteredSegments = useMemo(() => {
        if (!transcript) {
            return [];
        }
        const normalizedQuery = transcriptQuery.trim().toLowerCase();
        if (!normalizedQuery) {
            return transcript.segments;
        }
        return transcript.segments.filter((segment) => segment.text.toLowerCase().includes(normalizedQuery));
    }, [transcript, transcriptQuery]);

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

    const handleCopyHighlights = async () => {
        if (!transcript) return;
        const lines = [
            `# ${transcript.meeting.title}`,
            "",
            "## Action Items",
            ...insights.actionItems.map((item) => `- ${item}`),
            "",
            "## Decisions",
            ...insights.decisions.map((item) => `- ${item}`),
            "",
            "## Reminders",
            ...insights.reminders.map((item) => `- ${item}`),
        ];
        await navigator.clipboard.writeText(lines.join("\n"));
    };

    const handleCreateFollowups = async () => {
        if (creatingFollowups || !transcript) return;
        setCreatingFollowups(true);
        setError(null);
        try {
            const candidates = [...insights.actionItems, ...insights.reminders]
                .filter((item) => item.length > 3)
                .slice(0, 6);

            if (candidates.length === 0) {
                setError("No clear follow-up actions found yet in this meeting.");
                return;
            }

            for (const item of candidates) {
                await addTodo(item, `From meeting: ${transcript.meeting.title}`, "Followup");
            }
        } catch (err) {
            setError(String(err));
        } finally {
            setCreatingFollowups(false);
        }
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
            `Generate a structured post-meeting brief with decisions, blockers, and next steps: ${transcript.meeting.title}`,
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
                    <p>Auto-captured transcripts, ready when a meeting ends.</p>
                </div>
                <div className="meeting-header-actions">
                    <button className="ui-action-btn meeting-ghost-btn" onClick={() => refresh(true)} disabled={loading}>
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
                                <strong>{status?.is_recording ? "Meeting in progress" : "Auto monitor active"}</strong>
                            </div>
                            {status?.is_recording && (
                                <button className="ui-action-btn meeting-ghost-btn" onClick={handleStopNow}>Stop Now</button>
                            )}
                        </div>
                        <p className="meeting-subtle">
                            {status?.is_recording
                                ? `Recording: ${status.current_title ?? "Detected meeting"}`
                                : "Starts automatically when a supported meeting app is active."}
                        </p>
                        <p className="meeting-subtle">
                            Segments: {status?.segment_count ?? 0} • Audio: {status?.ffmpeg_available ? "ready" : "missing"}
                        </p>
                        <p className={`meeting-subtle meeting-consent ${status?.consent_state ?? "unknown"}`}>
                            Consent: {formatConsentState(status?.consent_state)}
                            {status?.consent_evidence ? ` • "${status.consent_evidence}"` : ""}
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
                            <p>
                                {selectedMeeting
                                    ? `${selectedMeeting.segment_count} segments • ${Math.max(
                                        1,
                                        Math.round(selectedMeeting.duration_seconds / 60)
                                    )} min`
                                    : "Select a meeting"}
                            </p>
                        </div>
                        {transcript && (
                            <div className="meeting-export-row">
                                <button className="ui-action-btn meeting-ghost-btn" onClick={handleCopyTranscript} disabled={!transcript?.full_text}>
                                    Copy
                                </button>
                                <button className="ui-action-btn meeting-ghost-btn" onClick={handleExportMarkdown} disabled={!transcript}>
                                    Markdown
                                </button>
                                <button className="ui-action-btn meeting-ghost-btn" onClick={handleExportJson} disabled={!transcript}>
                                    JSON
                                </button>
                                <button className="ui-action-btn meeting-ghost-btn" onClick={handleCopyHighlights}>
                                    Copy Highlights
                                </button>
                                <button
                                    className="ui-action-btn meeting-ghost-btn"
                                    onClick={handleCreateFollowups}
                                    disabled={creatingFollowups}
                                >
                                    {creatingFollowups ? "Creating..." : "Create Follow-ups"}
                                </button>
                                <button className="ui-action-btn meeting-primary-btn" onClick={handleAttachToAgent} disabled={!transcript?.full_text}>
                                    Attach to Run
                                </button>
                            </div>
                        )}
                    </div>

                    {transcript && (
                        <section className="meeting-insights">
                            <article className="meeting-insight-card">
                                <h4>Action Items</h4>
                                <ul>
                                    {insights.actionItems.map((item) => (
                                        <li key={item}>{item}</li>
                                    ))}
                                </ul>
                            </article>
                            <article className="meeting-insight-card">
                                <h4>Decisions</h4>
                                <ul>
                                    {insights.decisions.map((item) => (
                                        <li key={item}>{item}</li>
                                    ))}
                                </ul>
                            </article>
                            <article className="meeting-insight-card">
                                <h4>Reminders</h4>
                                <ul>
                                    {insights.reminders.map((item) => (
                                        <li key={item}>{item}</li>
                                    ))}
                                </ul>
                            </article>
                        </section>
                    )}

                    <div className="meeting-transcript">
                        <div className="meeting-transcript-toolbar">
                            <input
                                type="text"
                                value={transcriptQuery}
                                onChange={(event) => setTranscriptQuery(event.target.value)}
                                placeholder="Search transcript text..."
                            />
                            <span>
                                {transcript ? `${filteredSegments.length}/${transcript.segments.length} segments` : ""}
                            </span>
                        </div>
                        {!transcript && <p className="meeting-empty">No transcript selected.</p>}
                        {transcript && filteredSegments.length === 0 && (
                            <p className="meeting-empty">No segments match your search.</p>
                        )}
                        {filteredSegments.map((segment) => (
                            <article key={segment.id} className="segment-row">
                                <div className="segment-time">
                                    {formatTime(segment.start_timestamp)} - {formatTime(segment.end_timestamp)}
                                </div>
                                <div className="segment-text">
                                    {highlightSegment(segment.text || "(empty segment)", transcriptQuery)}
                                </div>
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

function formatConsentState(state?: "unknown" | "pending" | "detected" | "denied"): string {
    switch (state) {
        case "detected":
            return "detected";
        case "denied":
            return "denied";
        case "pending":
            return "pending";
        default:
            return "unknown";
    }
}

function summarizeMeeting(transcript: MeetingTranscript | null): MeetingInsights {
    const fallback: MeetingInsights = {
        actionItems: ["No clear action items yet."],
        decisions: ["No explicit decisions detected yet."],
        reminders: ["No time-sensitive reminders detected yet."],
    };

    if (!transcript?.full_text?.trim()) {
        return fallback;
    }

    const sentences = transcript.full_text
        .replace(/\n+/g, " ")
        .split(/(?<=[.!?])\s+/)
        .map((sentence) => sentence.trim())
        .filter((sentence) => sentence.length > 8);

    const actions = uniqTop(
        sentences.filter((sentence) =>
            /(action item|next step|todo|follow up|follow-up|send|share|review|implement|fix|ship|assign|owner)/i.test(
                sentence
            )
        )
    );
    const decisions = uniqTop(
        sentences.filter((sentence) =>
            /(decision|decided|agreed|approved|resolved|we will|we'll|finalize|chose)/i.test(sentence)
        )
    );
    const reminders = uniqTop(
        sentences.filter((sentence) =>
            /(by|before|deadline|due|tomorrow|next week|next month|monday|tuesday|wednesday|thursday|friday)/i.test(
                sentence
            )
        )
    );

    return {
        actionItems: actions.length > 0 ? actions : fallback.actionItems,
        decisions: decisions.length > 0 ? decisions : fallback.decisions,
        reminders: reminders.length > 0 ? reminders : fallback.reminders,
    };
}

function uniqTop(items: string[], limit = 5): string[] {
    const seen = new Set<string>();
    const output: string[] = [];
    for (const item of items) {
        const normalized = item.toLowerCase().replace(/\s+/g, " ").trim();
        if (!normalized || seen.has(normalized)) {
            continue;
        }
        seen.add(normalized);
        output.push(item);
        if (output.length >= limit) {
            break;
        }
    }
    return output;
}

function highlightSegment(text: string, query: string): (string | JSX.Element)[] | string {
    const normalizedQuery = query.trim();
    if (!normalizedQuery) {
        return text;
    }

    const escaped = normalizedQuery.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const pattern = new RegExp(`(${escaped})`, "ig");
    const parts = text.split(pattern);
    return parts.map((part, index) =>
        index % 2 === 1 ? <mark key={`${part}-${index}`}>{part}</mark> : part
    );
}
