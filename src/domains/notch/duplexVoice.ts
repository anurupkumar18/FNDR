/**
 * A spoken back-and-forth for the notch: the microphone stays open while FNDR
 * talks, so the user can interrupt, redirect or answer at any moment.
 *
 * Hearing uses the system recognizer exposed to WebKit (streaming, on-device
 * where the Mac supports it, nothing to download). Where that isn't available
 * it falls back to energy-based turn-taking over the microphone and FNDR's own
 * transcription. Speaking uses speechSynthesis so it can be cut off the instant
 * the user starts talking.
 *
 * There is no acoustic echo cancellation for synthesized speech, so while FNDR
 * speaks anything heard that mostly repeats its own words is treated as echo.
 */

import { transcribeVoiceInput } from "@/shared/ipc/tauri";
import { VoiceCapture } from "./notchVoice";

export type UtteranceIntent =
    | { kind: "stop" }
    | { kind: "approve" }
    | { kind: "decline" }
    | { kind: "say"; text: string };

const STOP_PHRASES = ["stop", "cancel", "never mind", "nevermind", "wait", "hold on", "pause", "abort"];
const YES_PHRASES = ["yes", "yeah", "yep", "yup", "sure", "ok", "okay", "go ahead", "do it", "confirm", "allow", "sounds good", "please do", "go for it"];
const NO_PHRASES = ["no", "nope", "don't", "do not", "deny", "skip", "not that", "no thanks"];

function normalize(text: string): string {
    return text
        .toLowerCase()
        .replace(/[^\p{L}\p{N}\s']/gu, " ")
        .replace(/\s+/g, " ")
        .trim();
}

function matchesShortPhrase(normalized: string, phrases: string[]): boolean {
    // Short commands only: "stop" or "okay do it", not a sentence that merely
    // contains the word ("stop at the second tab and open settings").
    if (normalized.split(" ").length > 4) return false;
    return phrases.some((phrase) => normalized === phrase || normalized.startsWith(`${phrase} `));
}

/** What the user meant, decided locally so stop and yes/no are instant. */
export function classifyUtterance(text: string, awaitingApproval: boolean): UtteranceIntent | null {
    const normalized = normalize(text);
    if (!normalized) return null;
    if (matchesShortPhrase(normalized, STOP_PHRASES)) return { kind: "stop" };
    if (awaitingApproval) {
        if (matchesShortPhrase(normalized, NO_PHRASES)) return { kind: "decline" };
        if (matchesShortPhrase(normalized, YES_PHRASES)) return { kind: "approve" };
    }
    return { kind: "say", text: text.trim() };
}

/** Heard text that is mostly FNDR's own recent speech coming back through the mic. */
export function isEcho(heard: string, recentlySpoken: string): boolean {
    const heardWords = normalize(heard).split(" ").filter(Boolean);
    if (heardWords.length === 0) return true;
    const spoken = new Set(normalize(recentlySpoken).split(" ").filter(Boolean));
    if (spoken.size === 0) return false;
    const overlap = heardWords.filter((word) => spoken.has(word)).length;
    return overlap / heardWords.length >= 0.6;
}

// MARK: - Speaking

export class Speaker {
    private recent = "";
    private voice: SpeechSynthesisVoice | null = null;
    onSpeakingChange: ((speaking: boolean) => void) | null = null;

    get available(): boolean {
        return typeof window !== "undefined" && "speechSynthesis" in window;
    }

    get speaking(): boolean {
        return this.available && window.speechSynthesis.speaking;
    }

    /** What FNDR said in the last few seconds, for echo filtering. */
    get recentlySpoken(): string {
        return this.recent;
    }

    private pickVoice(): SpeechSynthesisVoice | null {
        if (this.voice) return this.voice;
        const voices = window.speechSynthesis.getVoices();
        const language = navigator.language.split("-")[0];
        this.voice =
            voices.find((v) => v.lang.startsWith(language) && /premium|enhanced|siri/i.test(v.name)) ??
            voices.find((v) => v.lang.startsWith(language) && v.localService) ??
            null;
        return this.voice;
    }

    speak(text: string): void {
        if (!this.available || !text.trim()) return;
        const utterance = new SpeechSynthesisUtterance(text);
        const voice = this.pickVoice();
        if (voice) utterance.voice = voice;
        utterance.rate = 1.05;
        utterance.onstart = () => this.onSpeakingChange?.(true);
        utterance.onend = () => {
            if (!window.speechSynthesis.pending) this.onSpeakingChange?.(false);
            window.setTimeout(() => {
                if (!this.speaking) this.recent = "";
            }, 1500);
        };
        this.recent = `${this.recent} ${text}`.slice(-600);
        window.speechSynthesis.speak(utterance);
    }

    /** Barge-in: stop talking immediately. */
    cancel(): void {
        if (!this.available) return;
        window.speechSynthesis.cancel();
        this.onSpeakingChange?.(false);
    }
}

// MARK: - Hearing

type RecognitionCtor = new () => SpeechRecognitionLike;

interface SpeechRecognitionLike {
    continuous: boolean;
    interimResults: boolean;
    lang: string;
    onresult: ((event: { resultIndex: number; results: ArrayLike<ArrayLike<{ transcript: string }> & { isFinal: boolean }> }) => void) | null;
    onend: (() => void) | null;
    onerror: ((event: { error: string }) => void) | null;
    start(): void;
    stop(): void;
    abort(): void;
}

function recognitionCtor(): RecognitionCtor | null {
    if (typeof window === "undefined") return null;
    const w = window as unknown as { SpeechRecognition?: RecognitionCtor; webkitSpeechRecognition?: RecognitionCtor };
    return w.SpeechRecognition ?? w.webkitSpeechRecognition ?? null;
}

export interface ListenerCallbacks {
    /** A complete thing the user said. */
    onUtterance: (text: string) => void;
    /** The user started talking over FNDR. */
    onBargeIn: () => void;
    /** Live partial transcript, for display. */
    onPartial?: (text: string) => void;
    onError?: (message: string) => void;
}

/** Silence that ends a turn in the energy-based fallback. */
const FALLBACK_END_OF_TURN_MS = 800;
/** RMS level treated as speech; higher while FNDR talks so its own voice doesn't count. */
const FALLBACK_SPEECH_LEVEL = 0.02;
const FALLBACK_BARGE_IN_LEVEL = 0.06;

export class DuplexListener {
    private recognition: SpeechRecognitionLike | null = null;
    private running = false;
    private fallback: { meter: MediaStream; segment: VoiceCapture; audio: AudioContext; timer: number } | null = null;

    constructor(
        private readonly speaker: Speaker,
        private readonly callbacks: ListenerCallbacks,
    ) {}

    get usesSystemRecognizer(): boolean {
        return recognitionCtor() !== null;
    }

    /** The live microphone stream when the fallback owns one (for visuals). */
    get mediaStream(): MediaStream | null {
        return this.fallback?.meter ?? null;
    }

    async start(): Promise<void> {
        if (this.running) return;
        this.running = true;
        const Ctor = recognitionCtor();
        if (Ctor) {
            this.startRecognition(Ctor);
        } else {
            await this.startFallback();
        }
    }

    stop(): void {
        this.running = false;
        this.recognition?.abort();
        this.recognition = null;
        if (this.fallback) {
            window.clearInterval(this.fallback.timer);
            void this.fallback.audio.close();
            this.fallback.meter.getTracks().forEach((track) => track.stop());
            void this.fallback.segment.stop().catch(() => undefined);
            this.fallback = null;
        }
    }

    private heard(text: string, final: boolean): void {
        const transcript = text.trim();
        if (!transcript) return;
        if (this.speaker.speaking) {
            if (isEcho(transcript, this.speaker.recentlySpoken)) return;
            this.speaker.cancel();
            this.callbacks.onBargeIn();
        }
        if (final) this.callbacks.onUtterance(transcript);
        else this.callbacks.onPartial?.(transcript);
    }

    private startRecognition(Ctor: RecognitionCtor): void {
        const recognition = new Ctor();
        recognition.continuous = true;
        recognition.interimResults = true;
        recognition.lang = navigator.language || "en-US";
        recognition.onresult = (event) => {
            for (let i = event.resultIndex; i < event.results.length; i += 1) {
                const result = event.results[i];
                this.heard(result[0]?.transcript ?? "", result.isFinal);
            }
        };
        recognition.onerror = (event) => {
            if (event.error === "no-speech" || event.error === "aborted") return;
            this.callbacks.onError?.(
                event.error === "not-allowed"
                    ? "FNDR needs microphone and speech recognition access to listen."
                    : `Listening stopped (${event.error}).`,
            );
        };
        // The recognizer ends itself after a pause; keep the conversation open.
        recognition.onend = () => {
            if (this.running && this.recognition === recognition) {
                try {
                    recognition.start();
                } catch {
                    /* already restarting */
                }
            }
        };
        this.recognition = recognition;
        recognition.start();
    }

    private async startFallback(): Promise<void> {
        // The level meter gets its own stream: each spoken segment is recorded
        // by a fresh VoiceCapture that stops (and releases its stream) when the
        // segment is sent for transcription.
        const meter = await navigator.mediaDevices.getUserMedia({
            audio: { echoCancellation: true, noiseSuppression: true, autoGainControl: true },
        });
        const audio = new AudioContext();
        const analyser = audio.createAnalyser();
        analyser.fftSize = 1024;
        audio.createMediaStreamSource(meter).connect(analyser);
        const samples = new Float32Array(analyser.fftSize);

        let segment = new VoiceCapture();
        await segment.start();
        let speaking = false;
        let lastVoiceAt = 0;

        const timer = window.setInterval(() => {
            analyser.getFloatTimeDomainData(samples);
            const rms = Math.sqrt(samples.reduce((sum, s) => sum + s * s, 0) / samples.length);
            const threshold = this.speaker.speaking ? FALLBACK_BARGE_IN_LEVEL : FALLBACK_SPEECH_LEVEL;
            const now = performance.now();
            if (rms > threshold) {
                lastVoiceAt = now;
                if (!speaking) {
                    speaking = true;
                    if (this.speaker.speaking) {
                        this.speaker.cancel();
                        this.callbacks.onBargeIn();
                    }
                }
                return;
            }
            if (speaking && now - lastVoiceAt > FALLBACK_END_OF_TURN_MS) {
                speaking = false;
                const finished = segment;
                segment = new VoiceCapture();
                if (this.fallback) this.fallback.segment = segment;
                void segment.start().catch(() => undefined);
                void finished
                    .stop()
                    .then((clip) => (clip ? transcribeVoiceInput(clip.audioBytes, clip.mimeType) : null))
                    .then((result) => result && this.heard(result.text, true))
                    .catch((reason) =>
                        this.callbacks.onError?.(reason instanceof Error ? reason.message : String(reason)),
                    );
            }
        }, 50);
        this.fallback = { meter, segment, audio, timer };
    }
}
