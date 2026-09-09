import type {
    ScreenGuideAnswer,
    ScreenGuideHistoryEntry,
    ScreenGuidePhase,
    ScreenGuidePointCue,
} from "@/shared/ipc/tauri";
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
