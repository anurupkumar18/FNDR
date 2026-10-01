import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HomeHero } from "./HomeHero";

const ipcMocks = vi.hoisted(() => ({
    transcribeVoiceInput: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ({
    transcribeVoiceInput: ipcMocks.transcribeVoiceInput,
}));

vi.mock("@/shared/motion/useReducedMotionSafe", () => ({
    useReducedMotionSafe: () => ({ reduced: true }),
}));

class MockIntersectionObserver {
    observe = vi.fn();
    disconnect = vi.fn();
}

class FakeMediaRecorder {
    static instances: FakeMediaRecorder[] = [];
    static isTypeSupported() {
        return true;
    }

    mimeType = "audio/webm";
    state: RecordingState = "inactive";
    startArgs: unknown[] | null = null;
    ondataavailable: ((event: { data: Blob }) => void) | null = null;
    onstop: (() => void) | null = null;

    constructor() {
        FakeMediaRecorder.instances.push(this);
    }

    start(...args: unknown[]) {
        this.startArgs = args;
        this.state = "recording";
    }

    stop() {
        this.state = "inactive";
        this.ondataavailable?.({ data: new Blob(["audio"], { type: this.mimeType }) });
        this.onstop?.();
    }
}

if (typeof Blob.prototype.arrayBuffer !== "function") {
    Blob.prototype.arrayBuffer = function arrayBuffer(): Promise<ArrayBuffer> {
        return Promise.resolve(new TextEncoder().encode("audio").buffer);
    };
}

describe("HomeHero", () => {
    beforeEach(() => {
        vi.stubGlobal("IntersectionObserver", MockIntersectionObserver);
        FakeMediaRecorder.instances = [];
        ipcMocks.transcribeVoiceInput.mockResolvedValue({ text: "private spoken search" });
    });

    afterEach(() => {
        cleanup();
        vi.unstubAllGlobals();
    });

    it("keeps the landing screen focused on search instead of extra CTA buttons", () => {
        render(
            <HomeHero
                userName="Anurup"
                now={new Date("2026-05-28T22:00:00")}
                greeting="Good Night, Anurup!"
                onHeroSearch={vi.fn()}
            />
        );

        expect(screen.getByRole("search")).toBeInTheDocument();
        expect(screen.getByPlaceholderText("What shall we uncover tonight?")).toBeInTheDocument();
        expect(screen.getByText("Let's dive into your memories.")).toBeInTheDocument();
        expect(screen.getByText(/search saved memories by topic, app, person, or time/i)).toBeInTheDocument();
        expect(screen.queryByText(/scroll to explore/i)).not.toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Enter the reel" })).not.toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Open work mode" })).not.toBeInTheDocument();
    });

    it("gives a useful typed-search fallback when voice capture is unavailable", () => {
        vi.stubGlobal("MediaRecorder", undefined);
        render(
            <HomeHero
                userName="Anurup"
                now={new Date("2026-05-28T22:00:00")}
                onHeroSearch={vi.fn()}
            />
        );

        fireEvent.click(screen.getByRole("button", { name: "Start voice recording" }));

        expect(screen.getByText(
            "Microphone isn't available here. Type your search instead."
        )).not.toHaveAttribute("role");
        expect(screen.getByLabelText("Voice input activity")).toContainElement(
            screen.getByRole("status"),
        );
        expect(screen.getByRole("textbox", { name: "Search your memories" })).toBeEnabled();
    });

    it("records one complete clip instead of requesting time-sliced fragments", async () => {
        vi.stubGlobal("MediaRecorder", FakeMediaRecorder);
        vi.stubGlobal("navigator", Object.assign(Object.create(navigator), {
            mediaDevices: {
                getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [] }),
            },
        }));
        render(
            <HomeHero
                userName="Anurup"
                now={new Date("2026-05-28T22:00:00")}
                onHeroSearch={vi.fn()}
            />
        );

        fireEvent.click(screen.getByRole("button", { name: "Start voice recording" }));

        await waitFor(() => expect(FakeMediaRecorder.instances).toHaveLength(1));
        expect(FakeMediaRecorder.instances[0].startArgs).toEqual([]);
    });

    it("shows only observed, privacy-safe voice activity", async () => {
        vi.stubGlobal("MediaRecorder", FakeMediaRecorder);
        vi.stubGlobal("navigator", Object.assign(Object.create(navigator), {
            mediaDevices: {
                getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [] }),
            },
        }));
        render(
            <HomeHero
                userName="Anurup"
                now={new Date("2026-05-28T22:00:00")}
                onHeroSearch={vi.fn()}
            />
        );

        const startedAt = Date.now();
        const clock = vi.spyOn(Date, "now").mockReturnValue(startedAt);
        fireEvent.click(screen.getByRole("button", { name: "Start voice recording" }));

        await screen.findByText("Recording voice input");
        expect(screen.getByRole("region", { name: "Voice input activity" })).toBeInTheDocument();

        clock.mockReturnValue(startedAt + 1_500);
        fireEvent.click(screen.getByRole("button", { name: "Stop voice recording" }));

        await screen.findByText("Transcript ready");
        fireEvent.click(screen.getByRole("button", { name: "Show Voice input activity details" }));
        expect(screen.getByText("Requesting microphone access")).toBeInTheDocument();
        expect(screen.getByText("Microphone connected")).toBeInTheDocument();
        expect(screen.getByText("Recording stopped")).toBeInTheDocument();
        expect(screen.getByText("Transcribing on this Mac")).toBeInTheDocument();
        expect(screen.queryByText("private spoken search")).not.toBeInTheDocument();
        clock.mockRestore();
    });
});
