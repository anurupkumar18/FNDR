/**
 * Push-to-talk capture for the notch, on the same path the search bar and
 * Screen Guide use: record in the webview, hand the bytes to Whisper in Rust.
 *
 * Kept as a plain controller rather than a hook so the panel can start and
 * stop it from event handlers, and so the "is this build even capable of
 * recording" check can run before the mic button is rendered.
 */

import { VOICE_RECORDING } from "@/shared/utils/config";

export interface VoiceClip {
    audioBytes: number[];
    mimeType: string;
    durationMs: number;
}

export function isVoiceCaptureAvailable(): boolean {
    return (
        typeof navigator !== "undefined" &&
        Boolean(navigator.mediaDevices?.getUserMedia) &&
        typeof MediaRecorder !== "undefined"
    );
}

function recorderOptions(): MediaRecorderOptions | undefined {
    const candidates = [
        "audio/webm;codecs=opus",
        "audio/mp4",
        "audio/ogg;codecs=opus",
        "audio/webm",
    ];
    for (const mimeType of candidates) {
        if (MediaRecorder.isTypeSupported(mimeType)) {
            return { mimeType, audioBitsPerSecond: VOICE_RECORDING.audioBitsPerSecond };
        }
    }
    return undefined;
}

/**
 * One recording session. `stop()` resolves with the clip, or `null` when the
 * press was too short to be anything but an accident.
 */
export class VoiceCapture {
    private recorder: MediaRecorder | null = null;
    private stream: MediaStream | null = null;
    private chunks: Blob[] = [];
    private startedAt = 0;
    private settle: ((clip: VoiceClip | null) => void) | null = null;

    get isRecording(): boolean {
        return this.recorder !== null;
    }

    /** Live input, for anything that visualises the voice while it records. */
    get mediaStream(): MediaStream | null {
        return this.stream;
    }

    async start(): Promise<void> {
        if (this.recorder) {
            return;
        }
        const stream = await navigator.mediaDevices.getUserMedia({
            audio: {
                echoCancellation: true,
                noiseSuppression: true,
                autoGainControl: true,
                channelCount: VOICE_RECORDING.channelCount,
                sampleRate: VOICE_RECORDING.sampleRate,
            },
        });
        const options = recorderOptions();
        const recorder = options ? new MediaRecorder(stream, options) : new MediaRecorder(stream);

        this.stream = stream;
        this.recorder = recorder;
        this.chunks = [];
        this.startedAt = Date.now();

        recorder.ondataavailable = (event) => {
            if (event.data.size > 0) {
                this.chunks.push(event.data);
            }
        };
        recorder.onstop = () => {
            void this.finish(recorder.mimeType || options?.mimeType || "audio/webm");
        };
        recorder.start(VOICE_RECORDING.timesliceMs);
    }

    /** Resolves once the recorder has flushed its last chunk. */
    stop(): Promise<VoiceClip | null> {
        if (!this.recorder) {
            return Promise.resolve(null);
        }
        const pending = new Promise<VoiceClip | null>((resolve) => {
            this.settle = resolve;
        });
        this.recorder.stop();
        return pending;
    }

    /** Drop the session without transcribing — unmounting, or a cancelled press. */
    cancel(): void {
        this.settle = null;
        this.recorder?.stop();
        this.releaseStream();
        this.recorder = null;
        this.chunks = [];
    }

    private async finish(mimeType: string): Promise<void> {
        const chunks = [...this.chunks];
        const durationMs = Date.now() - this.startedAt;
        const settle = this.settle;

        this.chunks = [];
        this.recorder = null;
        this.settle = null;
        this.releaseStream();

        if (!settle) {
            return;
        }
        if (chunks.length === 0 || durationMs < VOICE_RECORDING.minDurationMs) {
            settle(null);
            return;
        }
        const blob = new Blob(chunks, { type: mimeType });
        settle({
            audioBytes: Array.from(new Uint8Array(await blob.arrayBuffer())),
            mimeType,
            durationMs,
        });
    }

    private releaseStream(): void {
        this.stream?.getTracks().forEach((track) => track.stop());
        this.stream = null;
    }
}
