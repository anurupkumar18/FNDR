import { describe, expect, it } from "vitest";
import { beginVoiceActivityTrace, recordVoiceActivityStep } from "./voiceActivityTrace";

describe("voiceActivityTrace", () => {
    it("starts with only the microphone request that actually occurred", () => {
        const trace = beginVoiceActivityTrace("home", 1_000);

        expect(trace.title).toBe("Voice input activity");
        expect(trace.steps).toEqual([
            expect.objectContaining({
                id: "microphone-request",
                label: "Requesting microphone access",
                evidence: "frontend-event",
                atMs: 1_000,
            }),
        ]);
    });

    it("uses bounded failure categories with no raw detail field", () => {
        const trace = recordVoiceActivityStep(
            beginVoiceActivityTrace("notch", 1_000),
            "transcription-failed",
            1_250,
            250,
        );
        const failure = trace.steps[trace.steps.length - 1];

        expect(failure).toEqual(expect.objectContaining({
            label: "Transcription failed",
            actor: "Local Whisper",
            status: "failed",
            evidence: "ipc-boundary",
            durationMs: 250,
        }));
        expect(failure).not.toHaveProperty("detail");
    });

    it("settles earlier observed phases as the voice workflow advances", () => {
        let trace = beginVoiceActivityTrace("search", 1_000);
        trace = recordVoiceActivityStep(trace, "microphone-connected", 1_050);
        trace = recordVoiceActivityStep(trace, "recording-started", 1_060);
        trace = recordVoiceActivityStep(trace, "recorder-stop-requested", 2_000);
        trace = recordVoiceActivityStep(trace, "recording-stopped", 2_050);
        trace = recordVoiceActivityStep(trace, "transcription-requested", 2_060);
        trace = recordVoiceActivityStep(trace, "transcript-ready", 2_500);

        expect(trace.steps.map((step) => step.label)).toContain("Requesting microphone access");
        expect(trace.steps.filter((step) => step.status === "running" || step.status === "waiting"))
            .toEqual([]);
        expect(trace.steps.find((step) => step.id === "recording"))
            .toEqual(expect.objectContaining({ status: "completed", durationMs: 990 }));
        expect(trace.steps.find((step) => step.id === "transcription-request"))
            .toEqual(expect.objectContaining({ status: "completed", durationMs: 440 }));
    });
});
