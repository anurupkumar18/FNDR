import { VoiceButton } from "@/shared/voice/VoiceButton";
import { VoiceStatus } from "@/shared/voice/VoiceStatus";
import type { VoiceState } from "@/shared/voice/useVoice";
import "./voicePreviewGallery.css";

const states: Array<{ label: string; state: VoiceState; level?: number }> = [
    { label: "Idle", state: { kind: "idle" } },
    {
        label: "Requesting permission",
        state: { kind: "requesting_permission", permission: "microphone" },
    },
    { label: "Preparing model", state: { kind: "preparing_model" } },
    { label: "Listening", state: { kind: "listening", level: 0.63 }, level: 0.63 },
    { label: "Partial transcript", state: { kind: "partial", text: "show my meetings" }, level: 0.48 },
    { label: "Final transcript", state: { kind: "final", text: "Show my meetings from today." } },
    {
        label: "Error",
        state: { kind: "error", code: "recognition_failed", message: "I couldn't recognize that speech." },
    },
    {
        label: "Unavailable",
        state: {
            kind: "unavailable",
            reason: "permission_denied",
            message: "Speech Recognition access is off. Typed input is still available.",
            permission: "speech_recognition",
            settingsPane: "speech-recognition",
        },
    },
];

export function VoicePreviewGallery() {
    return (
        <main className="voice-preview">
            <header className="voice-preview__header">
                <p>VO-06 · Shared control</p>
                <h1>Voice state preview</h1>
                <p>Static, local-only examples for UI review and acceptance evidence.</p>
                <div className="voice-preview__controls">
                    <VoiceButton
                        mode="toggle"
                        state={{ kind: "idle" }}
                        isActive={false}
                        onStart={() => {}}
                        onStop={() => {}}
                    />
                    <VoiceButton
                        mode="push_to_talk"
                        state={{ kind: "idle" }}
                        isActive={false}
                        onStart={() => {}}
                        onStop={() => {}}
                    />
                </div>
            </header>

            <div className="voice-preview__grid">
                {states.map(({ label, state, level = 0 }) => (
                    <article className="voice-preview__card" key={label}>
                        <h2>{label}</h2>
                        <VoiceStatus
                            state={state}
                            level={level}
                            onRetry={() => {}}
                            onCancel={() => {}}
                            onOpenSettings={() => {}}
                        />
                    </article>
                ))}
            </div>
        </main>
    );
}

