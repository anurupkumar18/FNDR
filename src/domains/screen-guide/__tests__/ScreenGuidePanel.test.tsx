import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const mocks = vi.hoisted(() => ({
    getScreenGuideSettings: vi.fn(),
    onScreenGuideState: vi.fn(),
    screenGuidePress: vi.fn(),
    screenGuideRelease: vi.fn(),
    setScreenGuideSettings: vi.fn(),
    submitScreenGuideText: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => mocks);

import { ScreenGuidePanel } from "../ScreenGuidePanel";

describe("ScreenGuidePanel", () => {
    let stateHandler: ((status: { phase: string; message: string | null }) => void) | null;

    beforeEach(() => {
        stateHandler = null;
        mocks.getScreenGuideSettings.mockReset().mockResolvedValue({
            enabled: false,
            shortcut: "Option+Space",
            speak_responses: false,
            show_cursor: true,
        });
        mocks.onScreenGuideState.mockReset().mockImplementation(async (handler) => {
            stateHandler = handler;
            return vi.fn();
        });
        mocks.screenGuidePress.mockReset().mockResolvedValue(1);
        mocks.screenGuideRelease.mockReset().mockResolvedValue(undefined);
        mocks.setScreenGuideSettings.mockReset().mockImplementation(async (settings) => settings);
        mocks.submitScreenGuideText.mockReset().mockResolvedValue(undefined);
    });

    afterEach(() => cleanup());

    it("renders a disabled state with explicit readiness and local privacy guidance", async () => {
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(screen.getByRole("dialog", { name: "Screen Guide" })).toBeInTheDocument();
        expect(await screen.findByRole("heading", { name: "Screen Guide" })).toBeInTheDocument();
        expect(screen.getByText("Screen Guide is off")).toBeInTheDocument();
        expect(screen.getByText(/processed on this Mac for this turn/i)).toBeInTheDocument();
        expect(screen.getByText(/does not click, type, or add the turn/i)).toBeInTheDocument();
        expect(screen.getByRole("checkbox", { name: "Enable Screen Guide" })).not.toBeChecked();
        expect(screen.getByRole("textbox", { name: "Ask about your main display" })).toBeDisabled();
        expect(screen.getByRole("button", { name: "Ask Screen Guide" })).toBeDisabled();
        expect(screen.getByRole("button", { name: /hold to talk/i })).toBeDisabled();
        expect(screen.getByText(/Option\+Space/)).toBeInTheDocument();
    });

    it("moves focus into the modal and closes it with Escape", async () => {
        const onClose = vi.fn();
        render(<ScreenGuidePanel isVisible onClose={onClose} />);

        const close = screen.getByRole("button", { name: "Close Screen Guide" });
        await waitFor(() => expect(close).toHaveFocus());

        fireEvent.keyDown(close, { key: "Escape" });
        expect(onClose).toHaveBeenCalledTimes(1);
    });

    it("offers an in-panel retry when settings fail to load", async () => {
        mocks.getScreenGuideSettings
            .mockRejectedValueOnce(new Error("Settings service unavailable"))
            .mockResolvedValueOnce({
                enabled: false,
                shortcut: "Option+Space",
                speak_responses: false,
                show_cursor: true,
            });
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(await screen.findByRole("alert")).toHaveTextContent(
            "Settings service unavailable",
        );
        fireEvent.click(
            screen.getByRole("button", { name: "Retry loading Screen Guide" }),
        );

        await waitFor(() => expect(mocks.getScreenGuideSettings).toHaveBeenCalledTimes(2));
        expect(await screen.findByText("Screen Guide is off")).toBeInTheDocument();
        expect(screen.queryByText("Settings service unavailable")).not.toBeInTheDocument();
    });

    it("keeps settings usable while disclosing a lost live-status connection", async () => {
        mocks.onScreenGuideState.mockRejectedValueOnce(new Error("event bridge unavailable"));
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(
            await screen.findByText(/live activity updates are unavailable/i),
        ).toBeInTheDocument();
        expect(screen.getByRole("checkbox", { name: "Enable Screen Guide" })).toBeEnabled();
    });

    it("persists the complete settings contract when enabled", async () => {
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);
        const enable = await screen.findByRole("checkbox", { name: "Enable Screen Guide" });

        fireEvent.click(enable);

        await waitFor(() =>
            expect(mocks.setScreenGuideSettings).toHaveBeenCalledWith({
                enabled: true,
                shortcut: "Option+Space",
                speak_responses: false,
                show_cursor: true,
            }),
        );
        expect(screen.getByRole("textbox", { name: "Ask about your main display" })).toBeEnabled();
        expect(screen.getByText("Enabled")).toBeInTheDocument();
    });

    it("keeps runtime status current while the mounted panel is hidden", async () => {
        mocks.getScreenGuideSettings.mockResolvedValue({
            enabled: true,
            shortcut: "Control+Alt+Space",
            speak_responses: true,
            show_cursor: true,
        });
        const { rerender } = render(<ScreenGuidePanel isVisible onClose={() => {}} />);
        await waitFor(() => expect(stateHandler).not.toBeNull());

        act(() => stateHandler?.({ phase: "listening", message: "Listening…" }));
        expect(screen.getByText("Listening…")).toBeInTheDocument();

        rerender(<ScreenGuidePanel isVisible={false} onClose={() => {}} />);
        act(() => stateHandler?.({ phase: "idle", message: null }));
        rerender(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(await screen.findByText("Enabled")).toBeInTheDocument();
        expect(screen.queryByText("Listening…")).not.toBeInTheDocument();
        expect(mocks.onScreenGuideState).toHaveBeenCalledTimes(1);
    });

    it("surfaces shortcut conflicts and restores the registered shortcut", async () => {
        mocks.getScreenGuideSettings.mockResolvedValue({
            enabled: true,
            shortcut: "Control+Alt+Space",
            speak_responses: true,
            show_cursor: true,
        });
        mocks.setScreenGuideSettings.mockRejectedValue(
            "That shortcut is already used by Autofill.",
        );
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        const shortcut = await screen.findByRole("textbox", {
            name: "Screen Guide shortcut",
        });
        fireEvent.change(shortcut, { target: { value: "Control+Shift+Space" } });
        fireEvent.click(screen.getByRole("button", { name: "Save shortcut" }));

        await waitFor(() =>
            expect(mocks.setScreenGuideSettings).toHaveBeenCalledWith({
                enabled: true,
                shortcut: "Control+Shift+Space",
                speak_responses: true,
                show_cursor: true,
            }),
        );
        expect(await screen.findByRole("alert")).toHaveTextContent(
            "That shortcut is already used by Autofill.",
        );
        await waitFor(() => expect(shortcut).toHaveValue("Control+Alt+Space"));
    });

    it("does not let an older press rejection cancel a newer hold", async () => {
        let rejectFirst: ((reason: Error) => void) | null = null;
        mocks.getScreenGuideSettings.mockResolvedValue({
            enabled: true,
            shortcut: "Control+Alt+Space",
            speak_responses: true,
            show_cursor: true,
        });
        mocks.screenGuidePress
            .mockReturnValueOnce(new Promise((_, reject) => {
                rejectFirst = reject;
            }))
            .mockResolvedValueOnce(22);
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        const hold = await screen.findByRole("button", { name: "Hold to talk" });
        fireEvent.pointerDown(hold, { pointerId: 1 });
        fireEvent.pointerUp(hold, { pointerId: 1 });
        fireEvent.pointerDown(hold, { pointerId: 2 });
        await act(async () => {
            rejectFirst?.(new Error("older press failed"));
            await Promise.resolve();
        });

        expect(screen.getByRole("button", { name: "Release to ask" })).toHaveAttribute(
            "aria-pressed",
            "true",
        );
        fireEvent.pointerUp(screen.getByRole("button", { name: "Release to ask" }), {
            pointerId: 2,
        });
        await waitFor(() => expect(mocks.screenGuideRelease).toHaveBeenCalledWith(22));
        expect(screen.queryByText("older press failed")).not.toBeInTheDocument();
    });

    it("reopens with a clean hold state after closing mid-press", async () => {
        mocks.getScreenGuideSettings.mockResolvedValue({
            enabled: true,
            shortcut: "Control+Alt+Space",
            speak_responses: true,
            show_cursor: true,
        });
        const { rerender } = render(<ScreenGuidePanel isVisible onClose={() => {}} />);
        const hold = await screen.findByRole("button", { name: "Hold to talk" });

        fireEvent.pointerDown(hold, { pointerId: 1 });
        expect(screen.getByRole("button", { name: "Release to ask" })).toHaveAttribute(
            "aria-pressed",
            "true",
        );

        rerender(<ScreenGuidePanel isVisible={false} onClose={() => {}} />);
        await waitFor(() => expect(mocks.screenGuideRelease).toHaveBeenCalledWith(1));
        rerender(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(await screen.findByRole("button", { name: "Hold to talk" })).toHaveAttribute(
            "aria-pressed",
            "false",
        );
    });
});
