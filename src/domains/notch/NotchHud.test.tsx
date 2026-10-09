import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, configure, fireEvent, render, screen, waitFor } from "@testing-library/react";

const eventMocks = vi.hoisted(() => ({
    listen: vi.fn(),
}));

const coreMocks = vi.hoisted(() => ({
    invoke: vi.fn(),
}));

const onboardingMocks = vi.hoisted(() => ({
    openSystemSettings: vi.fn(),
}));

const ipcMocks = vi.hoisted(() => ({
    NOTCH_HUD_HOVER_EVENT: "notch-hud://hover",
    NOTCH_HUD_GEOMETRY_EVENT: "notch-hud://geometry",
    NOTCH_HUD_SUMMON_EVENT: "notch-hud://summon",
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
    computerUsePlan: vi.fn(),
    computerUseStart: vi.fn(),
    computerUseRespond: vi.fn(),
    computerUseStop: vi.fn(),
    codexLoginStart: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => eventMocks);
vi.mock("@tauri-apps/api/core", () => coreMocks);
vi.mock("@/shared/ipc/onboarding", () => onboardingMocks);
vi.mock("@/shared/utils/openExternalUrl", () => ({ openExternalUrl: vi.fn().mockResolvedValue(undefined) }));
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
    static instances: FakeMediaRecorder[] = [];
    static isTypeSupported(): boolean {
        return true;
    }
    ondataavailable: ((event: { data: Blob }) => void) | null = null;
    onstop: (() => void) | null = null;
    mimeType = "audio/webm";
    startArgs: unknown[] | null = null;
    constructor() {
        FakeMediaRecorder.instances.push(this);
    }
    start(...args: unknown[]): void {
        this.startArgs = args;
        this.ondataavailable?.({ data: new Blob(["audio"], { type: this.mimeType }) });
    }
    stop(): void {
        this.onstop?.();
    }
}

const handlers = new Map<string, (event: { payload: unknown }) => void>();

function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<T>((resolvePromise, rejectPromise) => {
        resolve = resolvePromise;
        reject = rejectPromise;
    });
    return { promise, resolve, reject };
}

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
        FakeMediaRecorder.instances = [];
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
            backend: "codex_computer_use",
            backendPath: "/x/computer-use-client-launcher",
            activeRun: null,
        });
        ipcMocks.computerUsePlan.mockResolvedValue("r1");
        ipcMocks.computerUseStart.mockResolvedValue(undefined);
        ipcMocks.computerUseRespond.mockResolvedValue(undefined);
        ipcMocks.computerUseStop.mockResolvedValue(undefined);
        ipcMocks.codexLoginStart.mockResolvedValue({ loginId: "l1", authUrl: "https://auth.openai.com/x" });
        let voiceSession = 0;
        coreMocks.invoke.mockImplementation((command: string) =>
            Promise.resolve(command === "voice_start" ? { sessionId: `v${++voiceSession}` } : undefined),
        );
    });

    afterEach(() => {
        cleanup();
        vi.clearAllMocks();
        window.localStorage.clear();
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

    it("traces typed memory search from debounce through a verified count without exposing the query", async () => {
        const pending = deferred<MemoryCard[]>();
        ipcMocks.searchMemoryCards.mockReturnValueOnce(pending.promise);
        render(<NotchHud />);
        const input = await openPanel();

        fireEvent.change(input, { target: { value: "private roadmap term" } });

        expect(await screen.findByText("Waiting for typing to settle")).toBeInTheDocument();
        await screen.findByText("Requesting local memory matches");
        const trace = screen.getByRole("region", { name: "Notch memory search activity" });
        expect(trace).not.toHaveTextContent("private roadmap term");

        pending.resolve([card]);
        await screen.findByText("Memory search returned 1 match");
        fireEvent.click(screen.getByRole("button", { name: "Show Notch memory search activity details" }));
        expect(screen.getByText("Typing settled").closest("li")).toHaveTextContent("Completed");
        expect(screen.getByText("Memory search request completed").closest("li")).toHaveTextContent(
            "Completed",
        );
        expect(trace).not.toHaveTextContent(card.title);
    });

    it("traces a typed FNDR answer without exposing the question, answer, or raw failure", async () => {
        const pending = deferred<Awaited<ReturnType<typeof ipcMocks.fndrAnswer>>>();
        ipcMocks.fndrAnswer.mockReturnValueOnce(pending.promise);
        render(<NotchHud />);
        const input = await openPanel();

        fireEvent.change(input, { target: { value: "private question about payroll" } });
        fireEvent.keyDown(input, { key: "Enter" });

        await screen.findByText("Requesting an answer from FNDR");
        const trace = screen.getByRole("region", { name: "Notch answer activity" });
        expect(trace).not.toHaveTextContent("private question about payroll");

        pending.resolve({
            query: "private question about payroll",
            answer: "private answer from a memory",
            evidence: {},
            cards: [card],
            verify_outcome: {},
            surfacing_reasons: [],
        });
        await screen.findByText("Answer ready with 1 memory source");
        expect(trace).not.toHaveTextContent("private answer from a memory");

        const failed = deferred<Awaited<ReturnType<typeof ipcMocks.fndrAnswer>>>();
        ipcMocks.fndrAnswer.mockReturnValueOnce(failed.promise);
        fireEvent.change(input, { target: { value: "another private question" } });
        fireEvent.keyDown(input, { key: "Enter" });
        failed.reject(new Error("/private/tmp/raw-backend-secret.log"));
        await screen.findByText("Answer request failed");
        expect(screen.getByRole("region", { name: "Notch answer activity" })).not.toHaveTextContent(
            "raw-backend-secret",
        );
    });

    it("speaks to FNDR: records, transcribes, and asks what was said", async () => {
        render(<NotchHud />);
        await openPanel();

        const started = Date.now();
        const clock = vi.spyOn(Date, "now").mockReturnValue(started);
        fireEvent.click(screen.getByLabelText("Speak to FNDR"));
        await screen.findByLabelText("Stop and send");
        expect(FakeMediaRecorder.instances[0]?.startArgs).toEqual([]);
        expect(screen.getByText("Recording voice input")).toBeInTheDocument();
        expect(screen.getByRole("region", { name: "Voice input activity" })).toBeInTheDocument();
        // Past the minimum hold, so the clip isn't discarded as a stray tap.
        clock.mockReturnValue(started + 1500);
        fireEvent.click(screen.getByLabelText("Stop and send"));
        clock.mockRestore();

        await waitFor(() => expect(ipcMocks.transcribeVoiceInput).toHaveBeenCalled());
        await waitFor(() =>
            expect(ipcMocks.fndrAnswer).toHaveBeenCalledWith("what did I read about vLLM", 3)
        );
        expect(screen.getByText("Transcript ready")).toBeInTheDocument();
        expect(screen.queryByText("what did I read about vLLM", {
            selector: ".activity-trace *",
        })).not.toBeInTheDocument();
    });

    it("categorizes microphone failures without exposing raw errors", async () => {
        Object.defineProperty(navigator, "mediaDevices", {
            value: {
                getUserMedia: vi.fn().mockRejectedValue(new Error("/private/tmp/secret.wav")),
            },
            configurable: true,
        });
        render(<NotchHud />);
        await openPanel();

        fireEvent.click(screen.getByLabelText("Speak to FNDR"));

        await screen.findByText("Microphone access failed");
        expect(screen.getByRole("region", { name: "Voice input activity" })).toHaveTextContent(
            "Failed",
        );
        expect(screen.queryByText(/secret\.wav/)).not.toBeInTheDocument();
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
        const REQUEST = "open Spotify, play Blinding Lights, then open the browser and look up looped transformers";

        // A spoken request shows for a beat before it is sent, so waits are longer here.
        beforeEach(() => configure({ asyncUtilTimeout: 3000 }));
        afterEach(() => configure({ asyncUtilTimeout: 1000 }));

        /** What the native voice owner emits for the session it last started.
         *  Terminal states are followed by idle, as `emit_terminal` does. */
        function voice(state: { kind: string } & Record<string, unknown>) {
            const starts = coreMocks.invoke.mock.calls.filter(([command]) => command === "voice_start").length;
            const event = (s: unknown) => ({ version: 1, sessionId: `v${starts}`, surface: "notch_do", state: s });
            emit("voice://state", event(state));
            if (["final", "unavailable", "error"].includes(state.kind)) emit("voice://state", event({ kind: "idle" }));
        }

        function planned(autoStart = true) {
            emit("computer-use://event", {
                kind: "planned",
                runId: "r1",
                autoStart,
                steps: [
                    { label: "Open Spotify", action: "open_app", app: "Spotify" },
                    { label: "Play Blinding Lights", action: "operate", app: "Spotify" },
                    { label: "Search looped transformers", action: "open_url", app: "" },
                ],
            });
        }

        async function openDo() {
            render(<NotchHud />);
            await openPanel();
            await waitFor(() =>
                expect(coreMocks.invoke).toHaveBeenCalledWith("voice_start", { surface: "notch_do", mode: "toggle" }),
            );
        }

        beforeEach(() => {
            ipcMocks.computerUseStatus.mockResolvedValue({
                enabled: true,
                codexReady: true,
                backend: "codex_computer_use",
                backendPath: "/x/computer-use-client-launcher",
                activeRun: null,
            });
        });

        it("Alt+N opens the panel and starts listening; pressing it again closes it", async () => {
            // Even after the person last used Ask, the shortcut opens Do.
            window.localStorage.setItem("fndr.notch.mode", "ask");
            render(<NotchHud />);
            await waitFor(() => expect(handlers.has("notch-hud://summon")).toBe(true));
            emit("notch-hud://summon", true);
            await waitFor(() =>
                expect(coreMocks.invoke).toHaveBeenCalledWith("voice_start", { surface: "notch_do", mode: "toggle" }),
            );
            expect(ipcMocks.setNotchHudKeyboard).toHaveBeenCalledWith(true);

            emit("notch-hud://summon", false);
            await waitFor(() => expect(ipcMocks.setNotchHudKeyboard).toHaveBeenCalledWith(false));
            await waitFor(() => expect(coreMocks.invoke).toHaveBeenCalledWith("voice_cancel", { sessionId: "v1" }));
        });

        it("keeps Do hidden until Operate my Mac is on", async () => {
            ipcMocks.computerUseStatus.mockResolvedValue({
                enabled: false,
                codexReady: true,
                backend: null,
                backendPath: null,
                activeRun: null,
            });
            render(<NotchHud />);
            await openPanel();
            await waitFor(() => expect(ipcMocks.computerUseStatus).toHaveBeenCalled());
            expect(screen.queryByRole("button", { name: "Do" })).not.toBeInTheDocument();
            expect(coreMocks.invoke).not.toHaveBeenCalledWith("voice_start", expect.anything());
        });

        it("keeps Do hidden when it is on but nothing can click and type", async () => {
            ipcMocks.computerUseStatus.mockResolvedValue({
                enabled: true,
                codexReady: true,
                backend: null,
                backendPath: null,
                activeRun: null,
            });
            render(<NotchHud />);
            await openPanel();
            await waitFor(() => expect(ipcMocks.computerUseStatus).toHaveBeenCalled());
            expect(screen.queryByRole("button", { name: "Do" })).not.toBeInTheDocument();
            expect(coreMocks.invoke).not.toHaveBeenCalledWith("voice_start", expect.anything());
        });

        it("listens on open, ends the utterance after a pause, plans it and starts after the countdown", async () => {
            await openDo();
            voice({ kind: "listening", level: 0.2 });
            voice({ kind: "partial", text: "open Spotify play" });
            expect(await screen.findByText("open Spotify play")).toBeInTheDocument();
            await waitFor(() => expect(coreMocks.invoke).toHaveBeenCalledWith("voice_stop", { sessionId: "v1" }), {
                timeout: 2500,
            });

            voice({ kind: "final", text: REQUEST });
            await waitFor(() => expect(ipcMocks.computerUsePlan).toHaveBeenCalledWith(REQUEST));
            planned();
            const plan = await screen.findByRole("list", { name: "Plan" });
            expect(plan).toHaveTextContent("Open Spotify");
            expect(plan).toHaveTextContent("Search looped transformers");
            expect(ipcMocks.computerUseStart).not.toHaveBeenCalled();
            await waitFor(() => expect(ipcMocks.computerUseStart).toHaveBeenCalledWith("r1"), { timeout: 2500 });

            emit("computer-use://event", { kind: "stepStarted", runId: "r1", index: 1, attempt: 1 });
            expect(screen.getByText("Play Blinding Lights").closest("li")).toHaveAttribute("aria-current", "step");
            emit("computer-use://event", { kind: "stepDone", runId: "r1", index: 1, ok: true, detail: "Playing Blinding Lights" });
            expect(await screen.findByText("Playing Blinding Lights")).toBeInTheDocument();
            emit("computer-use://event", { kind: "finished", runId: "r1", ok: true, summary: "Done: Open Spotify, Play Blinding Lights, Search looped transformers." });
            expect(await screen.findByText(/^Done: Open Spotify/)).toBeInTheDocument();
        });

        it("shows what it heard before sending it, and Cancel keeps it on the Mac", async () => {
            await openDo();
            voice({ kind: "final", text: "open my bank and pay rent" });
            expect(await screen.findByText(/Heard this/)).toBeInTheDocument();
            expect(screen.getByText("“open my bank and pay rent”")).toBeInTheDocument();
            expect(ipcMocks.computerUsePlan).not.toHaveBeenCalled();

            fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
            await new Promise((resolve) => setTimeout(resolve, 1500));
            expect(ipcMocks.computerUsePlan).not.toHaveBeenCalled();
        });

        it("waits for a tap when a step in the plan could need a yes", async () => {
            await openDo();
            voice({ kind: "final", text: REQUEST });
            await waitFor(() => expect(ipcMocks.computerUsePlan).toHaveBeenCalled());
            planned(false);
            expect(await screen.findByText(/Ready\. Tap Start/)).toBeInTheDocument();
            await new Promise((resolve) => setTimeout(resolve, 1800));
            expect(ipcMocks.computerUseStart).not.toHaveBeenCalled();

            fireEvent.click(screen.getByRole("button", { name: "Start" }));
            await waitFor(() => expect(ipcMocks.computerUseStart).toHaveBeenCalledWith("r1"));
        });

        it("saying stop mid-run kills the run before the utterance ends", async () => {
            await openDo();
            voice({ kind: "final", text: REQUEST });
            await waitFor(() => expect(ipcMocks.computerUsePlan).toHaveBeenCalled());
            planned();
            emit("computer-use://event", { kind: "stepStarted", runId: "r1", index: 0, attempt: 1 });
            await waitFor(() => expect(coreMocks.invoke.mock.calls.filter(([c]) => c === "voice_start").length).toBe(2));

            voice({ kind: "partial", text: "stop" });
            await waitFor(() => expect(ipcMocks.computerUseStop).toHaveBeenCalled());
            expect(await screen.findByText("Stopped.")).toBeInTheDocument();
        });

        it("holds mid-run speech as a redirect and cancels the plan card on Cancel", async () => {
            await openDo();
            voice({ kind: "final", text: REQUEST });
            await waitFor(() => expect(ipcMocks.computerUsePlan).toHaveBeenCalled());
            planned();
            fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
            await waitFor(() => expect(ipcMocks.computerUseStop).toHaveBeenCalled());
            expect(ipcMocks.computerUseStart).not.toHaveBeenCalled();
        });

        it("answers an approval with a tap and plans typed requests", async () => {
            await openDo();
            voice({ kind: "final", text: REQUEST });
            await waitFor(() => expect(ipcMocks.computerUsePlan).toHaveBeenCalled());
            planned();
            emit("computer-use://event", { kind: "stepStarted", runId: "r1", index: 1, attempt: 1 });
            emit("computer-use://event", {
                kind: "approval",
                runId: "r1",
                requestKey: "req-2",
                tool: "type_text",
                summary: "type \"hello\" in Notes",
            });
            expect(await screen.findByRole("alertdialog", { name: "Approve action" })).toHaveTextContent(
                'Okay to type "hello" in Notes?',
            );
            fireEvent.click(screen.getByRole("button", { name: "Don't" }));
            await waitFor(() => expect(ipcMocks.computerUseRespond).toHaveBeenCalledWith("req-2", false));

            emit("computer-use://event", {
                kind: "blocked",
                runId: "r1",
                index: 1,
                tool: "click",
                summary: "click \"Buy Premium\" in Spotify",
                reason: "sending, deleting and buying are not allowed",
            });
            expect(await screen.findByText('Refused: click "Buy Premium" in Spotify')).toBeInTheDocument();

            emit("computer-use://event", { kind: "finished", runId: "r1", ok: true, summary: "Done." });
            const typed = screen.getByLabelText("Instruction for FNDR");
            fireEvent.change(typed, { target: { value: "close the window" } });
            fireEvent.submit(typed.closest("form") as HTMLFormElement);
            await waitFor(() => expect(ipcMocks.computerUsePlan).toHaveBeenCalledWith("close the window"));
        });

        it("names a denied microphone and offers its settings", async () => {
            await openDo();
            voice({
                kind: "unavailable",
                reason: "permission_denied",
                message: "Allow the microphone in System Settings.",
                permission: "microphone",
                settingsPane: "microphone",
            });
            expect(await screen.findByText("FNDR can't use the microphone.")).toBeInTheDocument();
            fireEvent.click(screen.getByRole("button", { name: "Open Microphone Settings" }));
            expect(onboardingMocks.openSystemSettings).toHaveBeenCalledWith("microphone");
        });

        it("offers Reconnect ChatGPT when the sign-in is gone", async () => {
            await openDo();
            voice({ kind: "final", text: REQUEST });
            await waitFor(() => expect(ipcMocks.computerUsePlan).toHaveBeenCalled());
            emit("computer-use://event", { kind: "failed", runId: "r1", error: "Sign in with ChatGPT to use Notch Do.", reconnect: true });
            fireEvent.click(await screen.findByRole("button", { name: "Reconnect ChatGPT" }));
            await waitFor(() => expect(ipcMocks.codexLoginStart).toHaveBeenCalled());
        });

        describe("spoken progress", () => {
            class FakeUtterance {
                onend: (() => void) | null = null;
                onerror: (() => void) | null = null;
                constructor(public text: string) {}
            }
            let spoken: FakeUtterance[];
            let current: FakeUtterance | null;
            const synth = {
                get speaking() {
                    return current !== null;
                },
                speak: vi.fn((u: FakeUtterance) => {
                    spoken.push(u);
                    current = u;
                }),
                cancel: vi.fn(() => {
                    const u = current;
                    current = null;
                    u?.onerror?.();
                }),
            };
            const said = () => spoken.map((u) => u.text);

            beforeEach(() => {
                spoken = [];
                current = null;
                synth.speak.mockClear();
                synth.cancel.mockClear();
                vi.stubGlobal("SpeechSynthesisUtterance", FakeUtterance);
                Object.defineProperty(window, "speechSynthesis", { value: synth, configurable: true });
                window.localStorage.removeItem("fndr.notch.do.muted");
            });

            afterEach(() => {
                vi.unstubAllGlobals();
                Reflect.deleteProperty(window, "speechSynthesis");
                window.localStorage.removeItem("fndr.notch.do.muted");
            });

            async function toRunning() {
                await openDo();
                voice({ kind: "final", text: "open Spotify and play Blinding Lights" });
                await waitFor(() => expect(ipcMocks.computerUsePlan).toHaveBeenCalled());
                planned();
            }

            it("says what it understood, then each step as it starts, by default", async () => {
                await toRunning();
                await waitFor(() => expect(said()[0]).toMatch(/^Understood: open Spotify and play Blinding Lights\. three steps\. Starting\.$/));
                emit("computer-use://event", { kind: "stepStarted", runId: "r1", index: 0, attempt: 1 });
                act(() => {
                    current?.onend?.();
                });
                await waitFor(() => expect(said()).toContain("Step one of three: Open Spotify."));
            });

            it("announces an ask-first step aloud while the Allow button waits for a tap", async () => {
                await toRunning();
                emit("computer-use://event", { kind: "stepStarted", runId: "r1", index: 1, attempt: 1 });
                emit("computer-use://event", {
                    kind: "approval",
                    runId: "r1",
                    requestKey: "req-9",
                    tool: "click",
                    summary: "click Add to cart",
                });
                await waitFor(() => expect(said()).toContain("I need your okay to click Add to cart. Tap Allow, or say no."));
                expect(ipcMocks.computerUseRespond).not.toHaveBeenCalled();
                expect(screen.getByRole("button", { name: "Allow" })).toBeInTheDocument();
            });

            it("says what it did not do and how much it checked", async () => {
                await toRunning();
                emit("computer-use://event", { kind: "stepDone", runId: "r1", index: 0, ok: true, detail: "", checked: true });
                emit("computer-use://event", { kind: "finished", runId: "r1", ok: true, summary: "Playing." });
                await waitFor(() => expect(said()[said().length - 1]).toMatch(/^Done\. Playing\. I left out: .* I checked every step myself\./));
            });

            it("goes quiet at once on the Stop button", async () => {
                await toRunning();
                emit("computer-use://event", { kind: "stepStarted", runId: "r1", index: 0, attempt: 1 });
                await waitFor(() => expect(synth.speak).toHaveBeenCalled());
                fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
                expect(synth.cancel).toHaveBeenCalled();
                expect(synth.speaking).toBe(false);
            });

            it("a spoken stop still works while FNDR is talking", async () => {
                await toRunning();
                emit("computer-use://event", { kind: "stepStarted", runId: "r1", index: 0, attempt: 1 });
                await waitFor(() => expect(coreMocks.invoke.mock.calls.filter(([c]) => c === "voice_start").length).toBe(2));
                voice({ kind: "partial", text: "stop" });
                await waitFor(() => expect(ipcMocks.computerUseStop).toHaveBeenCalled());
                expect(synth.speaking).toBe(false);
            });

            it("does not take its own voice for a new request", async () => {
                await toRunning();
                await waitFor(() => expect(synth.speak).toHaveBeenCalled());
                voice({ kind: "final", text: "understood 3 steps starting" });
                await new Promise((resolve) => setTimeout(resolve, 1500));
                expect(screen.queryByText(/Heard this/)).not.toBeInTheDocument();
                expect(ipcMocks.computerUsePlan).toHaveBeenCalledTimes(1);
            });

            it("mute silences speech now and keeps it off, and the switch comes back", async () => {
                await toRunning();
                await waitFor(() => expect(synth.speak).toHaveBeenCalled());
                fireEvent.click(screen.getByRole("button", { name: "Mute voice" }));
                expect(synth.speaking).toBe(false);
                synth.speak.mockClear();
                emit("computer-use://event", { kind: "stepStarted", runId: "r1", index: 0, attempt: 1 });
                await new Promise((resolve) => setTimeout(resolve, 50));
                expect(synth.speak).not.toHaveBeenCalled();
                expect(window.localStorage.getItem("fndr.notch.do.muted")).toBe("1");
                expect(screen.getByRole("button", { name: "Unmute voice" })).toBeInTheDocument();
            });

            it("closing the notch cancels speech", async () => {
                await toRunning();
                await waitFor(() => expect(synth.speak).toHaveBeenCalled());
                synth.cancel.mockClear();
                emit("notch-hud://summon", false);
                await waitFor(() => expect(synth.cancel).toHaveBeenCalled());
                expect(synth.speaking).toBe(false);
            });
        });
    });
});
