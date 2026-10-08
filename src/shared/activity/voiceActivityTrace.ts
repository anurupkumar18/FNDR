import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
    type ActivityTraceStep,
} from "./activityTrace";

/** Closed vocabulary for voice telemetry. Keeping copy here makes it hard for
 * callers to accidentally place transcript text, audio, paths, or raw errors
 * in the trace. */
export type VoiceActivityEvent =
    | "microphone-requested"
    | "microphone-connected"
    | "microphone-unavailable"
    | "microphone-failed"
    | "recording-started"
    | "recorder-stop-requested"
    | "recording-stopped"
    | "recording-too-short"
    | "no-audio"
    | "transcription-requested"
    | "transcript-ready"
    | "no-speech"
    | "transcription-failed";

type VoiceActivityDefinition = Pick<
    ActivityTraceStep,
    "id" | "label" | "actor" | "status" | "evidence"
>;

const VOICE_ACTIVITY: Record<VoiceActivityEvent, VoiceActivityDefinition> = {
    "microphone-requested": {
        id: "microphone-request",
        label: "Requesting microphone access",
        actor: "Microphone",
        status: "running",
        evidence: "frontend-event",
    },
    "microphone-connected": {
        id: "microphone-result",
        label: "Microphone connected",
        actor: "Microphone",
        status: "completed",
        evidence: "result-metadata",
    },
    "microphone-unavailable": {
        id: "microphone-result",
        label: "Voice capture unavailable",
        actor: "Microphone",
        status: "failed",
        evidence: "frontend-event",
    },
    "microphone-failed": {
        id: "microphone-result",
        label: "Microphone access failed",
        actor: "Microphone",
        status: "failed",
        evidence: "frontend-event",
    },
    "recording-started": {
        id: "recording",
        label: "Recording voice input",
        actor: "Media recorder",
        status: "running",
        evidence: "frontend-event",
    },
    "recorder-stop-requested": {
        id: "recorder-stop-request",
        label: "Stopping voice recording",
        actor: "Media recorder",
        status: "running",
        evidence: "frontend-event",
    },
    "recording-stopped": {
        id: "recorder-stopped",
        label: "Recording stopped",
        actor: "Media recorder",
        status: "completed",
        evidence: "frontend-event",
    },
    "recording-too-short": {
        id: "recording-result",
        label: "Recording was too short",
        actor: "Media recorder",
        status: "degraded",
        evidence: "frontend-event",
    },
    "no-audio": {
        id: "transcription-result",
        label: "No audio was captured",
        actor: "Media recorder",
        status: "degraded",
        evidence: "frontend-event",
    },
    "transcription-requested": {
        id: "transcription-request",
        label: "Transcribing on this Mac",
        actor: "Local Whisper",
        status: "running",
        evidence: "ipc-boundary",
    },
    "transcript-ready": {
        id: "transcription-result",
        label: "Transcript ready",
        actor: "Local Whisper",
        status: "completed",
        evidence: "result-metadata",
    },
    "no-speech": {
        id: "transcription-result",
        label: "No speech detected",
        actor: "Local Whisper",
        status: "degraded",
        evidence: "result-metadata",
    },
    "transcription-failed": {
        id: "transcription-result",
        label: "Transcription failed",
        actor: "Local Whisper",
        status: "failed",
        evidence: "ipc-boundary",
    },
};

export function beginVoiceActivityTrace(scope: string, atMs: number): ActivityTraceSnapshot {
    return recordVoiceActivityStep(
        beginActivityTrace({
            id: `${scope}-voice-${atMs}`,
            title: "Voice input activity",
            startedAtMs: atMs,
        }),
        "microphone-requested",
        atMs,
    );
}

export function recordVoiceActivityStep(
    trace: ActivityTraceSnapshot,
    event: VoiceActivityEvent,
    atMs: number,
    durationMs?: number,
): ActivityTraceSnapshot {
    let next = trace;
    const settle = (id: string, status: ActivityTraceStep["status"]) => {
        const step = next.steps.find((candidate) => candidate.id === id);
        if (!step || (step.status !== "running" && step.status !== "waiting")) return;
        next = recordActivityStep(next, {
            ...step,
            status,
            durationMs: step.durationMs ?? Math.max(0, atMs - step.atMs),
        });
    };

    switch (event) {
        case "microphone-connected":
            settle("microphone-request", "completed");
            break;
        case "microphone-unavailable":
        case "microphone-failed":
            settle("microphone-request", "failed");
            break;
        case "recording-stopped":
        case "recording-too-short":
        case "no-audio":
            settle("recording", "completed");
            settle("recorder-stop-request", "completed");
            break;
        case "transcript-ready":
            settle("transcription-request", "completed");
            break;
        case "no-speech":
            settle("transcription-request", "degraded");
            break;
        case "transcription-failed":
            settle("transcription-request", "failed");
            break;
        default:
            break;
    }

    return recordActivityStep(next, {
        ...VOICE_ACTIVITY[event],
        atMs,
        ...(durationMs === undefined ? {} : { durationMs }),
    });
}
