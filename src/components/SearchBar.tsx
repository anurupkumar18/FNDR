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
    disabled?: boolean;
    disabledHint?: string;
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
    disabled = false,
    disabledHint,
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
    const summaryRequestRef = useRef(0);
    const searchResultsRef = useRef(searchResults);
    const hasQuery = value.trim().length > 0;

    useEffect(() => {
        searchResultsRef.current = searchResults;
    }, [searchResults]);

    useEffect(() => {
        const activeValue = value.trim();
        const requestId = ++summaryRequestRef.current;

        if (!activeValue || resultCount === 0) {
            setSummary(null);
            setIsSummarizing(false);
            return;
        }

        let cancelled = false;
        setIsSummarizing(true);
        setSummary(null);

        const timer = window.setTimeout(async () => {
            const latestResults = searchResultsRef.current;
            if (cancelled || requestId !== summaryRequestRef.current) {
                return;
            }

            if (latestResults.length === 0) {
                setIsSummarizing(false);
                return;
            }

            try {
                const snippets = latestResults
                    .slice(0, 5)
                    .map((result) => `[${result.app_name}] ${result.snippet}`);

                const aiSummary = await summarizeSearch(activeValue, snippets);
                if (cancelled || requestId !== summaryRequestRef.current) {
                    return;
                }
                setSummary(aiSummary || "Found relevant memories.");
            } catch (err) {
                if (cancelled || requestId !== summaryRequestRef.current) {
                    return;
                }
                console.error("Summary generation failed:", err);
                setSummary(`Found ${latestResults.length} relevant memories.`);
            } finally {
                if (!cancelled && requestId === summaryRequestRef.current) {
                    setIsSummarizing(false);
                }
            }
        }, 600);

        return () => {
            cancelled = true;
            window.clearTimeout(timer);
        };
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
            const nextQuery = cleaned.slice("search for ".length).trim();
            onChange(nextQuery);
            setVoiceStatus(`Searching for: ${nextQuery}`);
            return;
        }

        if (normalized.startsWith("find ")) {
            const nextQuery = cleaned.slice("find ".length).trim();
            onChange(nextQuery);
            setVoiceStatus(`Searching for: ${nextQuery}`);
            return;
        }

        if (normalized.startsWith("look for ")) {
            const nextQuery = cleaned.slice("look for ".length).trim();
            onChange(nextQuery);
            setVoiceStatus(`Searching for: ${nextQuery}`);
            return;
        }

        if (normalized.includes("open meetings") || normalized.includes("open meeting recorder")) {
            onSetMeetingPanelOpen(true);
            setVoiceStatus("Opened Meetings.");
            return;
        }

        if (normalized.includes("close meetings") || normalized.includes("close meeting recorder")) {
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

        if (normalized.includes("resume capture") || normalized.includes("start capture")) {
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
        <div className="search-panel">
            {hasQuery && resultCount > 0 && (
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

            {disabled && disabledHint && (
                <p className="search-disabled-hint" role="status">
                    {disabledHint}
                </p>
            )}

            <div className="search-bar" role="search">
                <div className="search-input-group">
                    <svg className="search-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                        <circle cx="11" cy="11" r="8" />
                        <path d="M21 21l-4.35-4.35" />
                    </svg>

                    <input
                        id="fndr-search-input"
                        type="text"
                        value={value}
                        onChange={(e) => onChange(e.target.value)}
                        placeholder="What do you remember?"
                        className="search-input"
                        autoComplete="off"
                        disabled={disabled}
                        aria-disabled={disabled}
                    />

                    <button
                        className={`voice-btn ${isRecording ? "recording" : ""}`}
                        onClick={() => void handleVoiceToggle()}
                        aria-label={isRecording ? "Stop voice recording" : "Start voice recording"}
                        title={isRecording ? "Stop voice recording" : "Start voice recording"}
                        disabled={disabled || isTranscribing}
                    >
                        {isRecording ? "Stop" : isTranscribing ? "..." : "Mic"}
                    </button>

                    {value && (
                        <button
                            className="search-clear"
                            onClick={() => onChange("")}
                            aria-label="Clear search"
                            disabled={disabled}
                        >
                            ✕
                        </button>
                    )}
                </div>
            </div>

            {voiceStatus && (
                <div className={`voice-status ${isRecording ? "recording" : ""}`}>
                    {voiceStatus}
                </div>
            )}

            {hasQuery && (
                <div className="search-meta-row">
                    <div className="search-filters">
                        <div className="select-wrapper">
                            <select
                                value={timeFilter || ""}
                                onChange={(e) => onTimeFilterChange(e.target.value || null)}
                                className={`filter-select ${timeFilter ? "active" : ""}`}
                                disabled={disabled}
                            >
                                <option value="">Any time</option>
                                <option value="1h">Last hour</option>
                                <option value="24h">Last 24 hours</option>
                                <option value="7d">Last 7 days</option>
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
                                disabled={disabled}
                            >
                                <option value="">All apps</option>
                                {appNames.map((name) => (
                                    <option key={name} value={name}>{name}</option>
                                ))}
                            </select>
                            <svg className="select-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                                <path d="M6 9l6 6 6-6" />
                            </svg>
                        </div>
                    </div>

                    <div className="result-count">
                        {`${resultCount} results`}
                    </div>
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
