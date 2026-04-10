import { useEffect, useRef, useState } from "react";
import {
    SearchResult,
    pauseCapture,
    resumeCapture,
    summarizeSearch,
    transcribeVoiceInput,
} from "../api/tauri";
import "./SearchBar.css";

interface SearchBarProps {
    value: string;
    onChange: (value: string) => void;
    timeFilter: string | null;
    onTimeFilterChange: (filter: string | null) => void;
    appFilter: string | null;
    onAppFilterChange: (filter: string | null) => void;
    onSetMeetingPanelOpen: (open: boolean) => void;
    onSetGraphPanelOpen: (open: boolean) => void;
    appNames: string[];
    resultCount: number;
    searchResults: SearchResult[];
}

export function SearchBar({
    value,
    onChange,
    timeFilter,
    onTimeFilterChange,
    appFilter,
    onAppFilterChange,
    onSetMeetingPanelOpen,
    onSetGraphPanelOpen,
    appNames,
    resultCount,
    searchResults,
}: SearchBarProps) {
    const [summary, setSummary] = useState<string | null>(null);
    const [isSummarizing, setIsSummarizing] = useState(false);
    const [voiceStatus, setVoiceStatus] = useState<string | null>(null);
    const [isRecording, setIsRecording] = useState(false);
    const [isTranscribing, setIsTranscribing] = useState(false);

    const mediaRecorderRef = useRef<MediaRecorder | null>(null);
    const mediaStreamRef = useRef<MediaStream | null>(null);
    const audioChunksRef = useRef<Blob[]>([]);
    const mimeTypeRef = useRef<string>("audio/webm");
    // Ref so the 600ms timer always reads the latest results without restarting the debounce
    const searchResultsRef = useRef(searchResults);
    useEffect(() => { searchResultsRef.current = searchResults; }, [searchResults]);

    useEffect(() => {
        if (!value.trim() || resultCount === 0) {
            setSummary(null);
            setIsSummarizing(false);
            return;
        }

        setIsSummarizing(true);
        setSummary(null);

        const timer = setTimeout(async () => {
            const latestResults = searchResultsRef.current;
            if (latestResults.length === 0) {
                setIsSummarizing(false);
                return;
            }
            try {
                const snippets = latestResults
                    .slice(0, 5)
                    .map((result) => `[${result.app_name}] ${result.snippet}`);

                const aiSummary = await summarizeSearch(value, snippets);
                setSummary(aiSummary || "Found relevant memories.");
            } catch (err) {
                console.error("Summary generation failed:", err);
                setSummary(`Found ${resultCount} relevant memories.`);
            } finally {
                setIsSummarizing(false);
            }
        }, 600);

        return () => clearTimeout(timer);
    }, [value, resultCount]);

    useEffect(() => {
        return () => {
            stopMediaStream(mediaStreamRef.current);
            mediaStreamRef.current = null;
        };
    }, []);

    async function handleVoiceTranscript(transcript: string) {
        const cleaned = transcript.trim();
        if (!cleaned) {
            setVoiceStatus("I didn't catch that.");
            return;
        }

        const normalized = cleaned.toLowerCase();
        setVoiceStatus(`Heard: ${cleaned}`);

        if (normalized === "clear" || normalized === "clear search" || normalized === "reset search") {
            onChange("");
            setVoiceStatus("Search cleared.");
            return;
        }

        if (normalized.startsWith("search for ")) {
            const searchQuery = cleaned.slice("search for ".length).trim();
            onChange(searchQuery);
            setVoiceStatus(`Searching for: ${searchQuery}`);
            return;
        }

        if (normalized.startsWith("find ")) {
            const searchQuery = cleaned.slice("find ".length).trim();
            onChange(searchQuery);
            setVoiceStatus(`Searching for: ${searchQuery}`);
            return;
        }

        if (normalized.startsWith("look for ")) {
            const searchQuery = cleaned.slice("look for ".length).trim();
            onChange(searchQuery);
            setVoiceStatus(`Searching for: ${searchQuery}`);
            return;
        }

        if (normalized.includes("open meetings") || normalized.includes("open meeting recorder") || normalized.includes("open meeting notes")) {
            onSetMeetingPanelOpen(true);
            setVoiceStatus("Opened Meetings.");
            return;
        }

        if (normalized.includes("close meetings") || normalized.includes("close meeting recorder") || normalized.includes("close meeting notes")) {
            onSetMeetingPanelOpen(false);
            setVoiceStatus("Closed Meetings.");
            return;
        }

        if (normalized.includes("open graph")) {
            onSetGraphPanelOpen(true);
            setVoiceStatus("Opened Graph.");
            return;
        }

        if (normalized.includes("close graph")) {
            onSetGraphPanelOpen(false);
            setVoiceStatus("Closed Graph.");
            return;
        }

        if (normalized.includes("pause capture") || normalized.includes("pause recording")) {
            await pauseCapture();
            setVoiceStatus("Capture paused.");
            return;
        }

        if (normalized.includes("resume capture") || normalized.includes("resume recording") || normalized.includes("start capture")) {
            await resumeCapture();
            setVoiceStatus("Capture resumed.");
            return;
        }

        onChange(cleaned);
        setVoiceStatus(`Searching for: ${cleaned}`);
    }

    async function handleVoiceToggle() {
        if (isRecording) {
            mediaRecorderRef.current?.stop();
            return;
        }

        if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
            setVoiceStatus("Voice capture is not supported in this build.");
            return;
        }

        try {
            const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
            const options = chooseRecorderOptions();
            const recorder = options ? new MediaRecorder(stream, options) : new MediaRecorder(stream);

            mediaStreamRef.current = stream;
            mediaRecorderRef.current = recorder;
            audioChunksRef.current = [];
            mimeTypeRef.current = recorder.mimeType || options?.mimeType || "audio/webm";

            recorder.ondataavailable = (event) => {
                if (event.data.size > 0) {
                    audioChunksRef.current.push(event.data);
                }
            };

            recorder.onstop = () => {
                const chunks = [...audioChunksRef.current];
                audioChunksRef.current = [];
                stopMediaStream(mediaStreamRef.current);
                mediaStreamRef.current = null;
                mediaRecorderRef.current = null;
                setIsRecording(false);
                void transcribeRecordedVoice(chunks, mimeTypeRef.current);
            };

            recorder.start();
            setIsRecording(true);
            setVoiceStatus("Listening... tap again to stop.");
        } catch (err) {
            console.error("Voice capture failed:", err);
            setVoiceStatus("Microphone access failed.");
            stopMediaStream(mediaStreamRef.current);
            mediaStreamRef.current = null;
            mediaRecorderRef.current = null;
            setIsRecording(false);
        }
    }

    async function transcribeRecordedVoice(chunks: Blob[], mimeType: string) {
        if (chunks.length === 0) {
            setVoiceStatus("No voice input captured.");
            return;
        }

        setIsTranscribing(true);
        setVoiceStatus("Transcribing with Whisper...");

        try {
            const blob = new Blob(chunks, { type: mimeType });
            const audioBytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
            const result = await transcribeVoiceInput(audioBytes, mimeType);
            await handleVoiceTranscript(result.text);
        } catch (err) {
            console.error("Voice transcription failed:", err);
            setVoiceStatus(`Voice transcription failed: ${String(err)}`);
        } finally {
            setIsTranscribing(false);
        }
    }

    return (
        <div className="search-overlay">
            {value.trim() && resultCount > 0 && (
                <div className="summary-bubble">
                    {isSummarizing ? (
                        <div className="summary-loading">
                            <span className="summary-spinner" />
                            <span>Synthesizing memories...</span>
                        </div>
                    ) : (
                        <p className="summary-text">
                            <span className="summary-icon">💡</span>
                            {summary}
                        </p>
                    )}
                </div>
            )}

            <div className="search-bar">
                <div className="search-input-group">
                    <svg className="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <circle cx="11" cy="11" r="8" />
                        <path d="M21 21l-4.35-4.35" />
                    </svg>

                    <input
                        type="text"
                        value={value}
                        onChange={(e) => onChange(e.target.value)}
                        placeholder="Search your memories..."
                        className="search-input"
                        autoComplete="off"
                    />

                    <button
                        className={`voice-btn ${isRecording ? "recording" : ""}`}
                        onClick={() => void handleVoiceToggle()}
                        aria-label={isRecording ? "Stop voice recording" : "Start voice recording"}
                        title={isRecording ? "Stop voice recording" : "Start voice recording"}
                        disabled={isTranscribing}
                    >
                        {isRecording ? "Stop" : isTranscribing ? "..." : "Mic"}
                    </button>

                    {value && (
                        <button
                            className="search-clear"
                            onClick={() => onChange("")}
                            aria-label="Clear search"
                        >
                            ✕
                        </button>
                    )}
                </div>

                <div className="search-filters">
                    <div className="select-wrapper">
                        <select
                            value={timeFilter || ""}
                            onChange={(e) => onTimeFilterChange(e.target.value || null)}
                            className={`filter-select ${timeFilter ? "active" : ""}`}
                        >
                            <option value="">⏱ Any Time</option>
                            <option value="1h">Last Hour</option>
                            <option value="24h">Last 24 Hours</option>
                            <option value="7d">Last 7 Days</option>
                        </select>
                        <svg className="select-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                            <path d="M6 9l6 6 6-6" />
                        </svg>
                    </div>

                    <div className="select-wrapper">
                        <select
                            value={appFilter || ""}
                            onChange={(e) => onAppFilterChange(e.target.value || null)}
                            className={`filter-select ${appFilter ? "active" : ""}`}
                        >
                            <option value="">📱 All Apps</option>
                            {appNames.map((name) => (
                                <option key={name} value={name}>{name}</option>
                            ))}
                        </select>
                        <svg className="select-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                            <path d="M6 9l6 6 6-6" />
                        </svg>
                    </div>
                </div>
            </div>

            {voiceStatus && (
                <div className={`voice-status ${isRecording ? "recording" : ""}`}>
                    {voiceStatus}
                </div>
            )}
        </div>
    );
}

function chooseRecorderOptions(): MediaRecorderOptions | undefined {
    const candidates = [
        "audio/webm;codecs=opus",
        "audio/mp4",
        "audio/ogg;codecs=opus",
        "audio/webm",
    ];

    for (const mimeType of candidates) {
        if (MediaRecorder.isTypeSupported(mimeType)) {
            return { mimeType };
        }
    }

    return undefined;
}

function stopMediaStream(stream: MediaStream | null) {
    stream?.getTracks().forEach((track) => track.stop());
}
