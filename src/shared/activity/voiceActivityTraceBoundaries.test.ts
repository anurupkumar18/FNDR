import { describe, expect, it } from "vitest";
import {
    beginVoiceActivityTrace,
    recordVoiceActivityStep,
    type VoiceActivityEvent,
} from "./voiceActivityTrace";

const ALL_EVENTS: VoiceActivityEvent[] = [
    "microphone-requested",
    "microphone-connected",
    "microphone-unavailable",
    "microphone-failed",
    "recording-started",
    "recorder-stop-requested",
    "recording-stopped",
    "recording-too-short",
    "no-audio",
    "transcription-requested",
    "transcript-ready",
    "no-speech",
    "transcription-failed",
];

function active(trace: ReturnType<typeof beginVoiceActivityTrace>): string[] {
    return trace.steps
        .filter((step) => step.status === "running" || step.status === "waiting")
        .map((step) => step.id);
}

describe("voice activity boundaries", () => {
    it("leaves each step running until its own boundary event arrives", () => {
        let trace = beginVoiceActivityTrace("search", 1_000);
        expect(active(trace)).toEqual(["microphone-request"]);

        trace = recordVoiceActivityStep(trace, "microphone-connected", 1_100);
        trace = recordVoiceActivityStep(trace, "recording-started", 1_110);
        // Time passing alone never advances recording or transcription.
        expect(active(trace)).toEqual(["recording"]);
        expect(trace.status).toBe("running");
        expect(trace.finishedAtMs).toBeNull();

        trace = recordVoiceActivityStep(trace, "recorder-stop-requested", 3_000);
        expect(active(trace)).toEqual(["recording", "recorder-stop-request"]);

        trace = recordVoiceActivityStep(trace, "recording-stopped", 3_050);
        trace = recordVoiceActivityStep(trace, "transcription-requested", 3_060);
        expect(active(trace)).toEqual(["transcription-request"]);
        expect(trace.steps.some((step) => step.id === "transcription-result")).toBe(false);
    });

    it.each([
        ["transcript-ready", "completed"],
        ["no-speech", "degraded"],
        ["transcription-failed", "failed"],
    ] as const)("settles the same transcription request step on %s", (event, status) => {
        let trace = beginVoiceActivityTrace("home", 1_000);
        trace = recordVoiceActivityStep(trace, "transcription-requested", 2_000);
        const idsBefore = trace.steps.map((step) => step.id);
        trace = recordVoiceActivityStep(trace, event, 2_400);

        expect(trace.steps.find((step) => step.id === "transcription-request")?.status).toBe(
            event === "transcription-failed" ? "failed" : status,
        );
        expect(trace.steps.at(-1)?.status).toBe(status);
        // The request step is updated in place, never duplicated.
        for (const id of idsBefore) {
            expect(trace.steps.filter((step) => step.id === id)).toHaveLength(1);
        }
        expect(trace.status).toBe(status);
        expect(trace.finishedAtMs).toBe(2_400);
    });

    it.each([
        ["microphone-failed", "failed"],
        ["microphone-unavailable", "failed"],
    ] as const)("fails the same microphone request step on %s", (event, status) => {
        let trace = beginVoiceActivityTrace("notch", 1_000);
        trace = recordVoiceActivityStep(trace, event, 1_300);

        expect(trace.steps.find((step) => step.id === "microphone-request")?.status).toBe(status);
        expect(active(trace)).toEqual([]);
        expect(trace.steps.filter((step) => step.id === "microphone-request")).toHaveLength(1);
    });

    it("stays bounded and keeps one entry per stable id however often events repeat", () => {
        let trace = beginVoiceActivityTrace("search", 0);
        for (let turn = 0; turn < 40; turn += 1) {
            for (const event of ALL_EVENTS) {
                trace = recordVoiceActivityStep(trace, event, turn * 100 + 1);
            }
        }
        const ids = trace.steps.map((step) => step.id);
        expect(new Set(ids).size).toBe(ids.length);
        expect(trace.steps.length).toBeLessThanOrEqual(24);
    });

    it("carries no free text: only closed-vocabulary labels and actors, never detail", () => {
        let trace = beginVoiceActivityTrace("search", 0);
        for (const event of ALL_EVENTS) {
            trace = recordVoiceActivityStep(trace, event, 10);
        }
        for (const step of trace.steps) {
            expect(step).not.toHaveProperty("detail");
            expect(["Microphone", "Media recorder", "Local Whisper"]).toContain(step.actor);
            expect(step.label).toMatch(/^[A-Z][A-Za-z ]+$/);
        }
    });
});
