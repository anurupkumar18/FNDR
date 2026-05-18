import { useEffect, useRef, useState } from "react";
import {
    motion,
    useMotionValue,
    useSpring,
    useTransform,
    type MotionValue,
} from "framer-motion";
import { transcribeVoiceInput } from "@/shared/ipc/tauri";
import { staggerContainer, fadeUp, useReducedMotionSafe, s, motionTokens } from "@/shared/motion";
import { VOICE_RECORDING } from "@/shared/utils/config";
import "./HeroSection.css";

interface HeroSectionProps {
    onEnterReel: () => void;
    onEnterWorkMode: () => void;
    onScrollToSearch: (query: string) => void;
    greeting?: string;
    dateLabel?: string;
}

function getSubtitle(greeting: string): string {
    const g = greeting.toLowerCase();
    if (g.includes("morning")) return "Let's see what the morning holds.";
    if (g.includes("afternoon")) return "Let's pick up where you left off.";
    if (g.includes("evening")) return "Let's revisit your day.";
    return "Let's dive into your memories.";
}

// ── Parallax helpers ───────────────────────────────────────────────────────

function layerX(spring: MotionValue<number>, px: number) {
    // eslint-disable-next-line react-hooks/rules-of-hooks
    return useTransform(spring, [-1, 1], [-px, px]);
}
function layerY(spring: MotionValue<number>, px: number) {
    // eslint-disable-next-line react-hooks/rules-of-hooks
    return useTransform(spring, [-1, 1], [-px * 0.6, px * 0.6]);
}

// ── Constants ──────────────────────────────────────────────────────────────

const DATE_PX = 6;
const TITLE_PX = 22;
const SUB_PX = 16;
const SEARCH_PX = 10;
const SCROLL_PX = 4;

// ── Component ──────────────────────────────────────────────────────────────

export function HeroSection({
    onEnterReel: _onEnterReel,
    onEnterWorkMode: _onEnterWorkMode,
    onScrollToSearch,
    greeting: greetingProp,
    dateLabel: dateLabelProp,
}: HeroSectionProps) {
    const { reduced, transition } = useReducedMotionSafe();

    const greeting = greetingProp?.trim() || "Welcome back to FNDR.";
    const dateLabel = dateLabelProp?.trim() || "";
    const subtitle = getSubtitle(greeting);

    // ── Parallax motion values ──────────────────────────────────────────────
    const mx = useMotionValue(0);
    const my = useMotionValue(0);
    const sx = useSpring(mx, { stiffness: 80, damping: 22, mass: 1 });
    const sy = useSpring(my, { stiffness: 80, damping: 22, mass: 1 });

    const dateTx = layerX(sx, DATE_PX);
    const dateTy = layerY(sy, DATE_PX);
    const titleTx = layerX(sx, TITLE_PX);
    const titleTy = layerY(sy, TITLE_PX);
    const subTx = layerX(sx, SUB_PX);
    const subTy = layerY(sy, SUB_PX);
    const searchTx = layerX(sx, SEARCH_PX);
    const searchTy = layerY(sy, SEARCH_PX);
    const scrollTx = layerX(sx, SCROLL_PX);
    const scrollTy = layerY(sy, SCROLL_PX);

    const handleMouseMove = (e: React.MouseEvent<HTMLElement>) => {
        const r = e.currentTarget.getBoundingClientRect();
        mx.set(((e.clientX - r.left) / r.width) * 2 - 1);
        my.set(((e.clientY - r.top) / r.height) * 2 - 1);
    };
    const handleMouseLeave = () => {
        mx.set(0);
        my.set(0);
    };

    const handleSearchSubmit = (query: string) => {
        onScrollToSearch(query);
    };

    return (
        <section
            id="fndr-section-hero"
            data-section-id="hero"
            className="fndr-section fndr-hero-section film-grain"
            onMouseMove={reduced ? undefined : handleMouseMove}
            onMouseLeave={reduced ? undefined : handleMouseLeave}
        >
            <motion.div
                className="fndr-hero-stage"
                variants={staggerContainer}
                initial="hidden"
                animate="visible"
                transition={transition({ delayChildren: s(motionTokens.dur.fast) })}
            >
                {dateLabel && (
                    <motion.p
                        className="fndr-hero-date"
                        style={reduced ? {} : { x: dateTx, y: dateTy }}
                        variants={fadeUp}
                    >
                        {dateLabel}
                    </motion.p>
                )}

                <motion.h1
                    className="fndr-hero-greeting"
                    style={reduced ? {} : { x: titleTx, y: titleTy }}
                    variants={fadeUp}
                >
                    {greeting}
                </motion.h1>

                <motion.p
                    className="fndr-hero-subtitle"
                    style={reduced ? {} : { x: subTx, y: subTy }}
                    variants={fadeUp}
                >
                    {subtitle}
                </motion.p>

                <motion.div
                    className="fndr-hero-search-wrap"
                    style={reduced ? {} : { x: searchTx, y: searchTy }}
                    variants={fadeUp}
                >
                    <HeroSearchBar onSubmit={handleSearchSubmit} />
                </motion.div>
            </motion.div>

            {/* Scroll indicator */}
            <motion.div
                style={reduced ? {} : { x: scrollTx, y: scrollTy }}
                className="fndr-hero-scroll-hint"
            >
                <span className="fndr-hero-scroll-label">SCROLL TO EXPLORE</span>
                <div className="fndr-hero-scroll-line">
                    <div className="fndr-hero-scroll-dot" />
                </div>
            </motion.div>
        </section>
    );
}

// ── HeroSearchBar ──────────────────────────────────────────────────────────

function HeroSearchBar({ onSubmit }: { onSubmit: (q: string) => void }) {
    const [query, setQuery] = useState("");
    const { isRecording, isTranscribing, voiceStatus, handleVoiceToggle } = useHeroVoice((text) => {
        setQuery(text);
    });

    const handleSubmit = (e: React.FormEvent) => {
        e.preventDefault();
        if (query.trim()) onSubmit(query.trim());
    };

    const placeholder = "What shall we uncover tonight?";

    return (
        <form className="fndr-hero-search" onSubmit={handleSubmit} data-aurora-ignore>
            <span className="fndr-hero-search-icon" aria-hidden="true">
                <svg
                    viewBox="0 0 20 20"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                >
                    <circle cx="8.5" cy="8.5" r="4.5" />
                    <line x1="13" y1="13" x2="17" y2="17" />
                </svg>
            </span>
            <input
                className="fndr-hero-search-input"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={placeholder}
                spellCheck={false}
                autoComplete="off"
            />
            <span className="fndr-hero-voice-divider" aria-hidden="true" />
            <button
                type="button"
                className={`fndr-hero-voice-btn${isRecording ? " is-listening" : ""}`}
                onClick={() => void handleVoiceToggle()}
                aria-label={isRecording ? "Stop voice recording" : "Start voice recording"}
                title={voiceStatus ?? (isRecording ? "Stop voice recording" : "Start voice recording")}
                disabled={isTranscribing}
                data-aurora-ignore
            >
                {/* Three vertical bars — audio waveform */}
                <svg
                    viewBox="0 0 14 14"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                >
                    <line x1="2" y1="5" x2="2" y2="9" />
                    <line x1="7" y1="2" x2="7" y2="12" />
                    <line x1="12" y1="5" x2="12" y2="9" />
                </svg>
                <span>{isRecording ? "Stop" : isTranscribing ? "..." : "Speak"}</span>
            </button>
            <button
                type="submit"
                className="fndr-hero-search-submit"
                aria-label="Search"
                data-aurora-ignore
                disabled={!query.trim()}
            >
                <svg
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.8"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                >
                    <line x1="5" y1="12" x2="19" y2="12" />
                    <polyline points="13 6 19 12 13 18" />
                </svg>
            </button>
        </form>
    );
}

function useHeroVoice(onTranscript: (text: string) => void) {
    const [voiceStatus, setVoiceStatus] = useState<string | null>(null);
    const [isRecording, setIsRecording] = useState(false);
    const [isTranscribing, setIsTranscribing] = useState(false);
    const mediaRecorderRef = useRef<MediaRecorder | null>(null);
    const mediaStreamRef = useRef<MediaStream | null>(null);
    const audioChunksRef = useRef<Blob[]>([]);
    const mimeTypeRef = useRef("audio/webm");
    const recordingStartedAtRef = useRef(0);

    useEffect(() => {
        return () => {
            if (mediaRecorderRef.current && mediaRecorderRef.current.state !== "inactive") {
                mediaRecorderRef.current.stop();
            }
            stopMediaStream(mediaStreamRef.current);
        };
    }, []);

    async function handleVoiceToggle() {
        if (isRecording) {
            if (mediaRecorderRef.current && mediaRecorderRef.current.state !== "inactive") {
                mediaRecorderRef.current.stop();
            }
            return;
        }

        if (!navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
            setVoiceStatus("Voice capture is not supported in this build.");
            return;
        }

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
                if (durationMs < VOICE_RECORDING.minDurationMs) {
                    setVoiceStatus("Hold the mic a bit longer and try again.");
                    return;
                }
                void transcribeRecordedVoice(chunks, mimeTypeRef.current);
            };

            recorder.start(VOICE_RECORDING.timesliceMs);
            setIsRecording(true);
            setVoiceStatus("Listening... tap again to stop.");
        } catch (err) {
            console.error("Hero voice capture failed:", err);
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
            const cleaned = result.text.trim();
            if (cleaned) {
                onTranscript(cleaned);
                setVoiceStatus("Transcript added.");
                window.setTimeout(() => setVoiceStatus(null), VOICE_RECORDING.statusClearMs);
            } else {
                setVoiceStatus("No voice input captured.");
            }
        } catch (err) {
            console.error("Hero voice transcription failed:", err);
            setVoiceStatus(`Voice transcription failed: ${String(err)}`);
        } finally {
            setIsTranscribing(false);
        }
    }

    return { isRecording, isTranscribing, voiceStatus, handleVoiceToggle };
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

export default HeroSection;
