import { useEffect, useRef, useState } from "react";
import {
    MemoryCard,
    pauseCapture,
    resumeCapture,
    searchMemoryCards,
    summarizeSearch,
    transcribeVoiceInput,
} from "@/shared/ipc/tauri";
import {
    MEMORY_MENTIONS,
    SEARCH_PLACEHOLDER,
    SEARCH_SUMMARY,
    VOICE_RECORDING,
} from "@/shared/utils/config";
import { bubblePurityGate, extractAnchorTerms, scoreAnchorCoverage } from "@/shared/utils/search";
import { PLACEHOLDERS } from "./placeholders";
import { Icon } from "@/shared/components/atoms";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
    type ActivityTraceStep,
} from "@/shared/activity/activityTrace";
import {
    beginVoiceActivityTrace,
    recordVoiceActivityStep,
    type VoiceActivityEvent,
} from "@/shared/activity/voiceActivityTrace";
import { ActivityTrace } from "@/shared/components/ActivityTrace";
import "./SearchBar.css";

interface SearchBarProps {
    value: string;
    submittedValue: string;
    onChange: (value: string) => void;
    onSubmit: (value?: string) => void | Promise<void>;
    timeFilter: string | null;
    onTimeFilterChange: (filter: string | null) => void;
    appFilter: string | null;
    onAppFilterChange: (filter: string | null) => void;
    onSetMemoryCardsPanelOpen: (open: boolean) => void;
    appNames: string[];
    resultCount: number;
    searchResults: MemoryCard[];
    disabled?: boolean;
    disabledHint?: string;
}

const PLACEHOLDER_DISPLAY_DURATION = SEARCH_PLACEHOLDER.displayDurationMs;
const PLACEHOLDER_FADE_DURATION = SEARCH_PLACEHOLDER.fadeDurationMs;
const DEFAULT_PLACEHOLDER = "Recall a specific meeting, note, or idea...";

export function SearchBar({
    value,
    submittedValue,
    onChange,
    onSubmit,
    timeFilter,
    onTimeFilterChange,
    appFilter,
    onAppFilterChange,
    onSetMemoryCardsPanelOpen,
    appNames,
    resultCount,
    searchResults,
    disabled = false,
    disabledHint,
}: SearchBarProps) {
    const [summary, setSummary] = useState<string | null>(null);
    const [summaryActivityTrace, setSummaryActivityTrace] = useState<ActivityTraceSnapshot | null>(null);
    const [voiceStatus, setVoiceStatus] = useState<string | null>(null);
    const [voiceActivityTrace, setVoiceActivityTrace] = useState<ActivityTraceSnapshot | null>(null);
    const [isRecording, setIsRecording] = useState(false);
    const [isPreparingVoice, setIsPreparingVoice] = useState(false);
    const [isTranscribing, setIsTranscribing] = useState(false);
    const [placeholderIndex, setPlaceholderIndex] = useState(0);
    const [placeholderVisible, setPlaceholderVisible] = useState(true);

    const mediaRecorderRef = useRef<MediaRecorder | null>(null);
    const mediaStreamRef = useRef<MediaStream | null>(null);
    const audioChunksRef = useRef<Blob[]>([]);
    const inputRef = useRef<HTMLTextAreaElement>(null);
    const mimeTypeRef = useRef<string>("audio/webm");
    const recordingStartedAtRef = useRef<number>(0);
    const summaryRequestRef = useRef(0);
    const statusTimeoutRef = useRef<number | null>(null);
    const hasQuery = submittedValue.trim().length > 0;
    const hasPendingSubmit = value.trim() !== submittedValue.trim();
    const showMetaRow = hasQuery;
    const hasInput = value.length > 0;
    const activePlaceholder =
        PLACEHOLDERS[placeholderIndex % Math.max(PLACEHOLDERS.length, 1)] ?? DEFAULT_PLACEHOLDER;
    const showAnimatedPlaceholder = !hasInput;
    const uniqueAppNames = Array.from(
        new Set(appNames.map((name) => name.trim()).filter(Boolean))
    );

    const atMemoryMatch = /@memory\s+(.+)/i.exec(value);
    const atMemoryQuery = atMemoryMatch?.[1]?.trim() ?? "";
    const [memoryMentionHits, setMemoryMentionHits] = useState<MemoryCard[]>([]);
    const [memoryMentionBusy, setMemoryMentionBusy] = useState(false);
    const [memoryMentionError, setMemoryMentionError] = useState(false);

    function recordVoiceStep(event: VoiceActivityEvent, atMs = Date.now(), durationMs?: number) {
        setVoiceActivityTrace((current) => current
            ? recordVoiceActivityStep(current, event, atMs, durationMs)
            : current);
    }

    useEffect(() => {
        if (!atMemoryQuery || atMemoryQuery.length < MEMORY_MENTIONS.minQueryLength) {
            setMemoryMentionHits([]);
            setMemoryMentionBusy(false);
            setMemoryMentionError(false);
            return;
        }
        let cancelled = false;
        const timer = window.setTimeout(() => {
            void (async () => {
                setMemoryMentionBusy(true);
                setMemoryMentionError(false);
                try {
                    const hits = await searchMemoryCards(
                        atMemoryQuery,
                        undefined,
                        undefined,
                        MEMORY_MENTIONS.limit
                    );
                    if (!cancelled) {
                        setMemoryMentionHits(hits);
                    }
                } catch {
                    if (!cancelled) {
                        setMemoryMentionHits([]);
                        setMemoryMentionError(true);
                    }
                } finally {
                    if (!cancelled) {
                        setMemoryMentionBusy(false);
                    }
                }
            })();
        }, MEMORY_MENTIONS.debounceMs);
        return () => {
            cancelled = true;
            window.clearTimeout(timer);
        };
    }, [atMemoryQuery, value]);

    useEffect(() => {
        if (PLACEHOLDERS.length <= 1 || hasInput) {
            return;
        }

        let swapTimer: number | undefined;
        const displayTimer = window.setTimeout(() => {
            setPlaceholderVisible(false);
            swapTimer = window.setTimeout(() => {
                setPlaceholderIndex((index) => (index + 1) % PLACEHOLDERS.length);
                setPlaceholderVisible(true);
            }, PLACEHOLDER_FADE_DURATION);
        }, PLACEHOLDER_DISPLAY_DURATION);

        return () => {
            window.clearTimeout(displayTimer);
            if (swapTimer !== undefined) {
                window.clearTimeout(swapTimer);
            }
        };
    }, [hasInput, placeholderIndex]);

    useEffect(() => {
        const activeValue = submittedValue.trim();
        const requestId = ++summaryRequestRef.current;

        if (!activeValue || resultCount === 0) {
            setSummary(null);
            setSummaryActivityTrace(null);
            return;
        }

        let cancelled = false;
        setSummary(null);
        const startedAt = Date.now();
        const traceId = `search-summary-${requestId}`;
        setSummaryActivityTrace(recordActivityStep(
            beginActivityTrace({
                id: traceId,
                title: "Search summary activity",
                startedAtMs: startedAt,
            }),
            {
                id: "settle-delay",
                label: "Waiting for search results to settle",
                actor: "Search summary scheduler",
                status: "waiting",
                evidence: "frontend-event",
                atMs: startedAt,
            },
        ));

        const recordSummaryStep = (step: ActivityTraceStep) => {
            setSummaryActivityTrace((current) => current?.id === traceId
                ? recordActivityStep(current, step)
                : current);
        };

        const timer = window.setTimeout(async () => {
            const latestResults = searchResults;
            if (cancelled || requestId !== summaryRequestRef.current) {
                return;
            }

            const delayCompletedAt = Date.now();
            recordSummaryStep({
                id: "settle-delay",
                label: "Search-result settle delay completed",
                actor: "Search summary scheduler",
                status: "completed",
                evidence: "frontend-event",
                atMs: delayCompletedAt,
                durationMs: delayCompletedAt - startedAt,
            });

            if (latestResults.length === 0) {
                recordSummaryStep({
                    id: "result",
                    label: "Summary skipped: no retrieved memories",
                    actor: "Search evidence gate",
                    status: "completed",
                    evidence: "result-metadata",
                    atMs: Date.now(),
                    detail: "0 memories available",
                });
                return;
            }

            let requestStartedAt: number | null = null;
            try {
                const coverageStartedAt = Date.now();
                recordSummaryStep({
                    id: "coverage",
                    label: "Evaluating evidence coverage",
                    actor: "Search evidence gate",
                    status: "running",
                    evidence: "frontend-event",
                    atMs: coverageStartedAt,
                });
                const anchorTerms = extractAnchorTerms(activeValue);
                const topicalCards = latestResults
                    .map((result) => {
                        const fallbackText = [
                            result.title,
                            result.display_summary ?? result.summary,
                            result.summary,
                            ...(result.raw_snippets ?? []),
                        ]
                            .filter(Boolean)
                            .join(" ");
                        const coverage = result.anchor_coverage_score
                            ?? scoreAnchorCoverage(fallbackText, anchorTerms);
                        return { result, coverage };
                    })
                    .filter((item) => item.coverage >= SEARCH_SUMMARY.coverageFloor)
                    .slice(0, SEARCH_SUMMARY.maxCards);

                const coverageCompletedAt = Date.now();
                recordSummaryStep({
                    id: "coverage",
                    label: "Evaluated evidence coverage",
                    actor: "Search evidence gate",
                    status: "completed",
                    evidence: "result-metadata",
                    atMs: coverageCompletedAt,
                    durationMs: coverageCompletedAt - coverageStartedAt,
                    detail: `${topicalCards.length} of ${latestResults.length} memories met coverage`,
                });

                if (topicalCards.length < 2) {
                    setSummary(null);
                    recordSummaryStep({
                        id: "result",
                        label: "Summary skipped: insufficient evidence",
                        actor: "Search evidence gate",
                        status: "completed",
                        evidence: "result-metadata",
                        atMs: Date.now(),
                        detail: `${topicalCards.length} qualifying memories; 2 required`,
                    });
                    return;
                }

                const snippets = topicalCards
                    .flatMap(({ result }) => {
                        const evidence = (result.raw_snippets ?? [])
                            .map((snippet) => snippet.trim())
                            .filter(Boolean)
                            .slice(0, SEARCH_SUMMARY.snippetsPerCard);
                        if (evidence.length === 0) {
                            const fallback = (result.display_summary ?? result.summary ?? "").trim();
                            return fallback
                                ? [{ memoryId: result.id, score: result.score, appName: result.app_name, snippet: fallback }]
                                : [];
                        }
                        return evidence.map((snippet) => ({
                            memoryId: result.id,
                            score: result.score,
                            appName: result.app_name,
                            snippet,
                        }));
                    })
                    .slice(0, SEARCH_SUMMARY.maxSnippets)
                    .map(
                        (item) =>
                            `[id:${item.memoryId}][score:${item.score.toFixed(3)}][app:${item.appName}] ${item.snippet}`
                    );

                if (snippets.length < 2) {
                    setSummary(null);
                    recordSummaryStep({
                        id: "result",
                        label: "Summary skipped: insufficient evidence",
                        actor: "Search evidence gate",
                        status: "completed",
                        evidence: "result-metadata",
                        atMs: Date.now(),
                        detail: `${snippets.length} evidence items; 2 required`,
                    });
                    return;
                }

                requestStartedAt = Date.now();
                recordSummaryStep({
                    id: "summary-request",
                    label: "Requesting grounded summary",
                    actor: "Local summary service",
                    status: "running",
                    evidence: "ipc-boundary",
                    atMs: requestStartedAt,
                });
                const aiSummary = await summarizeSearch(activeValue, snippets);
                if (cancelled || requestId !== summaryRequestRef.current) {
                    return;
                }
                const requestCompletedAt = Date.now();
                recordSummaryStep({
                    id: "summary-request",
                    label: "Summary request completed",
                    actor: "Local summary service",
                    status: "completed",
                    evidence: "ipc-boundary",
                    atMs: requestCompletedAt,
                    durationMs: requestCompletedAt - requestStartedAt,
                });
                if (!aiSummary?.trim()) {
                    setSummary(null);
                    recordSummaryStep({
                        id: "result",
                        label: "Summary withheld: no usable result",
                        actor: "Search evidence gate",
                        status: "degraded",
                        evidence: "result-metadata",
                        atMs: Date.now(),
                    });
                    return;
                }

                const purity = bubblePurityGate(aiSummary, anchorTerms);
                if (!purity.pass) {
                    setSummary(null);
                    recordSummaryStep({
                        id: "result",
                        label: "Summary withheld by evidence check",
                        actor: "Search evidence gate",
                        status: "degraded",
                        evidence: "result-metadata",
                        atMs: Date.now(),
                    });
                    return;
                }
                setSummary(aiSummary);
                recordSummaryStep({
                    id: "result",
                    label: "Summary ready",
                    actor: "Search evidence gate",
                    status: "completed",
                    evidence: "result-metadata",
                    atMs: Date.now(),
                });
            } catch {
                if (cancelled || requestId !== summaryRequestRef.current) {
                    return;
                }
                console.error("Summary generation failed");
                setSummary(null);
                const failedAt = Date.now();
                recordSummaryStep({
                    id: requestStartedAt === null ? "summary-preparation" : "summary-request",
                    label: requestStartedAt === null
                        ? "Summary preparation failed"
                        : "Summary request failed",
                    actor: requestStartedAt === null
                        ? "Search evidence gate"
                        : "Local summary service",
                    status: "failed",
                    evidence: requestStartedAt === null
                        ? "frontend-event"
                        : "ipc-boundary",
                    atMs: failedAt,
                    durationMs: requestStartedAt === null
                        ? undefined
                        : failedAt - requestStartedAt,
                });
            }
        }, SEARCH_SUMMARY.delayMs);

        return () => {
            cancelled = true;
            window.clearTimeout(timer);
        };
    }, [appFilter, resultCount, searchResults, submittedValue, timeFilter]);
    
    useEffect(() => {
        return () => {
            stopMediaStream(mediaStreamRef.current);
            mediaStreamRef.current = null;
            if (statusTimeoutRef.current !== null) {
                window.clearTimeout(statusTimeoutRef.current);
            }
        };
    }, []);

    useEffect(() => {
        const handleKeydown = (event: KeyboardEvent) => {
            if (
                event.key === "Escape"
                && !disabled
                && document.activeElement === inputRef.current
            ) {
                onChange("");
                onSubmit("");
            }
        };

        window.addEventListener("keydown", handleKeydown);
        return () => window.removeEventListener("keydown", handleKeydown);
    }, [disabled, onChange, onSubmit]);

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
            onSubmit("");
            setVoiceStatus("Search cleared.");
            return;
        }

        if (normalized.startsWith("search for ")) {
            const nextQuery = cleaned.slice("search for ".length).trim();
            onChange(nextQuery);
            onSubmit(nextQuery);
            setVoiceStatus(`Searching for: ${nextQuery}`);
            return;
        }

        if (normalized.startsWith("find ")) {
            const nextQuery = cleaned.slice("find ".length).trim();
            onChange(nextQuery);
            onSubmit(nextQuery);
            setVoiceStatus(`Searching for: ${nextQuery}`);
            return;
        }

        if (normalized.startsWith("look for ")) {
            const nextQuery = cleaned.slice("look for ".length).trim();
            onChange(nextQuery);
            onSubmit(nextQuery);
            setVoiceStatus(`Searching for: ${nextQuery}`);
            return;
        }

        if (normalized.includes("open meetings") || normalized.includes("open meeting recorder")) {
            const nextQuery = "meeting notes and follow-ups";
            onChange(nextQuery);
            onSubmit(nextQuery);
            setVoiceStatus("Searching your meeting memories.");
            return;
        }

        if (normalized.includes("open graph") || normalized.includes("open knowledge graph")) {
            onSetMemoryCardsPanelOpen(true);
            setVoiceStatus("Opened Memory Vault. Connected context is available inside each memory.");
            return;
        }

        if (normalized.includes("open memory vault")) {
            onSetMemoryCardsPanelOpen(true);
            setVoiceStatus("Opened Memory Vault.");
            return;
        }

        if (normalized.includes("close memory vault")) {
            onSetMemoryCardsPanelOpen(false);
            setVoiceStatus("Closed Memory Vault.");
            return;
        }

        if (normalized.includes("pause capture") || normalized.includes("pause recording")) {
            try {
                await pauseCapture();
                setVoiceStatus("Capture paused. It will remain paused after relaunch.");
            } catch {
                setVoiceStatus("Couldn't pause capture. Use Capture settings to try again.");
            }
            return;
        }

        if (normalized.includes("resume capture") || normalized.includes("start capture")) {
            try {
                await resumeCapture();
                setVoiceStatus("Capture resumed.");
            } catch {
                setVoiceStatus("Couldn't resume capture. Use Capture settings to try again.");
            }
            return;
        }

        onChange(cleaned);
        onSubmit(cleaned);
        setVoiceStatus(`Searching for: ${cleaned}`);
        if (statusTimeoutRef.current !== null) {
            window.clearTimeout(statusTimeoutRef.current);
        }
        statusTimeoutRef.current = window.setTimeout(() => {
            setVoiceStatus(null);
            statusTimeoutRef.current = null;
        }, VOICE_RECORDING.statusClearMs);
    }

    async function handleVoiceToggle() {
        if (isRecording) {
            recordVoiceStep("recorder-stop-requested");
            mediaRecorderRef.current?.stop();
            return;
        }

        const requestedAt = Date.now();
        const nextTrace = beginVoiceActivityTrace("search", requestedAt);
        if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
            setVoiceStatus("Microphone isn't available here. Type your search instead.");
            setVoiceActivityTrace(recordVoiceActivityStep(
                nextTrace,
                "microphone-unavailable",
                Date.now(),
            ));
            return;
        }

        setVoiceActivityTrace(nextTrace);
        setIsPreparingVoice(true);
        setVoiceStatus("Waiting for microphone permission…");
        try {
            const stream = await navigator.mediaDevices.getUserMedia({
                audio: {
                    echoCancellation: true,
                    noiseSuppression: true,
                    autoGainControl: true,
                    channelCount: VOICE_RECORDING.channelCount,
                    sampleRate: VOICE_RECORDING.sampleRate,
                },
            });
            recordVoiceStep("microphone-connected");
            const options = chooseRecorderOptions();
            const recorder = options ? new MediaRecorder(stream, options) : new MediaRecorder(stream);

            mediaStreamRef.current = stream;
            mediaRecorderRef.current = recorder;
            audioChunksRef.current = [];
            mimeTypeRef.current = recorder.mimeType || options?.mimeType || "audio/webm";
            recordingStartedAtRef.current = Date.now();

            recorder.ondataavailable = (event) => {
                if (event.data.size > 0) {
                    audioChunksRef.current.push(event.data);
                }
            };

            recorder.onstop = () => {
                const chunks = [...audioChunksRef.current];
                audioChunksRef.current = [];
                const durationMs = Date.now() - recordingStartedAtRef.current;
                stopMediaStream(mediaStreamRef.current);
                mediaStreamRef.current = null;
                mediaRecorderRef.current = null;
                setIsRecording(false);
                recordVoiceStep("recording-stopped", Date.now(), durationMs);
                if (durationMs < VOICE_RECORDING.minDurationMs) {
                    setVoiceStatus("Hold the mic a bit longer and try again.");
                    recordVoiceStep("recording-too-short", Date.now(), durationMs);
                    return;
                }
                void transcribeRecordedVoice(chunks, mimeTypeRef.current);
            };

            recorder.start();
            setIsRecording(true);
            setVoiceStatus("Listening... tap again to stop.");
            recordVoiceStep("recording-started", recordingStartedAtRef.current);
        } catch (err) {
            console.error("Voice capture failed:", err);
            setVoiceStatus(microphoneFailureMessage(err));
            recordVoiceStep("microphone-failed");
            stopMediaStream(mediaStreamRef.current);
            mediaStreamRef.current = null;
            mediaRecorderRef.current = null;
            setIsRecording(false);
        } finally {
            setIsPreparingVoice(false);
        }
    }

    async function transcribeRecordedVoice(chunks: Blob[], mimeType: string) {
        if (chunks.length === 0) {
            setVoiceStatus("No voice input captured.");
            recordVoiceStep("no-audio");
            return;
        }

        setIsTranscribing(true);
        setVoiceStatus("Transcribing with Whisper...");
        const transcribingAt = Date.now();
        recordVoiceStep("transcription-requested", transcribingAt);

        try {
            const blob = new Blob(chunks, { type: mimeType });
            const audioBytes = Array.from(new Uint8Array(await blob.arrayBuffer()));
            const result = await transcribeVoiceInput(audioBytes, mimeType);
            await handleVoiceTranscript(result.text);
            const completedAt = Date.now();
            recordVoiceStep(
                result.text.trim() ? "transcript-ready" : "no-speech",
                completedAt,
                completedAt - transcribingAt,
            );
        } catch (err) {
            console.error("Voice transcription failed:", err);
            setVoiceStatus("Voice transcription failed. Type your search or try again.");
            const failedAt = Date.now();
            recordVoiceStep("transcription-failed", failedAt, failedAt - transcribingAt);
        } finally {
            setIsTranscribing(false);
        }
    }

    return (
        <div className="search-panel">
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

                    <div className="search-input-wrap">
                        <textarea
                            id="fndr-search-input"
                            ref={inputRef}
                            rows={1}
                            wrap="off"
                            value={value}
                            onChange={(e) => onChange(e.target.value.replace(/\r?\n/g, " "))}
                            onKeyDown={(e) => {
                                if (e.key === "Enter") {
                                    e.preventDefault();
                                    if (!disabled) {
                                        onSubmit();
                                    }
                                }
                            }}
                            placeholder={showAnimatedPlaceholder ? "" : activePlaceholder}
                            className="search-input search-input-cycling search-input-scrollable"
                            autoComplete="off"
                            disabled={disabled}
                            aria-disabled={disabled}
                            aria-label="Search memories"
                            enterKeyHint="search"
                            spellCheck={false}
                        />
                        <span
                            className="search-placeholder-overlay"
                            aria-hidden="true"
                            style={{
                                opacity: showAnimatedPlaceholder ? (placeholderVisible ? 1 : 0) : 0,
                                transform: showAnimatedPlaceholder
                                    ? placeholderVisible
                                        ? "translateY(-50%)"
                                        : "translateY(calc(-50% + 6px))"
                                    : "translateY(-50%)",
                                transition: `opacity ${PLACEHOLDER_FADE_DURATION}ms ease, transform ${PLACEHOLDER_FADE_DURATION}ms ease`,
                            }}
                        >
                            {activePlaceholder}
                        </span>
                    </div>

                    <button
                        type="button"
                        className={`fndr-os-chrome-btn voice-btn ${isRecording ? "recording" : ""}`}
                        onClick={() => void handleVoiceToggle()}
                        aria-label={
                            isRecording
                                ? "Stop voice recording"
                                : isPreparingVoice
                                  ? "Waiting for microphone permission"
                                  : isTranscribing
                                    ? "Transcribing voice recording"
                                    : "Start voice recording"
                        }
                        title={isRecording ? "Stop voice recording" : "Speak"}
                        disabled={disabled || isPreparingVoice || isTranscribing}
                        aria-busy={isPreparingVoice || isTranscribing}
                    >
                        <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden>
                            <rect x="5" y="8" width="2.5" height="10" rx="1.2" />
                            <rect x="10.75" y="5" width="2.5" height="14" rx="1.2" />
                            <rect x="16.5" y="9" width="2.5" height="8" rx="1.2" />
                        </svg>
                    </button>

                    {value && (
                        <button
                            className="search-clear"
                            onClick={() => {
                                onChange("");
                                onSubmit("");
                                window.requestAnimationFrame(() => inputRef.current?.focus());
                            }}
                            aria-label="Clear search"
                            disabled={disabled}
                        >
                            ×
                        </button>
                    )}
                </div>
                {atMemoryQuery.length >= MEMORY_MENTIONS.minQueryLength && (
                    <div className="memory-mention-popover" role="region" aria-label="Memory suggestions">
                        {memoryMentionBusy ? (
                            <div className="memory-mention-loading" role="status">Searching saved memories…</div>
                        ) : memoryMentionError ? (
                            <div className="memory-mention-empty" role="status">
                                Memory suggestions are unavailable. Keep typing or press Enter to search.
                            </div>
                        ) : memoryMentionHits.length === 0 ? (
                            <div className="memory-mention-empty" role="status">No matching saved memories</div>
                        ) : (
                            memoryMentionHits.map((h) => (
                                <button
                                    key={h.id}
                                    type="button"
                                    className="memory-mention-item"
                                    onClick={() => {
                                        const stamp = new Date(h.timestamp).toISOString();
                                        const block = `[memory ${h.app_name} @ ${stamp}] ${h.summary.slice(0, 200)}`;
                                        onChange(value.replace(/@memory\s+.*/i, block));
                                        window.requestAnimationFrame(() => inputRef.current?.focus());
                                    }}
                                >
                                    <span className="memory-mention-title">{h.title}</span>
                                    <span className="memory-mention-snippet">{h.summary.slice(0, 120)}</span>
                                </button>
                            ))
                        )}
                    </div>
                )}
            </div>

            {showMetaRow && (
                <div className="search-meta-row">
                    <div className="search-filters">
                        <div className="select-wrapper">
                            <svg className="filter-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                                <circle cx="12" cy="12" r="8" />
                                <path d="M12 8v4l2.5 2.5" />
                            </svg>
                            <select
                                value={timeFilter || ""}
                                onChange={(e) => onTimeFilterChange(e.target.value || null)}
                                className={`filter-select ${timeFilter ? "active" : ""}`}
                                disabled={disabled}
                                aria-label="Time range"
                            >
                                <option value="">All time</option>
                                <option value="1h">Last hour</option>
                                <option value="24h">Last 24 hours</option>
                                <option value="7d">Last 7 days</option>
                            </select>
                            <svg className="select-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                                <path d="M6 9l6 6 6-6" />
                            </svg>
                        </div>

                        <div className="select-wrapper">
                            <svg className="filter-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                                <path d="M4 6h7v5H4zM13 6h7v5h-7zM4 13h7v5H4zM13 13h7v5h-7z" />
                            </svg>
                            <select
                                value={appFilter || ""}
                                onChange={(e) => onAppFilterChange(e.target.value || null)}
                                className={`filter-select ${appFilter ? "active" : ""}`}
                                disabled={disabled}
                                aria-label="App"
                            >
                                <option value="">All apps</option>
                                {uniqueAppNames.map((name) => (
                                    <option key={name} value={name}>{name}</option>
                                ))}
                            </select>
                            <svg className="select-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                                <path d="M6 9l6 6 6-6" />
                            </svg>
                        </div>
                    </div>

                    <div
                        className="result-count"
                        role="status"
                        aria-live="polite"
                        aria-label="Search result count"
                    >
                        {resultCount === 1 ? "1 result" : `${resultCount} results`}
                    </div>
                </div>
            )}

            {hasPendingSubmit && (
                <div className="voice-status" role={voiceActivityTrace ? undefined : "status"}>
                    Press Enter to search
                </div>
            )}

            {voiceStatus && (
                <div
                    className={`voice-status ${isRecording ? "recording" : ""}`}
                    role={voiceActivityTrace ? undefined : "status"}
                    aria-live={voiceActivityTrace ? undefined : "polite"}
                >
                    {voiceStatus}
                </div>
            )}

            {voiceActivityTrace && (
                <ActivityTrace trace={voiceActivityTrace} className="search-voice-trace" />
            )}

            {hasQuery && resultCount > 0 && summaryActivityTrace && (
                <ActivityTrace
                    trace={summaryActivityTrace}
                    className="search-summary-trace"
                    announce={!summary}
                />
            )}

            {hasQuery && resultCount > 0 && summary && (
                <div className="summary-bubble" aria-live="polite">
                    <p className="summary-text">
                        <span className="summary-icon"><Icon name="lightbulb" size={15} /></span>
                        {summary}
                    </p>
                </div>
            )}
        </div>
    );
}

function microphoneFailureMessage(error: unknown): string {
    const name = error instanceof DOMException
        ? error.name
        : typeof error === "object" && error && "name" in error
          ? String(error.name)
          : "";

    if (name === "NotAllowedError" || name === "PermissionDeniedError") {
        return "Microphone permission wasn't granted. Type your search instead.";
    }
    if (name === "NotFoundError" || name === "DevicesNotFoundError") {
        return "No microphone was found. Type your search instead.";
    }
    return "Couldn't start the microphone. Type your search instead.";
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
            return {
                mimeType,
                audioBitsPerSecond: VOICE_RECORDING.audioBitsPerSecond,
            };
        }
    }

    return undefined;
}

function stopMediaStream(stream: MediaStream | null) {
    stream?.getTracks().forEach((track) => track.stop());
}
