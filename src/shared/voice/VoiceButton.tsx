import { useReducedMotionSafe } from "@/shared/motion/useReducedMotionSafe";
import type { VoiceMode, VoiceState } from "./useVoice";
import "./voice.css";

interface VoiceButtonProps {
    mode: VoiceMode;
    state: VoiceState;
    isActive: boolean;
    disabled?: boolean;
    onStart: () => void | Promise<void>;
    onStop: () => void | Promise<void>;
    className?: string;
}

function isBusy(state: VoiceState): boolean {
    return state.kind === "requesting_permission" || state.kind === "preparing_model";
}

export function VoiceButton({
    mode,
    state,
    isActive,
    disabled = false,
    onStart,
    onStop,
    className,
}: VoiceButtonProps) {
    const { reduced } = useReducedMotionSafe();
    const pushToTalk = mode === "push_to_talk";
    const label = pushToTalk ? "Hold to speak" : isActive ? "Stop voice input" : "Start voice input";

    const begin = () => {
        if (!disabled) void onStart();
    };
    const end = () => {
        if (!disabled) void onStop();
    };

    return (
        <button
            type="button"
            className={["fndr-voice-button", isActive ? "fndr-voice-button--active" : "", className ?? ""]
                .filter(Boolean)
                .join(" ")}
            aria-label={label}
            aria-pressed={pushToTalk ? undefined : isActive}
            data-state={state.kind}
            data-reduced-motion={reduced ? "true" : "false"}
            disabled={disabled}
            onClick={pushToTalk ? undefined : isActive ? end : begin}
            onPointerDown={pushToTalk ? begin : undefined}
            onPointerUp={pushToTalk ? end : undefined}
            onPointerCancel={pushToTalk ? end : undefined}
            onKeyDown={
                pushToTalk
                    ? (event) => {
                          if ((event.key === " " || event.key === "Enter") && !event.repeat) {
                              event.preventDefault();
                              begin();
                          }
                      }
                    : undefined
            }
            onKeyUp={
                pushToTalk
                    ? (event) => {
                          if (event.key === " " || event.key === "Enter") {
                              event.preventDefault();
                              end();
                          }
                      }
                    : undefined
            }
        >
            <span className="fndr-voice-button__icon" aria-hidden="true">
                {isActive ? "■" : "●"}
            </span>
            <span>{pushToTalk ? "Hold to speak" : isActive ? "Stop" : isBusy(state) ? "Starting…" : "Speak"}</span>
        </button>
    );
}
