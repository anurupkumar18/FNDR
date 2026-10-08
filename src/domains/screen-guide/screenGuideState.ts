import type {
    ScreenGuideActivityStage,
    ScreenGuideAnswer,
    ScreenGuideHistoryEntry,
    ScreenGuidePhase,
    ScreenGuidePointCue,
} from "@/shared/ipc/tauri";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceSnapshot,
    type ActivityTraceStatus,
} from "@/shared/activity/activityTrace";
import { VOICE_RECORDING } from "@/shared/utils/config";

export interface ScreenGuideExchange {
    question: string;
    answer: string;
}

export interface ScreenGuideOverlayState {
    phase: ScreenGuidePhase;
    question: string | null;
    answer: string | null;
    message: string | null;
    pointCue: ScreenGuidePointCue | null;
    exchanges: ScreenGuideExchange[];
}

export type ScreenGuideAction =
    | { type: "idle" }
    | { type: "listening" }
    | { type: "transcribing" }
    | { type: "thinking"; question: string }
    | { type: "answered"; question: string; response: ScreenGuideAnswer }
    | { type: "failed"; message: string };

export const initialScreenGuideState: ScreenGuideOverlayState = {
    phase: "idle",
    question: null,
    answer: null,
    message: null,
    pointCue: null,
    exchanges: [],
};

export interface ScreenGuideActivity {
    stage: ScreenGuideActivityStage;
    targetApp: string | null;
    generation: number;
    atMs: number;
    failedAtMs?: number;
}

export type ScreenGuideFrontendActivityStage =
    | "microphone_access"
    | "voice_recording"
    | "voice_transcription";

interface ScreenGuideFrontendActivity {
    stage: ScreenGuideFrontendActivityStage;
    generation: number;
    status: Extract<ActivityTraceStatus, "running" | "completed">;
    atMs: number;
}

const FRONTEND_ACTIVITY_COPY: Record<ScreenGuideFrontendActivityStage, {
    active: string;
    completed: string;
    actor: string;
}> = {
    microphone_access: {
        active: "Requesting microphone access",
        completed: "Microphone connected",
        actor: "macOS microphone",
    },
    voice_recording: {
        active: "Recording voice input",
        completed: "Recording stopped",
        actor: "Screen Guide recorder",
    },
    voice_transcription: {
        active: "Transcribing voice input",
        completed: "Transcription ready",
        actor: "Local speech model",
    },
};

const ACTIVITY_COPY: Record<ScreenGuideActivityStage, {
    active: string;
    failed: string;
}> = {
    preparing: {
        active: "Preparing this turn",
        failed: "preparing this turn",
    },
    searching_file_names: {
        active: "Searching allowed file names",
        failed: "searching allowed file names",
    },
    hiding_fndr: {
        active: "Hiding FNDR from the capture",
        failed: "hiding FNDR from the capture",
    },
    verifying_target: {
        active: "Verifying the target window",
        failed: "verifying the target window",
    },
    capturing: {
        active: "Capturing the main display",
        failed: "capturing the main display",
    },
    reading_text: {
        active: "Reading visible text",
        failed: "reading visible text",
    },
    checking_on_device_model: {
        active: "Checking on-device model availability",
        failed: "checking on-device model availability",
    },
    answering_on_device: {
        active: "Answering with the on-device model",
        failed: "answering with the on-device model",
    },
    using_grounded_fallback: {
        active: "Using OCR-grounded fallback",
        failed: "using OCR-grounded fallback",
    },
    answering_chat_gpt: {
        active: "Answering with ChatGPT",
        failed: "answering with ChatGPT",
    },
    speech_started: {
        active: "Started macOS speech",
        failed: "starting macOS speech",
    },
};

export function screenGuideActivityCopy(
    activity: Pick<ScreenGuideActivity, "stage" | "targetApp">,
    failed = false,
): string {
    const copy = ACTIVITY_COPY[activity.stage];
    const base = failed ? copy.failed : copy.active;
    return failed ? `Stopped while ${base}` : base;
}

const ACTIVITY_ACTOR: Record<ScreenGuideActivityStage, string> = {
    preparing: "FNDR coordinator",
    searching_file_names: "macOS Spotlight",
    hiding_fndr: "FNDR window manager",
    verifying_target: "macOS Accessibility",
    capturing: "macOS Screen Recording",
    reading_text: "Apple Vision OCR",
    checking_on_device_model: "FNDR model runtime",
    answering_on_device: "FNDR on-device model",
    using_grounded_fallback: "FNDR grounding",
    answering_chat_gpt: "ChatGPT",
    speech_started: "macOS speech",
};

export function screenGuideActivityTrace(
    activity: ScreenGuideActivity,
    phase: ScreenGuidePhase,
): ActivityTraceSnapshot {
    const trace = recordScreenGuideActivity(null, activity);
    return phase === "error"
        ? finishScreenGuideActivity(trace, "failed", activity.failedAtMs ?? activity.atMs) ?? trace
        : trace;
}

/** Append only the native stages that were actually emitted for this turn.
 * A later stage is also evidence that the previous stage finished. */
export function recordScreenGuideActivity(
    current: ActivityTraceSnapshot | null,
    activity: ScreenGuideActivity,
): ActivityTraceSnapshot {
    const traceId = `screen-guide-${activity.generation}`;
    let trace = current?.id === traceId
        ? current
        : beginActivityTrace({
            id: traceId,
            title: "Screen Guide activity",
            startedAtMs: activity.atMs,
        });
    const latest = trace.steps[trace.steps.length - 1];

    if (latest && latest.id !== activity.stage && latest.status === "running") {
        trace = recordActivityStep(trace, {
            ...latest,
            status: "completed",
            atMs: activity.atMs,
            durationMs: Math.max(0, activity.atMs - latest.atMs),
        });
    }

    const repeated = trace.steps.find((step) => step.id === activity.stage);
    return recordActivityStep(trace, {
        id: activity.stage,
        label: screenGuideActivityCopy(activity),
        actor: ACTIVITY_ACTOR[activity.stage],
        // This event is emitted only after `/usr/bin/say` accepts the text.
        // It records a completed launch boundary, not inferred speech lifetime.
        status: activity.stage === "speech_started" ? "completed" : "running",
        evidence: "backend-event",
        atMs: repeated?.status === "running" ? repeated.atMs : activity.atMs,
    });
}

/** Record only browser/runtime boundaries observed before the native Screen
 * Guide request begins. The trace intentionally excludes audio and transcript
 * contents. */
export function recordScreenGuideFrontendActivity(
    current: ActivityTraceSnapshot | null,
    activity: ScreenGuideFrontendActivity,
): ActivityTraceSnapshot {
    const traceId = `screen-guide-${activity.generation}`;
    let trace = current?.id === traceId
        ? current
        : beginActivityTrace({
            id: traceId,
            title: "Screen Guide activity",
            startedAtMs: activity.atMs,
        });
    const latest = trace.steps[trace.steps.length - 1];
    if (latest && latest.id !== activity.stage && latest.status === "running") {
        trace = recordActivityStep(trace, {
            ...latest,
            status: "completed",
            atMs: activity.atMs,
            durationMs: Math.max(0, activity.atMs - latest.atMs),
        });
    }

    const copy = FRONTEND_ACTIVITY_COPY[activity.stage];
    const previous = trace.steps.find((step) => step.id === activity.stage);
    return recordActivityStep(trace, {
        id: activity.stage,
        label: activity.status === "completed" ? copy.completed : copy.active,
        actor: copy.actor,
        status: activity.status,
        evidence: activity.stage === "voice_transcription"
            ? (activity.status === "completed" ? "result-metadata" : "ipc-boundary")
            : "frontend-event",
        atMs: activity.atMs,
        ...(previous
            ? { durationMs: Math.max(0, activity.atMs - previous.atMs) }
            : {}),
    });
}

/** Mark the last emitted native stage terminal without inventing another
 * stage. Earlier observed stages remain available in Details. */
export function finishScreenGuideActivity(
    current: ActivityTraceSnapshot | null,
    status: Extract<ActivityTraceStatus, "completed" | "failed" | "cancelled">,
    atMs: number,
): ActivityTraceSnapshot | null {
    const latest = current?.steps[current.steps.length - 1];
    if (!current || !latest) return current;
    if (latest.status === status) return current;
    const label = status === "failed"
        ? `Stopped while ${latest.label.charAt(0).toLocaleLowerCase()}${latest.label.slice(1)}`
        : latest.label;
    return recordActivityStep(current, {
        ...latest,
        label,
        status,
        atMs,
        durationMs: Math.max(0, atMs - latest.atMs),
    });
}

const MAX_EXCHANGES = 10;

export function screenGuideReducer(
    state: ScreenGuideOverlayState,
    action: ScreenGuideAction,
): ScreenGuideOverlayState {
    switch (action.type) {
        case "idle":
            return {
                ...state,
                phase: "idle",
                question: null,
                answer: null,
                message: null,
                pointCue: null,
            };
        case "listening":
            return {
                ...state,
                phase: "listening",
                question: null,
                answer: null,
                message: "Listening…",
                pointCue: null,
            };
        case "transcribing":
            return {
                ...state,
                phase: "transcribing",
                message: "Transcribing on this Mac…",
                pointCue: null,
            };
        case "thinking":
            return {
                ...state,
                phase: "thinking",
                question: action.question,
                answer: null,
                message: "Finding the answer on this Mac…",
                pointCue: null,
            };
        case "answered": {
            const exchanges = [
                ...state.exchanges,
                { question: action.question, answer: action.response.answer },
            ].slice(-MAX_EXCHANGES);
            return {
                ...state,
                phase: "answer",
                question: action.question,
                answer: action.response.answer,
                message: null,
                pointCue: action.response.point_cue,
                exchanges,
            };
        }
        case "failed":
            return {
                ...state,
                phase: "error",
                answer: null,
                message: action.message,
                pointCue: null,
            };
    }
}

export function toScreenGuideHistory(
    exchanges: ScreenGuideExchange[],
): ScreenGuideHistoryEntry[] {
    return exchanges.flatMap(({ question, answer }) => [
        { role: "user" as const, content: question },
        { role: "assistant" as const, content: answer },
    ]);
}

export function isLongEnoughVoiceClip(startedAtMs: number, endedAtMs: number): boolean {
    return endedAtMs - startedAtMs >= VOICE_RECORDING.minDurationMs;
}

export function screenGuideErrorMessage(reason: unknown, fallback: string): string {
    if (reason instanceof Error && reason.message.trim()) return reason.message;
    if (typeof reason === "string" && reason.trim()) return reason;
    return fallback;
}
