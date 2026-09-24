import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const eventMocks = vi.hoisted(() => ({
    listen: vi.fn(),
}));

const ipcMocks = vi.hoisted(() => ({
    NOTCH_HUD_HOVER_EVENT: "notch-hud://hover",
    NOTCH_HUD_GEOMETRY_EVENT: "notch-hud://geometry",
    getNotchHudGeometry: vi.fn(),
    setNotchHudHitRect: vi.fn(),
    setNotchHudKeyboard: vi.fn(),
    notchHudOpenMemory: vi.fn(),
    searchMemoryCards: vi.fn(),
    listMemoryCards: vi.fn(),
    fndrAnswer: vi.fn(),
    transcribeVoiceInput: vi.fn(),
    COMPUTER_USE_EVENT: "computer-use://event",
    computerUseStatus: vi.fn(),
    computerUseSay: vi.fn(),
    computerUseInterrupt: vi.fn(),
    computerUseRespond: vi.fn(),
    computerUseStop: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => eventMocks);
vi.mock("@/shared/ipc/tauri", () => ipcMocks);

import type { MemoryCard } from "@/shared/ipc/tauri";
import { NotchHud } from "./NotchHud";

const geometry = {
    closed_width: 220,
    closed_height: 38,
    is_physical_notch: true,
    window_width: 622,
    window_height: 440,
    screen_width: 1512,
    screen_height: 982,
};

const card = {
    id: "memory-1",
    title: "Refactored the notch overlay",
    summary: "",
    action: "",
    context: [],
    timestamp: Date.now(),
    app_name: "Xcode",
    window_title: "",
    score: 1,
    source_count: 1,
    raw_snippets: [],
} as unknown as MemoryCard;

/**
 * jsdom has neither `MediaRecorder` nor a microphone, so the mic button would
 * never render. This is the smallest recorder that still exercises the real
 * start → stop → bytes path in `notchVoice.ts`.
 */
// jsdom's Blob has no `arrayBuffer()`; WKWebView's does.
if (typeof Blob.prototype.arrayBuffer !== "function") {
    Blob.prototype.arrayBuffer = function arrayBuffer(): Promise<ArrayBuffer> {
        return Promise.resolve(new Uint8Array([1, 2, 3, 4]).buffer);
    };
}

class FakeMediaRecorder {
    static isTypeSupported(): boolean {
        return true;
    }
    ondataavailable: ((event: { data: Blob }) => void) | null = null;
    onstop: (() => void) | null = null;
    mimeType = "audio/webm";
    start(): void {
        this.ondataavailable?.({ data: new Blob(["audio"], { type: this.mimeType }) });
    }
    stop(): void {
        this.onstop?.();
    }
}

const handlers = new Map<string, (event: { payload: unknown }) => void>();

/** Fire a backend event the way the Rust side emits it. */
function emit(event: string, payload: unknown) {
    act(() => {
        handlers.get(event)?.({ payload });
    });
}

/** Hover the notch, then click it open. */
async function openPanel() {
    emit("notch-hud://hover", true);
    fireEvent.mouseDown(document.querySelector(".notch-panel") as HTMLElement);
    return screen.findByLabelText("Ask FNDR");
}

describe("NotchHud", () => {
    beforeEach(() => {
        handlers.clear();
        Object.defineProperty(globalThis, "MediaRecorder", {
            value: FakeMediaRecorder,
            configurable: true,
            writable: true,
        });
        Object.defineProperty(navigator, "mediaDevices", {
            value: { getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [] }) },
            configurable: true,
        });
        eventMocks.listen.mockImplementation((event: string, handler: never) => {
            handlers.set(event, handler);
            return Promise.resolve(() => handlers.delete(event));
        });
        ipcMocks.getNotchHudGeometry.mockResolvedValue(geometry);
        ipcMocks.setNotchHudHitRect.mockResolvedValue(undefined);
        ipcMocks.setNotchHudKeyboard.mockResolvedValue(undefined);
        ipcMocks.notchHudOpenMemory.mockResolvedValue(undefined);
        ipcMocks.listMemoryCards.mockResolvedValue([card]);
        ipcMocks.searchMemoryCards.mockResolvedValue([card]);
        ipcMocks.fndrAnswer.mockResolvedValue({
            query: "",
            answer: "You were reading the vLLM batching docs.",
            evidence: {},
            cards: [card],
            verify_outcome: {},
            surfacing_reasons: [],
        });
        ipcMocks.transcribeVoiceInput.mockResolvedValue({ text: "what did I read about vLLM" });
        ipcMocks.computerUseStatus.mockResolvedValue({
            enabled: false,
            codexReady: true,
            openComputerUsePath: "/opt/homebrew/bin/open-computer-use",
            active: false,
        });
        ipcMocks.computerUseSay.mockResolvedValue(undefined);
        ipcMocks.computerUseInterrupt.mockResolvedValue(undefined);
        ipcMocks.computerUseRespond.mockResolvedValue(undefined);
        ipcMocks.computerUseStop.mockResolvedValue(undefined);
    });

    afterEach(() => {
        cleanup();
        vi.clearAllMocks();
    });

    it("reports the drawn panel's frame so the window can stay click-through", async () => {
        render(<NotchHud />);
        // The first report goes out on the fallback geometry, before the
        // backend has said which display the HUD is parked on.
        await waitFor(() => {
            const calls = ipcMocks.setNotchHudHitRect.mock.calls;
            const rect = calls[calls.length - 1][0];
            expect(rect.width).toBeGreaterThan(0);
            // Centred in the fixed window, flush with its top edge on a real notch.
            expect(rect.x).toBeCloseTo((geometry.window_width - rect.width) / 2, 3);
            expect(rect.y).toBe(0);
        });
    });

    it("peeks under the pointer and opens on click", async () => {
        render(<NotchHud />);
        await waitFor(() => expect(ipcMocks.getNotchHudGeometry).toHaveBeenCalled());

        const peek = screen.getByText("Ask FNDR", { selector: ".notch-peek-label" })
            .parentElement as HTMLElement;
        expect(peek.getAttribute("aria-hidden")).toBe("true");

        emit("notch-hud://hover", true);
        await waitFor(() => expect(peek.getAttribute("aria-hidden")).toBe("false"));

        fireEvent.mouseDown(document.querySelector(".notch-panel") as HTMLElement);
        await waitFor(() => expect(ipcMocks.setNotchHudKeyboard).toHaveBeenCalledWith(true));
        // An empty question shows what FNDR saw most recently.
        await waitFor(() => expect(ipcMocks.listMemoryCards).toHaveBeenCalled());
        await screen.findByText(card.title);
    });

    it("asks FNDR in natural language and shows the answer with its sources", async () => {
        render(<NotchHud />);
        const input = await openPanel();

        fireEvent.change(input, { target: { value: "what did I read about vLLM" } });
        fireEvent.keyDown(input, { key: "Enter" });

        await screen.findByText("You were reading the vLLM batching docs.");
        expect(ipcMocks.fndrAnswer).toHaveBeenCalledWith("what did I read about vLLM", 3);
        // The question stays on screen above its answer, and the input clears.
        expect(screen.getByText("what did I read about vLLM")).toBeTruthy();
        expect((input as HTMLInputElement).value).toBe("");

        // Cited memories open in the vault.
        fireEvent.click(screen.getByText(`${card.app_name} · ${card.title}`));
        await waitFor(() => expect(ipcMocks.notchHudOpenMemory).toHaveBeenCalledWith("memory-1"));
    });

    it("expands a follow-up with the question that came before it", async () => {
        render(<NotchHud />);
        const input = await openPanel();

        fireEvent.change(input, { target: { value: "what did I read about vLLM batching" } });
        fireEvent.keyDown(input, { key: "Enter" });
        await screen.findByText("You were reading the vLLM batching docs.");

        fireEvent.change(input, { target: { value: "what about yesterday?" } });
        fireEvent.keyDown(input, { key: "Enter" });

        await waitFor(() => expect(ipcMocks.fndrAnswer).toHaveBeenCalledTimes(2));
        const [followUpQuery] = ipcMocks.fndrAnswer.mock.calls[1];
        expect(followUpQuery).toBe("what did I read about vLLM batching what about yesterday?");
    });

    it("opens a highlighted memory instead of asking", async () => {
        render(<NotchHud />);
        const input = await openPanel();

        fireEvent.change(input, { target: { value: "notch" } });
        await waitFor(() =>
            expect(ipcMocks.searchMemoryCards).toHaveBeenCalledWith("notch", undefined, undefined, 5)
        );
        await screen.findByText(card.title);

        fireEvent.keyDown(input, { key: "ArrowDown" });
        fireEvent.keyDown(input, { key: "Enter" });

        await waitFor(() => expect(ipcMocks.notchHudOpenMemory).toHaveBeenCalledWith("memory-1"));
        expect(ipcMocks.fndrAnswer).not.toHaveBeenCalled();
    });

    it("speaks to FNDR: records, transcribes, and asks what was said", async () => {
        render(<NotchHud />);
        await openPanel();

        const started = Date.now();
        const clock = vi.spyOn(Date, "now").mockReturnValue(started);
        fireEvent.click(screen.getByLabelText("Speak to FNDR"));
        await screen.findByLabelText("Stop and send");
        // Past the minimum hold, so the clip isn't discarded as a stray tap.
        clock.mockReturnValue(started + 1500);
        fireEvent.click(screen.getByLabelText("Stop and send"));
        clock.mockRestore();

        await waitFor(() => expect(ipcMocks.transcribeVoiceInput).toHaveBeenCalled());
        await waitFor(() =>
            expect(ipcMocks.fndrAnswer).toHaveBeenCalledWith("what did I read about vLLM", 3)
        );
    });

    it("backs out one layer at a time on Escape", async () => {
        render(<NotchHud />);
        const input = await openPanel();

        fireEvent.change(input, { target: { value: "what did I read about vLLM" } });
        fireEvent.keyDown(input, { key: "Enter" });
        await screen.findByText("You were reading the vLLM batching docs.");

        // First Escape clears the thread but keeps the panel open.
        fireEvent.keyDown(input, { key: "Escape" });
        await waitFor(() =>
            expect(screen.queryByText("You were reading the vLLM batching docs.")).toBeNull()
        );
        expect(ipcMocks.setNotchHudKeyboard).not.toHaveBeenLastCalledWith(false);

        // The second closes it and hands the keyboard back.
        fireEvent.keyDown(input, { key: "Escape" });
        await waitFor(() => expect(ipcMocks.setNotchHudKeyboard).toHaveBeenLastCalledWith(false));
        const openLayer = document.querySelector(".notch-open") as HTMLElement;
        expect(openLayer.getAttribute("aria-hidden")).toBe("true");
    });

    it("wears the pill silhouette on a display without a camera housing", async () => {
        ipcMocks.getNotchHudGeometry.mockResolvedValue({
            ...geometry,
            is_physical_notch: false,
            closed_width: 132,
            closed_height: 28,
        });
        render(<NotchHud />);
        await waitFor(() => expect(ipcMocks.setNotchHudHitRect).toHaveBeenCalled());
        await waitFor(() => {
            const calls = ipcMocks.setNotchHudHitRect.mock.calls;
            // The pill is detached from the screen edge; the notch never is.
            expect(calls[calls.length - 1][0].y).toBe(6);
        });
    });

    describe("Do mode: spoken computer use", () => {
        let recognition: FakeRecognition | null = null;

        class FakeRecognition {
            continuous = false;
            interimResults = false;
            lang = "";
            onresult: ((event: unknown) => void) | null = null;
            onend: (() => void) | null = null;
            onerror: ((event: unknown) => void) | null = null;
            constructor() {
                recognition = this;
            }
            start(): void {}
            stop(): void {}
            abort(): void {}
            /** The system recognizer finishing a phrase. */
            hear(transcript: string): void {
                act(() => {
                    this.onresult?.({ resultIndex: 0, results: [Object.assign([{ transcript }], { isFinal: true })] });
                });
            }
        }

        beforeEach(() => {
            recognition = null;
            Object.defineProperty(window, "webkitSpeechRecognition", {
                value: FakeRecognition,
                configurable: true,
                writable: true,
            });
            ipcMocks.computerUseStatus.mockResolvedValue({
                enabled: true,
                codexReady: true,
                openComputerUsePath: "/opt/homebrew/bin/open-computer-use",
                active: false,
            });
        });

        afterEach(() => {
            delete (window as unknown as Record<string, unknown>).webkitSpeechRecognition;
        });

        it("keeps Do hidden until Operate my Mac is on", async () => {
            ipcMocks.computerUseStatus.mockResolvedValue({
                enabled: false,
                codexReady: true,
                openComputerUsePath: null,
                active: false,
            });
            render(<NotchHud />);
            await openPanel();
            await waitFor(() => expect(ipcMocks.computerUseStatus).toHaveBeenCalled());
            expect(screen.queryByRole("button", { name: "Do" })).not.toBeInTheDocument();
        });

        it("sends spoken instructions, asks before acting, and stops on command", async () => {
            render(<NotchHud />);
            await openPanel();
            fireEvent.click(await screen.findByRole("button", { name: "Do" }));
            await waitFor(() => expect(recognition).not.toBeNull());

            recognition!.hear("open Notes and start a new note");
            await waitFor(() =>
                expect(ipcMocks.computerUseSay).toHaveBeenCalledWith("open Notes and start a new note"),
            );

            emit("computer-use://event", { kind: "message", text: "I'll open Notes.", final: false });
            expect(await screen.findByText("I'll open Notes.")).toBeInTheDocument();

            emit("computer-use://event", {
                kind: "approval",
                requestKey: "req-1",
                tool: "click",
                summary: "click \"New Note\" in Notes",
            });
            expect(await screen.findByRole("alertdialog", { name: "Approve action" })).toHaveTextContent(
                'Okay to click "New Note" in Notes?',
            );

            recognition!.hear("yes");
            await waitFor(() => expect(ipcMocks.computerUseRespond).toHaveBeenCalledWith("req-1", true));
            expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();

            recognition!.hear("stop");
            await waitFor(() => expect(ipcMocks.computerUseInterrupt).toHaveBeenCalled());
            expect(ipcMocks.computerUseSay).toHaveBeenCalledTimes(1);
        });

        it("answers an approval with a tap and accepts typed instructions", async () => {
            render(<NotchHud />);
            await openPanel();
            fireEvent.click(await screen.findByRole("button", { name: "Do" }));

            emit("computer-use://event", {
                kind: "approval",
                requestKey: "req-2",
                tool: "type_text",
                summary: "type \"hello\" in Notes",
            });
            fireEvent.click(await screen.findByRole("button", { name: "Don't" }));
            await waitFor(() => expect(ipcMocks.computerUseRespond).toHaveBeenCalledWith("req-2", false));

            const typed = screen.getByLabelText("Instruction for FNDR");
            fireEvent.change(typed, { target: { value: "close the window" } });
            fireEvent.submit(typed.closest("form") as HTMLFormElement);
            await waitFor(() => expect(ipcMocks.computerUseSay).toHaveBeenCalledWith("close the window"));
        });
    });
});
