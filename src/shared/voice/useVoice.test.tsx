import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useVoice, type VoiceStateEvent } from "./useVoice";

const mocks = vi.hoisted(() => ({
    invoke: vi.fn(),
    eventHandler: null as ((payload: VoiceStateEvent) => void) | null,
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@/shared/hooks/useTauriEvent", () => ({
    useTauriEvent: (_event: string, handler: (payload: VoiceStateEvent) => void) => {
        mocks.eventHandler = handler;
    },
}));

function Harness({
    onPartial = vi.fn(),
    onFinal = vi.fn(),
}: {
    onPartial?: (text: string) => void;
    onFinal?: (text: string) => void;
}) {
    const voice = useVoice({
        surface: "home_search",
        mode: "toggle",
        onPartial,
        onFinal,
    });

    return (
        <div>
            <output data-testid="state">{voice.state.kind}</output>
            <output data-testid="level">{voice.level}</output>
            <output data-testid="session">{voice.sessionId ?? "none"}</output>
            <button onClick={() => void voice.start()}>start</button>
            <button onClick={() => void voice.stop()}>stop</button>
            <button onClick={() => void voice.cancel()}>cancel</button>
        </div>
    );
}

describe("useVoice", () => {
    beforeEach(() => {
        mocks.invoke.mockReset();
        mocks.eventHandler = null;
    });

    afterEach(cleanup);

    it("owns a session and forwards matching partial and final text", async () => {
        const onPartial = vi.fn();
        const onFinal = vi.fn();
        mocks.invoke.mockResolvedValueOnce({ sessionId: "session-1" });

        render(<Harness onPartial={onPartial} onFinal={onFinal} />);
        fireEvent.click(screen.getByRole("button", { name: "start" }));

        await waitFor(() =>
            expect(mocks.invoke).toHaveBeenCalledWith("voice_start", {
                surface: "home_search",
                mode: "toggle",
            }),
        );

        act(() => {
            mocks.eventHandler?.({
                version: 1,
                sessionId: "another-session",
                surface: "home_search",
                state: { kind: "partial", text: "ignore me" },
            });
            mocks.eventHandler?.({
                version: 1,
                sessionId: "session-1",
                surface: "home_search",
                state: { kind: "partial", text: "show my" },
            });
            mocks.eventHandler?.({
                version: 1,
                sessionId: "session-1",
                surface: "home_search",
                state: { kind: "final", text: "show my meetings" },
            });
        });

        expect(onPartial).toHaveBeenCalledOnce();
        expect(onPartial).toHaveBeenCalledWith("show my");
        expect(onFinal).toHaveBeenCalledWith("show my meetings");
        expect(screen.getByTestId("state")).toHaveTextContent("final");

        act(() => {
            mocks.eventHandler?.({
                version: 1,
                sessionId: "session-1",
                surface: "home_search",
                state: { kind: "idle" },
            });
        });

        expect(screen.getByTestId("state")).toHaveTextContent("final");
        expect(screen.getByTestId("session")).toHaveTextContent("none");
    });

    it("retains the latest audio level while partial text arrives", async () => {
        mocks.invoke.mockResolvedValueOnce({ sessionId: "session-2" });
        render(<Harness />);
        fireEvent.click(screen.getByRole("button", { name: "start" }));
        await waitFor(() => expect(mocks.invoke).toHaveBeenCalled());

        act(() => {
            mocks.eventHandler?.({
                version: 1,
                sessionId: "session-2",
                surface: "home_search",
                state: { kind: "listening", level: 0.42 },
            });
            mocks.eventHandler?.({
                version: 1,
                sessionId: "session-2",
                surface: "home_search",
                state: { kind: "partial", text: "find notes" },
            });
        });

        expect(screen.getByTestId("level")).toHaveTextContent("0.42");
    });

    it("sends stop and cancel for the active session", async () => {
        mocks.invoke
            .mockResolvedValueOnce({ sessionId: "session-3" })
            .mockResolvedValue(undefined);
        render(<Harness />);
        fireEvent.click(screen.getByRole("button", { name: "start" }));
        await waitFor(() => expect(mocks.invoke).toHaveBeenCalledTimes(1));

        fireEvent.click(screen.getByRole("button", { name: "stop" }));
        fireEvent.click(screen.getByRole("button", { name: "cancel" }));

        await waitFor(() => {
            expect(mocks.invoke).toHaveBeenCalledWith("voice_stop", { sessionId: "session-3" });
            expect(mocks.invoke).toHaveBeenCalledWith("voice_cancel", { sessionId: "session-3" });
        });
    });
});
