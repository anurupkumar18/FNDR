import { openSystemSettings } from "@/shared/ipc/onboarding";
import type { VoiceSettingsPane, VoiceState } from "./useVoice";
import "./voice.css";

interface VoiceStatusProps {
    state: VoiceState;
    level: number;
    onRetry?: () => void | Promise<void>;
    onCancel?: () => void | Promise<void>;
    onOpenSettings?: (pane: VoiceSettingsPane) => void | Promise<void>;
}

function permissionLabel(permission: "microphone" | "speech_recognition"): string {
    return permission === "microphone" ? "microphone" : "Speech Recognition";
}

function stateMessage(state: VoiceState): string {
    switch (state.kind) {
        case "idle":
            return "Voice ready";
        case "requesting_permission":
            return `Requesting ${permissionLabel(state.permission)} access`;
        case "preparing_model":
            return "Preparing on-device speech recognition";
        case "listening":
            return "Listening";
        case "partial":
            return `Hearing: ${state.text}`;
        case "final":
            return `Transcript ready: ${state.text}`;
        case "error":
        case "unavailable":
            return state.message;
    }
}

export function VoiceStatus({
    state,
    level,
    onRetry,
    onCancel,
    onOpenSettings = openSystemSettings,
}: VoiceStatusProps) {
    const message = stateMessage(state);
    const percent = Math.round(Math.max(0, Math.min(1, level)) * 100);
    const isFailure = state.kind === "error" || state.kind === "unavailable";

    return (
        <section className="fndr-voice-status" data-state={state.kind} aria-live="polite" role="status">
            <div className="fndr-voice-status__message" role={isFailure ? "alert" : undefined}>
                {message}
            </div>

            {(state.kind === "listening" || state.kind === "partial") && (
                <div
                    className="fndr-voice-status__meter"
                    role="progressbar"
                    aria-label="Microphone input level"
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-valuenow={percent}
                >
                    <span style={{ width: `${percent}%` }} />
                </div>
            )}

            <div className="fndr-voice-status__actions">
                {state.kind === "error" && onRetry && (
                    <button type="button" onClick={() => void onRetry()}>
                        Try voice again
                    </button>
                )}
                {state.kind === "unavailable" && state.settingsPane && (
                    <button type="button" onClick={() => void onOpenSettings(state.settingsPane!)}>
                        Open {state.settingsPane === "microphone" ? "Microphone" : "Speech Recognition"} settings
                    </button>
                )}
                {(state.kind === "listening" || state.kind === "partial") && onCancel && (
                    <button type="button" onClick={() => void onCancel()}>
                        Cancel voice input
                    </button>
                )}
            </div>
        </section>
    );
}
