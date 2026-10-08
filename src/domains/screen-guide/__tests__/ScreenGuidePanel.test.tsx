import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const mocks = vi.hoisted(() => ({
    armScreenGuideDiagnostic: vi.fn(),
    deleteScreenGuideDiagnostics: vi.fn(),
    getScreenGuideDiagnosticStatus: vi.fn(),
    revealScreenGuideDiagnostics: vi.fn(),
    getScreenGuideSettings: vi.fn(),
    onScreenGuideState: vi.fn(),
    screenGuidePress: vi.fn(),
    screenGuideRelease: vi.fn(),
    setScreenGuideSettings: vi.fn(),
    submitScreenGuideText: vi.fn(),
    computerUseStatus: vi.fn(),
    openClickyBridgeStatus: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => mocks);

import { ScreenGuidePanel } from "../ScreenGuidePanel";

describe("ScreenGuidePanel", () => {
    let stateHandler: ((status: {
        phase: string;
        message: string | null;
        generation: number;
        activity_stage?: "reading_text" | "answering_chat_gpt";
        target_app?: string | null;
    }) => void) | null;

    beforeEach(() => {
        stateHandler = null;
        mocks.getScreenGuideSettings.mockReset().mockResolvedValue({
            enabled: false,
            shortcut: "Option+Space",
            speak_responses: false,
            show_cursor: true,
        });
        mocks.getScreenGuideDiagnosticStatus.mockReset().mockResolvedValue({
            armed: false,
            expiresInMs: null,
            bundleCount: 0,
            partialCount: 0,
            totalBytes: 0,
            lastResult: null,
        });
        mocks.armScreenGuideDiagnostic.mockReset().mockResolvedValue({
            armed: true,
            expiresInMs: 300_000,
            bundleCount: 0,
            partialCount: 0,
            totalBytes: 0,
            lastResult: null,
        });
        mocks.deleteScreenGuideDiagnostics.mockReset().mockResolvedValue({
            armed: false,
            expiresInMs: null,
            bundleCount: 0,
            partialCount: 0,
            totalBytes: 0,
            lastResult: {
                kind: "deleted",
                code: "diagnostics_deleted",
                message: "Deleted local Screen Guide diagnostics.",
                screenshotSaved: false,
                ocrSaved: false,
            },
        });
        mocks.revealScreenGuideDiagnostics.mockReset().mockResolvedValue({
            armed: false,
            expiresInMs: null,
            bundleCount: 1,
            partialCount: 0,
            totalBytes: 2048,
            lastResult: null,
        });
        mocks.onScreenGuideState.mockReset().mockImplementation(async (handler) => {
            stateHandler = handler;
            return vi.fn();
        });
        mocks.screenGuidePress.mockReset().mockResolvedValue(1);
        mocks.computerUseStatus.mockReset().mockResolvedValue({
            enabled: false,
            codexReady: true,
            openComputerUsePath: null,
            active: false,
        });
        mocks.openClickyBridgeStatus.mockReset().mockResolvedValue({
            reachable: false,
            tokenFound: false,
            bridgeTokenConfigured: false,
        });
        mocks.screenGuideRelease.mockReset().mockResolvedValue(undefined);
        mocks.setScreenGuideSettings.mockReset().mockImplementation(async (settings) => settings);
        mocks.submitScreenGuideText.mockReset().mockResolvedValue(undefined);
    });

    afterEach(() => {
        cleanup();
        vi.useRealTimers();
    });

    it("renders a disabled state with explicit readiness and local privacy guidance", async () => {
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(screen.getByRole("dialog", { name: "Screen Guide" })).toBeInTheDocument();
        expect(await screen.findByRole("heading", { name: "Screen Guide" })).toBeInTheDocument();
        expect(screen.getByText("Screen Guide is off")).toBeInTheDocument();
        expect(screen.getByText(/stay on this Mac for this turn/i)).toBeInTheDocument();
        expect(screen.getByText(/does not click or type/i)).toBeInTheDocument();
        expect(screen.getByText(/not added to Memory Vault/i)).toBeInTheDocument();
        expect(
            screen.getByText(/file names in Documents, Desktop, and Downloads/i),
        ).toBeInTheDocument();
        expect(screen.getByRole("switch", { name: "Enable Screen Guide" })).not.toBeChecked();
        expect(
            screen.getByRole("textbox", { name: "Ask about your display or find a named file" }),
        ).toBeDisabled();
        expect(screen.getByRole("button", { name: "Ask Screen Guide" })).toBeDisabled();
        expect(screen.getByRole("button", { name: /hold to talk/i })).toBeDisabled();
        expect(screen.getByText(/Option\+Space/)).toBeInTheDocument();
    });

    it("shows live private mode instead of claiming the shortcut is ready", async () => {
        mocks.getScreenGuideSettings.mockResolvedValueOnce({
            enabled: true,
            shortcut: "Control+Alt+Space",
            speak_responses: false,
            show_cursor: true,
        });

        render(<ScreenGuidePanel isVisible isPrivateMode onClose={() => {}} />);

        expect(await screen.findByText("FNDR Private Mode is on")).toBeInTheDocument();
        expect(screen.getByText(/exit Private Mode in Settings/i)).toBeInTheDocument();
        expect(screen.queryByText(/from any app/i)).toBeNull();
        expect(screen.getByRole("button", { name: "Ask Screen Guide" })).toBeDisabled();
        expect(screen.getByRole("button", { name: /hold to talk/i })).toBeDisabled();
        expect(screen.getByRole("button", { name: "Save next turn" })).toBeDisabled();
    });

    it("requires an explicit one-shot opt-in and can delete local diagnostics", async () => {
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        fireEvent.click(await screen.findByRole("button", { name: "Save next turn" }));

        await waitFor(() => expect(mocks.armScreenGuideDiagnostic).toHaveBeenCalledTimes(1));
        expect(await screen.findByText(/Next turn is armed/i)).toBeInTheDocument();
        expect(screen.getByText(/can contain anything visible/i)).toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Delete diagnostics" }));
        await waitFor(() => expect(mocks.deleteScreenGuideDiagnostics).toHaveBeenCalledTimes(1));
        expect(screen.getByRole("button", { name: "Save next turn" })).toBeEnabled();
    });

    it("refreshes diagnostic status when Private Mode changes", async () => {
        mocks.getScreenGuideDiagnosticStatus
            .mockResolvedValueOnce({
                armed: true,
                expiresInMs: 120_000,
                bundleCount: 0,
                partialCount: 0,
                totalBytes: 0,
                lastResult: null,
            })
            .mockResolvedValueOnce({
                armed: false,
                expiresInMs: null,
                bundleCount: 0,
                partialCount: 0,
                totalBytes: 0,
                lastResult: {
                    kind: "cancelled",
                    code: "private_mode_disarmed",
                    message: "Private Mode disarmed Screen Guide diagnostics.",
                    screenshotSaved: false,
                    ocrSaved: false,
                },
            });
        const { rerender } = render(
            <ScreenGuidePanel isVisible isPrivateMode={false} onClose={() => {}} />,
        );

        expect(await screen.findByText(/Next turn is armed/i)).toBeInTheDocument();
        rerender(<ScreenGuidePanel isVisible isPrivateMode onClose={() => {}} />);

        await waitFor(() => expect(mocks.getScreenGuideDiagnosticStatus).toHaveBeenCalledTimes(2));
        expect(screen.queryByText(/Next turn is armed/i)).not.toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Save next turn" })).toBeDisabled();
    });

    it("refreshes an armed diagnostic when its backend deadline expires", async () => {
        vi.useFakeTimers();
        mocks.getScreenGuideDiagnosticStatus
            .mockResolvedValueOnce({
                armed: true,
                expiresInMs: 60_000,
                bundleCount: 0,
                partialCount: 0,
                totalBytes: 0,
                lastResult: null,
            })
            .mockResolvedValueOnce({
                armed: false,
                expiresInMs: null,
                bundleCount: 0,
                partialCount: 0,
                totalBytes: 0,
                lastResult: null,
            });
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        await act(async () => {
            await Promise.resolve();
            await Promise.resolve();
        });
        expect(screen.getByText(/Next turn is armed/i)).toBeInTheDocument();

        await act(async () => {
            await vi.advanceTimersByTimeAsync(60_000);
        });

        expect(mocks.getScreenGuideDiagnosticStatus).toHaveBeenCalledTimes(2);
        expect(screen.queryByText(/Next turn is armed/i)).not.toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Save next turn" })).toBeEnabled();
    });

    it("discloses screenshot egress instead of labeling a ChatGPT turn local", async () => {
        mocks.getScreenGuideSettings.mockResolvedValue({
            enabled: true,
            shortcut: "Control+Alt+Space",
            speak_responses: false,
            show_cursor: true,
            model: "codex",
            send_screenshot_to_codex: true,
            operate_computer: false,
        });
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(await screen.findByText("Cloud answer with screenshot")).toBeInTheDocument();
        expect(screen.getByText(/downscaled screenshot are sent to OpenAI/i)).toBeInTheDocument();
        expect(screen.queryByText("Local and read-only")).not.toBeInTheDocument();
    });

    it("does not claim a local boundary before settings identify the active mode", () => {
        mocks.getScreenGuideSettings.mockReturnValue(new Promise(() => undefined));
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(screen.getByText("Checking privacy and agency settings")).toBeInTheDocument();
        expect(screen.queryByText("Local and read-only")).not.toBeInTheDocument();
    });

    it("shows the backend diagnostic receipt and reveals only through path-free IPC", async () => {
        mocks.getScreenGuideDiagnosticStatus.mockResolvedValue({
            armed: false,
            expiresInMs: null,
            bundleCount: 1,
            partialCount: 0,
            totalBytes: 2048,
            lastResult: {
                kind: "saved",
                code: "bundle_saved",
                message: "Saved one local Screen Guide diagnostic bundle.",
                screenshotSaved: true,
                ocrSaved: false,
            },
        });
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(await screen.findByText(/1 local diagnostic bundle/i)).toHaveTextContent("2.0 KB");
        expect(screen.getByText(/Receipt: screenshot saved; OCR not saved/i)).toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Reveal diagnostics in Finder" }));
        await waitFor(() => expect(mocks.revealScreenGuideDiagnostics).toHaveBeenCalledTimes(1));
        expect(mocks.revealScreenGuideDiagnostics).toHaveBeenCalledWith();
    });

    it("shows a bounded backend diagnostic error code", async () => {
        mocks.getScreenGuideDiagnosticStatus.mockResolvedValue({
            armed: false,
            expiresInMs: null,
            bundleCount: 0,
            partialCount: 0,
            totalBytes: 0,
            lastResult: {
                kind: "error",
                code: "bundle_too_large",
                message: "The diagnostic bundle exceeded its local size limit.",
                screenshotSaved: false,
                ocrSaved: false,
            },
        });
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(await screen.findByRole("alert")).toHaveTextContent(
            "The diagnostic bundle exceeded its local size limit. (bundle_too_large)",
        );
    });

    it("discloses approval-gated agency instead of labeling Operate mode read-only", async () => {
        mocks.getScreenGuideSettings.mockResolvedValue({
            enabled: true,
            shortcut: "Control+Alt+Space",
            speak_responses: false,
            show_cursor: true,
            model: "local",
            operate_computer: true,
        });
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(
            await screen.findByText("Local answers with approval-gated control"),
        ).toBeInTheDocument();
        expect(screen.getByText(/click and type only after showing an approval/i)).toBeInTheDocument();
        expect(screen.queryByText("Local and read-only")).not.toBeInTheDocument();
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
        expect(screen.getByRole("switch", { name: "Enable Screen Guide" })).toBeEnabled();
    });

    it("persists the complete settings contract when enabled", async () => {
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);
        const enable = await screen.findByRole("switch", { name: "Enable Screen Guide" });

        fireEvent.click(enable);

        await waitFor(() =>
            expect(mocks.setScreenGuideSettings).toHaveBeenCalledWith({
                enabled: true,
                shortcut: "Option+Space",
                speak_responses: false,
                show_cursor: true,
            }),
        );
        expect(
            screen.getByRole("textbox", { name: "Ask about your display or find a named file" }),
        ).toBeEnabled();
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

        act(() => stateHandler?.({ phase: "listening", message: "Listening…", generation: 1 }));
        expect(screen.getByText("Listening…")).toBeInTheDocument();

        rerender(<ScreenGuidePanel isVisible={false} onClose={() => {}} />);
        act(() => stateHandler?.({ phase: "idle", message: null, generation: 1 }));
        rerender(<ScreenGuidePanel isVisible onClose={() => {}} />);

        expect(await screen.findByText("Enabled")).toBeInTheDocument();
        expect(screen.queryByText("Listening…")).not.toBeInTheDocument();
        expect(mocks.onScreenGuideState).toHaveBeenCalledTimes(1);
    });

    it("shows typed live activity without exposing captured content", async () => {
        mocks.getScreenGuideSettings.mockResolvedValue({
            enabled: true,
            shortcut: "Control+Alt+Space",
            speak_responses: true,
            show_cursor: true,
        });
        render(<ScreenGuidePanel isVisible onClose={() => {}} />);
        await waitFor(() => expect(stateHandler).not.toBeNull());

        act(() => stateHandler?.({
            phase: "thinking",
            message: "Answering with ChatGPT…",
            generation: 3,
            activity_stage: "answering_chat_gpt",
            target_app: null,
        }));

        expect(screen.getByRole("region", { name: "Screen Guide activity" }))
            .toHaveTextContent("Answering with ChatGPT");
        expect(screen.getAllByRole("status")).toHaveLength(1);
        fireEvent.click(screen.getByRole("button", { name: "Show Screen Guide activity details" }));
        expect(screen.getByText(/No prompts, captured text, or private model reasoning/i))
            .toBeInTheDocument();
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
