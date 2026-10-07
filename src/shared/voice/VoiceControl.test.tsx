import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { VoiceButton } from "./VoiceButton";
import { VoiceStatus } from "./VoiceStatus";
import type { VoiceState } from "./useVoice";

vi.mock("@/shared/motion/useReducedMotionSafe", () => ({
    useReducedMotionSafe: () => ({ reduced: false }),
}));

afterEach(cleanup);

describe("VoiceStatus", () => {
    const cases: Array<[VoiceState, string]> = [
        [{ kind: "idle" }, "Voice ready"],
        [
            { kind: "requesting_permission", permission: "microphone" },
            "Requesting microphone access",
        ],
        [{ kind: "preparing_model" }, "Preparing on-device speech recognition"],
        [{ kind: "listening", level: 0.63 }, "Listening"],
        [{ kind: "partial", text: "show my" }, "Hearing: show my"],
        [{ kind: "final", text: "show my meetings" }, "Transcript ready: show my meetings"],
    ];

    it.each(cases)("announces %#", (state, message) => {
        render(<VoiceStatus state={state} level={state.kind === "listening" ? state.level : 0} />);
        expect(screen.getByRole("status")).toHaveTextContent(message);
    });

    it("shows the live input level without animating meaning into it", () => {
        render(<VoiceStatus state={{ kind: "listening", level: 0.63 }} level={0.63} />);
        expect(screen.getByRole("progressbar", { name: "Microphone input level" })).toHaveAttribute(
            "aria-valuenow",
            "63",
        );
    });

    it("offers retry for a recoverable error", () => {
        const onRetry = vi.fn();
        render(
            <VoiceStatus
                state={{ kind: "error", code: "recognition_failed", message: "Speech was unclear." }}
                level={0}
                onRetry={onRetry}
            />,
        );
        expect(screen.getByRole("alert")).toHaveTextContent("Speech was unclear.");
        fireEvent.click(screen.getByRole("button", { name: "Try voice again" }));
        expect(onRetry).toHaveBeenCalledOnce();
    });

    it("explains unavailable voice and opens the exact settings pane", () => {
        const onOpenSettings = vi.fn();
        render(
            <VoiceStatus
                state={{
                    kind: "unavailable",
                    reason: "permission_denied",
                    message: "Speech Recognition access is off.",
                    permission: "speech_recognition",
                    settingsPane: "speech-recognition",
                }}
                level={0}
                onOpenSettings={onOpenSettings}
            />,
        );
        expect(screen.getByRole("alert")).toHaveTextContent("Speech Recognition access is off.");
        fireEvent.click(screen.getByRole("button", { name: "Open Speech Recognition settings" }));
        expect(onOpenSettings).toHaveBeenCalledWith("speech-recognition");
    });
});

describe("VoiceButton", () => {
    it("starts and stops a toggle session", () => {
        const start = vi.fn();
        const stop = vi.fn();
        const { rerender } = render(
            <VoiceButton mode="toggle" state={{ kind: "idle" }} isActive={false} onStart={start} onStop={stop} />,
        );
        fireEvent.click(screen.getByRole("button", { name: "Start voice input" }));
        expect(start).toHaveBeenCalledOnce();

        rerender(
            <VoiceButton
                mode="toggle"
                state={{ kind: "listening", level: 0.2 }}
                isActive
                onStart={start}
                onStop={stop}
            />,
        );
        fireEvent.click(screen.getByRole("button", { name: "Stop voice input" }));
        expect(stop).toHaveBeenCalledOnce();
    });

    it("supports pointer and keyboard push-to-talk", () => {
        const start = vi.fn();
        const stop = vi.fn();
        render(
            <VoiceButton
                mode="push_to_talk"
                state={{ kind: "idle" }}
                isActive={false}
                onStart={start}
                onStop={stop}
            />,
        );
        const button = screen.getByRole("button", { name: "Hold to speak" });
        fireEvent.pointerDown(button);
        fireEvent.pointerUp(button);
        fireEvent.keyDown(button, { key: " " });
        fireEvent.keyUp(button, { key: " " });
        expect(start).toHaveBeenCalledTimes(2);
        expect(stop).toHaveBeenCalledTimes(2);
    });
});
