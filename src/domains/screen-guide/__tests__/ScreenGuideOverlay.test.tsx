import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

const mocks = vi.hoisted(() => ({
    acknowledgeScreenGuideMicrophoneStopped: vi.fn(),
    askScreenGuide: vi.fn(),
    emitScreenGuideState: vi.fn(),
    finishScreenGuideVisual: vi.fn(),
    getScreenGuideCursorPosition: vi.fn(),
    getScreenGuideSettings: vi.fn(),
    onScreenGuideShortcut: vi.fn(),
    onScreenGuideSubmit: vi.fn(),
    screenGuideMicrophoneStarted: vi.fn(),
    setScreenGuideOverlayReady: vi.fn(),
    transcribeScreenGuideVoiceInput: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => mocks);

import { ScreenGuideOverlay } from "../ScreenGuideOverlay";

class FakeMediaRecorder {
    static instances: FakeMediaRecorder[] = [];
    static isTypeSupported() {
        return true;
    }

    mimeType = "audio/webm";
    state: RecordingState = "inactive";
    ondataavailable: ((event: { data: Blob }) => void) | null = null;
    onstop: (() => void) | null = null;

    constructor() {
        FakeMediaRecorder.instances.push(this);
    }

    start() {
        this.state = "recording";
    }

    stop() {
        this.state = "inactive";
        const data = {
            size: 5,
            arrayBuffer: async () => new TextEncoder().encode("voice").buffer,
        } as Blob;
        this.ondataavailable?.({ data });
        this.onstop?.();
    }
}

function installFakeMediaRecorder() {
    const stopTrack = vi.fn();
    const originalMediaRecorder = globalThis.MediaRecorder;
    const originalMediaDevices = navigator.mediaDevices;
    const originalBlobArrayBuffer = Object.getOwnPropertyDescriptor(Blob.prototype, "arrayBuffer");
    FakeMediaRecorder.instances = [];

    Object.defineProperty(globalThis, "MediaRecorder", {
        configurable: true,
        value: FakeMediaRecorder,
    });
    Object.defineProperty(navigator, "mediaDevices", {
        configurable: true,
        value: {
            getUserMedia: vi.fn().mockResolvedValue({
                getTracks: () => [{ stop: stopTrack }],
            }),
        },
    });
    Object.defineProperty(Blob.prototype, "arrayBuffer", {
        configurable: true,
        value: async () => new TextEncoder().encode("voice").buffer,
    });

    return {
        stopTrack,
        restore() {
            Object.defineProperty(globalThis, "MediaRecorder", {
                configurable: true,
                value: originalMediaRecorder,
            });
            Object.defineProperty(navigator, "mediaDevices", {
                configurable: true,
                value: originalMediaDevices,
            });
            if (originalBlobArrayBuffer) {
                Object.defineProperty(Blob.prototype, "arrayBuffer", originalBlobArrayBuffer);
            } else {
                Reflect.deleteProperty(Blob.prototype, "arrayBuffer");
            }
        },
    };
}

describe("ScreenGuideOverlay", () => {
    let shortcutHandler: ((payload: {
        action: "press" | "release" | "cancel";
        generation: number;
    }) => void) | null;
    let submitHandler: ((payload: { text: string; generation: number }) => void) | null;

    beforeEach(() => {
        shortcutHandler = null;
        submitHandler = null;
        for (const mock of Object.values(mocks)) mock.mockReset();
        mocks.getScreenGuideSettings.mockResolvedValue({
            enabled: true,
            shortcut: "Control+Alt+Space",
            speak_responses: true,
            show_cursor: true,
        });
        mocks.getScreenGuideCursorPosition.mockResolvedValue({ x: 40, y: 50 });
        mocks.onScreenGuideShortcut.mockImplementation(async (handler) => {
            shortcutHandler = handler;
            return vi.fn();
        });
        mocks.onScreenGuideSubmit.mockImplementation(async (handler) => {
            submitHandler = handler;
            return vi.fn();
        });
        mocks.askScreenGuide.mockResolvedValue({ answer: "Use Save.", point_cue: null });
        mocks.finishScreenGuideVisual.mockResolvedValue(true);
        mocks.screenGuideMicrophoneStarted.mockResolvedValue(undefined);
        mocks.acknowledgeScreenGuideMicrophoneStopped.mockResolvedValue(true);
        mocks.setScreenGuideOverlayReady.mockResolvedValue(undefined);
        mocks.emitScreenGuideState.mockResolvedValue(undefined);
    });

    afterEach(() => cleanup());

    it("marks the hidden overlay ready only after both native listeners mount", async () => {
        const { unmount } = render(<ScreenGuideOverlay />);

        await waitFor(() => expect(mocks.setScreenGuideOverlayReady).toHaveBeenCalledWith(true));
        expect(mocks.onScreenGuideShortcut).toHaveBeenCalledTimes(1);
        expect(mocks.onScreenGuideSubmit).toHaveBeenCalledTimes(1);

        unmount();
        await waitFor(() => expect(mocks.setScreenGuideOverlayReady).toHaveBeenCalledWith(false));
    });

    it("stops an active track before marking an unmounted overlay not ready", async () => {
        const recorder = installFakeMediaRecorder();

        try {
            const { unmount } = render(<ScreenGuideOverlay />);
            await waitFor(() => expect(shortcutHandler).not.toBeNull());
            act(() => shortcutHandler?.({ action: "press", generation: 1 }));
            await waitFor(() => expect(FakeMediaRecorder.instances[0]?.state).toBe("recording"));

            unmount();
            await waitFor(() =>
                expect(mocks.setScreenGuideOverlayReady).toHaveBeenCalledWith(false),
            );

            const notReadyCall = mocks.setScreenGuideOverlayReady.mock.calls.findIndex(
                ([ready]) => ready === false,
            );
            expect(recorder.stopTrack).toHaveBeenCalledTimes(1);
            expect(recorder.stopTrack.mock.invocationCallOrder[0]).toBeLessThan(
                mocks.setScreenGuideOverlayReady.mock.invocationCallOrder[notReadyCall],
            );
        } finally {
            recorder.restore();
        }
    });

    it("cancels an active recording without transcribing its chunks", async () => {
        const recorder = installFakeMediaRecorder();

        try {
            render(<ScreenGuideOverlay />);
            await waitFor(() => expect(shortcutHandler).not.toBeNull());

            act(() => shortcutHandler?.({ action: "press", generation: 1 }));
            await waitFor(() => expect(FakeMediaRecorder.instances[0]?.state).toBe("recording"));
            mocks.emitScreenGuideState.mockClear();

            act(() => shortcutHandler?.({ action: "cancel", generation: 1 }));

            await waitFor(() => expect(recorder.stopTrack).toHaveBeenCalledTimes(1));
            await waitFor(() =>
                expect(mocks.acknowledgeScreenGuideMicrophoneStopped).toHaveBeenCalledWith(1),
            );
            expect(recorder.stopTrack.mock.invocationCallOrder[0]).toBeLessThan(
                mocks.acknowledgeScreenGuideMicrophoneStopped.mock.invocationCallOrder[0],
            );
            expect(mocks.transcribeScreenGuideVoiceInput).not.toHaveBeenCalled();
            await waitFor(() =>
                expect(mocks.emitScreenGuideState).toHaveBeenCalledWith({
                    phase: "idle",
                    message: null,
                }),
            );
        } finally {
            recorder.restore();
        }
    });

    it("waits for pending microphone permission before acknowledging release", async () => {
        const recorder = installFakeMediaRecorder();
        const delayedStopTrack = vi.fn();
        let resolveStream: ((stream: MediaStream) => void) | null = null;
        const getUserMedia = vi.fn().mockImplementation(
            () => new Promise<MediaStream>((resolve) => {
                resolveStream = resolve;
            }),
        );
        Object.defineProperty(navigator, "mediaDevices", {
            configurable: true,
            value: { getUserMedia },
        });

        try {
            render(<ScreenGuideOverlay />);
            await waitFor(() => expect(shortcutHandler).not.toBeNull());

            act(() => shortcutHandler?.({ action: "press", generation: 1 }));
            await waitFor(() => expect(getUserMedia).toHaveBeenCalledTimes(1));
            act(() => shortcutHandler?.({ action: "release", generation: 1 }));

            await act(async () => Promise.resolve());
            expect(mocks.acknowledgeScreenGuideMicrophoneStopped).not.toHaveBeenCalled();

            await act(async () => {
                resolveStream?.({
                    getTracks: () => [{ stop: delayedStopTrack } as unknown as MediaStreamTrack],
                } as unknown as MediaStream);
                await Promise.resolve();
            });

            await waitFor(() => expect(delayedStopTrack).toHaveBeenCalledTimes(1));
            await waitFor(() =>
                expect(mocks.acknowledgeScreenGuideMicrophoneStopped).toHaveBeenCalledWith(1),
            );
            expect(delayedStopTrack.mock.invocationCallOrder[0]).toBeLessThan(
                mocks.acknowledgeScreenGuideMicrophoneStopped.mock.invocationCallOrder[0],
            );
            expect(FakeMediaRecorder.instances).toHaveLength(0);
        } finally {
            recorder.restore();
        }
    });

    it("defers a superseding press acknowledgement until pending media is stopped", async () => {
        const recorder = installFakeMediaRecorder();
        const delayedStopTrack = vi.fn();
        let resolveStream: ((stream: MediaStream) => void) | null = null;
        const getUserMedia = vi.fn().mockImplementation(
            () => new Promise<MediaStream>((resolve) => {
                resolveStream = resolve;
            }),
        );
        Object.defineProperty(navigator, "mediaDevices", {
            configurable: true,
            value: { getUserMedia },
        });

        try {
            render(<ScreenGuideOverlay />);
            await waitFor(() => expect(shortcutHandler).not.toBeNull());

            act(() => shortcutHandler?.({ action: "press", generation: 1 }));
            await waitFor(() => expect(getUserMedia).toHaveBeenCalledTimes(1));
            act(() => shortcutHandler?.({ action: "press", generation: 2 }));

            await act(async () => Promise.resolve());
            expect(mocks.acknowledgeScreenGuideMicrophoneStopped).not.toHaveBeenCalled();

            await act(async () => {
                resolveStream?.({
                    getTracks: () => [{ stop: delayedStopTrack } as unknown as MediaStreamTrack],
                } as unknown as MediaStream);
                await Promise.resolve();
            });

            await waitFor(() => expect(delayedStopTrack).toHaveBeenCalledTimes(1));
            await waitFor(() =>
                expect(mocks.acknowledgeScreenGuideMicrophoneStopped).toHaveBeenCalledWith(2),
            );
            expect(delayedStopTrack.mock.invocationCallOrder[0]).toBeLessThan(
                mocks.acknowledgeScreenGuideMicrophoneStopped.mock.invocationCallOrder[0],
            );
            expect(FakeMediaRecorder.instances).toHaveLength(0);
        } finally {
            recorder.restore();
        }
    });

    it("lets a newer shortcut press supersede an active frontend turn", async () => {
        const recorder = installFakeMediaRecorder();

        try {
            render(<ScreenGuideOverlay />);
            await waitFor(() => expect(shortcutHandler).not.toBeNull());

            act(() => shortcutHandler?.({ action: "press", generation: 1 }));
            await waitFor(() => expect(FakeMediaRecorder.instances[0]?.state).toBe("recording"));
            act(() => shortcutHandler?.({ action: "press", generation: 2 }));

            await waitFor(() => expect(FakeMediaRecorder.instances).toHaveLength(2));
            expect(FakeMediaRecorder.instances[0].state).toBe("inactive");
            expect(FakeMediaRecorder.instances[1].state).toBe("recording");
            expect(recorder.stopTrack).toHaveBeenCalledTimes(1);
            expect(mocks.transcribeScreenGuideVoiceInput).not.toHaveBeenCalled();
        } finally {
            recorder.restore();
        }
    });

    it("cancels pending transcription before it can start a fresh ask", async () => {
        const recorder = installFakeMediaRecorder();
        let resolveTranscript: ((result: { text: string; backend: string }) => void) | null = null;
        mocks.transcribeScreenGuideVoiceInput.mockReturnValue(
            new Promise((resolve) => {
                resolveTranscript = resolve;
            }),
        );
        const now = vi.spyOn(Date, "now").mockReturnValueOnce(1_000).mockReturnValue(5_000);

        try {
            render(<ScreenGuideOverlay />);
            await waitFor(() => expect(shortcutHandler).not.toBeNull());

            act(() => shortcutHandler?.({ action: "press", generation: 1 }));
            await waitFor(() => expect(FakeMediaRecorder.instances[0]?.state).toBe("recording"));
            act(() => shortcutHandler?.({ action: "release", generation: 1 }));
            await waitFor(() =>
                expect(mocks.transcribeScreenGuideVoiceInput).toHaveBeenCalledTimes(1),
            );
            mocks.emitScreenGuideState.mockClear();

            act(() => shortcutHandler?.({ action: "cancel", generation: 1 }));
            await act(async () => {
                resolveTranscript?.({ text: "Where is Save?", backend: "local" });
                await Promise.resolve();
            });

            expect(mocks.askScreenGuide).not.toHaveBeenCalled();
            await waitFor(() =>
                expect(mocks.emitScreenGuideState).toHaveBeenCalledWith({
                    phase: "idle",
                    message: null,
                }),
            );
        } finally {
            now.mockRestore();
            recorder.restore();
        }
    });

    it("discards an active voice recording when a typed question supersedes it", async () => {
        const recorder = installFakeMediaRecorder();

        try {
            render(<ScreenGuideOverlay />);
            await waitFor(() => expect(submitHandler).not.toBeNull());

            act(() => shortcutHandler?.({ action: "press", generation: 1 }));
            await waitFor(() => expect(FakeMediaRecorder.instances[0]?.state).toBe("recording"));
            act(() => submitHandler?.({ text: "Where is Save?", generation: 2 }));

            await waitFor(() => expect(mocks.askScreenGuide).toHaveBeenCalledTimes(1));
            expect(recorder.stopTrack).toHaveBeenCalledTimes(1);
            expect(mocks.transcribeScreenGuideVoiceInput).not.toHaveBeenCalled();
        } finally {
            recorder.restore();
        }
    });

    it("uses the native voice generation for guarded error cleanup", async () => {
        vi.useFakeTimers();
        const originalMediaRecorder = globalThis.MediaRecorder;
        Object.defineProperty(globalThis, "MediaRecorder", {
            configurable: true,
            value: undefined,
        });

        try {
            render(<ScreenGuideOverlay />);
            await act(async () => {
                await vi.runAllTicks();
            });

            act(() => shortcutHandler?.({ action: "press", generation: 41 }));
            await act(async () => {
                await vi.runAllTicks();
            });
            expect(screen.getByText("Microphone recording is not available in this build."))
                .toBeInTheDocument();
            expect(screen.getByRole("alert")).toHaveTextContent(
                "Microphone recording is not available in this build.",
            );
            expect(screen.getByText("SCREEN GUIDE")).toBeInTheDocument();
            expect(mocks.acknowledgeScreenGuideMicrophoneStopped).toHaveBeenCalledWith(41);

            await act(async () => {
                await vi.advanceTimersByTimeAsync(3_500);
            });
            expect(mocks.finishScreenGuideVisual).toHaveBeenCalledWith(41);
        } finally {
            Object.defineProperty(globalThis, "MediaRecorder", {
                configurable: true,
                value: originalMediaRecorder,
            });
            vi.useRealTimers();
        }
    });

    it("auto-hides only the completed visual turn and does not stop speech", async () => {
        vi.useFakeTimers();
        try {
            render(<ScreenGuideOverlay />);
            await act(async () => {
                await vi.runAllTicks();
            });
            expect(submitHandler).not.toBeNull();

            act(() => submitHandler?.({ text: "Where is Save?", generation: 1 }));
            await act(async () => {
                await vi.runAllTicks();
            });
            const requestId = mocks.askScreenGuide.mock.calls[0]?.[1];
            await act(async () => {
                await vi.advanceTimersByTimeAsync(9_000);
            });

            expect(mocks.finishScreenGuideVisual).toHaveBeenCalledWith(requestId);
        } finally {
            vi.useRealTimers();
        }
    });

    it("retries a failed visual finish without interrupting speech", async () => {
        vi.useFakeTimers();
        mocks.finishScreenGuideVisual
            .mockRejectedValueOnce(new Error("window busy"))
            .mockRejectedValueOnce(new Error("window busy"))
            .mockResolvedValue(true);

        try {
            render(<ScreenGuideOverlay />);
            await act(async () => {
                await vi.runAllTicks();
            });
            act(() => submitHandler?.({ text: "Where is Save?", generation: 1 }));
            await act(async () => {
                await vi.runAllTicks();
            });
            await act(async () => {
                await vi.advanceTimersByTimeAsync(9_500);
            });

            expect(mocks.finishScreenGuideVisual).toHaveBeenCalledTimes(3);
        } finally {
            vi.useRealTimers();
        }
    });

    it("keeps a bounded long answer visible for reading time", async () => {
        vi.useFakeTimers();
        const answer = Array.from({ length: 50 }, (_, index) => `word${index}`).join(" ");
        mocks.askScreenGuide.mockResolvedValue({ answer, point_cue: null });

        try {
            render(<ScreenGuideOverlay />);
            await act(async () => {
                await vi.runAllTicks();
            });
            act(() => submitHandler?.({ text: "Explain this screen", generation: 1 }));
            await act(async () => {
                await vi.runAllTicks();
            });

            expect(screen.getByText(answer)).toBeInTheDocument();
            await act(async () => {
                await vi.advanceTimersByTimeAsync(9_000);
            });
            expect(mocks.finishScreenGuideVisual).not.toHaveBeenCalled();

            await act(async () => {
                await vi.advanceTimersByTimeAsync(6_000);
            });
            expect(mocks.finishScreenGuideVisual).toHaveBeenCalledTimes(1);
        } finally {
            vi.useRealTimers();
        }
    });

    it("keeps an edge cue anchored while placing its label inside the viewport", async () => {
        mocks.askScreenGuide.mockResolvedValue({
            answer: "Use the control in the corner.",
            point_cue: { x: 0.98, y: 0.96, label: "Save" },
        });
        const { container } = render(<ScreenGuideOverlay />);
        await waitFor(() => expect(submitHandler).not.toBeNull());

        act(() => submitHandler?.({ text: "Where is Save?", generation: 1 }));
        expect(await screen.findByText("Use the control in the corner.")).toBeInTheDocument();

        const cursor = container.querySelector<HTMLElement>(".sg-guidance-cursor");
        expect(cursor).toHaveAttribute("data-label-placement", "left-above");
        expect(cursor?.style.left).toBe(`${window.innerWidth * 0.98}px`);
        expect(cursor?.style.top).toBe(`${window.innerHeight * 0.96}px`);
    });

    it("does not let an in-flight old finish clear a newer typed answer", async () => {
        vi.useFakeTimers();
        let resolveOldFinish: (() => void) | null = null;
        mocks.finishScreenGuideVisual.mockReturnValueOnce(
            new Promise<boolean>((resolve) => {
                resolveOldFinish = () => resolve(true);
            }),
        );
        mocks.askScreenGuide
            .mockResolvedValueOnce({ answer: "First answer", point_cue: null })
            .mockResolvedValueOnce({ answer: "Second answer", point_cue: null });

        try {
            render(<ScreenGuideOverlay />);
            await act(async () => {
                await vi.runAllTicks();
            });
            act(() => submitHandler?.({ text: "First question", generation: 1 }));
            await act(async () => {
                await vi.runAllTicks();
            });
            await act(async () => {
                await vi.advanceTimersByTimeAsync(9_000);
            });

            act(() => submitHandler?.({ text: "Second question", generation: 2 }));
            await act(async () => {
                await vi.runAllTicks();
            });
            expect(screen.getByText("Second answer")).toBeInTheDocument();

            await act(async () => {
                resolveOldFinish?.();
                await vi.runAllTicks();
            });
            expect(screen.getByText("Second answer")).toBeInTheDocument();
        } finally {
            vi.useRealTimers();
        }
    });

    it("ignores delayed events from an older native generation", async () => {
        let resolveFirst: ((value: { answer: string; point_cue: null }) => void) | null = null;
        mocks.askScreenGuide
            .mockReturnValueOnce(new Promise((resolve) => {
                resolveFirst = resolve;
            }))
            .mockResolvedValueOnce({ answer: "Newest answer", point_cue: null });

        render(<ScreenGuideOverlay />);
        await waitFor(() => expect(submitHandler).not.toBeNull());

        act(() => submitHandler?.({ text: "First", generation: 10 }));
        await waitFor(() => expect(mocks.askScreenGuide).toHaveBeenCalledTimes(1));
        act(() => submitHandler?.({ text: "Second", generation: 11 }));
        expect(await screen.findByText("Newest answer")).toBeInTheDocument();

        act(() => {
            shortcutHandler?.({ action: "cancel", generation: 10 });
            shortcutHandler?.({ action: "press", generation: 10 });
            submitHandler?.({ text: "Delayed stale question", generation: 10 });
        });
        await act(async () => {
            resolveFirst?.({ answer: "Old answer", point_cue: null });
            await Promise.resolve();
        });

        expect(mocks.askScreenGuide).toHaveBeenCalledTimes(2);
        expect(screen.getByText("Newest answer")).toBeInTheDocument();
        expect(screen.queryByText("Old answer")).not.toBeInTheDocument();
    });
});
