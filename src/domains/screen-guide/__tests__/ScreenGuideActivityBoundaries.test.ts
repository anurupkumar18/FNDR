import { describe, expect, it } from "vitest";
import {
    finishScreenGuideActivity,
    recordScreenGuideActivity,
    recordScreenGuideFrontendActivity,
    screenGuideActivityTrace,
} from "../screenGuideState";
import type { ScreenGuideActivityStage } from "@/shared/ipc/tauri";

const ALL_STAGES: ScreenGuideActivityStage[] = [
    "preparing",
    "searching_file_names",
    "hiding_fndr",
    "verifying_target",
    "capturing",
    "reading_text",
    "checking_on_device_model",
    "answering_on_device",
    "using_grounded_fallback",
    "answering_chat_gpt",
    "speech_started",
];

const SENSITIVE_APP = "Quarterly-Board-Deck.key - /Users/alex/secret https://internal.example";

function stage(stage: ScreenGuideActivityStage, atMs: number, generation = 7) {
    return { stage, targetApp: SENSITIVE_APP, generation, atMs };
}

describe("Screen Guide activity boundaries", () => {
    it("keeps the newest stage running until the backend emits a later stage", () => {
        let trace = recordScreenGuideActivity(null, stage("capturing", 1_000));
        // No event arrives for a long time; nothing advances on a timer.
        expect(trace.steps.map((step) => [step.id, step.status])).toEqual([["capturing", "running"]]);
        expect(trace.status).toBe("running");

        trace = recordScreenGuideActivity(trace, stage("reading_text", 4_000));
        expect(trace.steps.map((step) => [step.id, step.status])).toEqual([
            ["capturing", "completed"],
            ["reading_text", "running"],
        ]);
        expect(trace.steps[0].durationMs).toBe(3_000);
    });

    it("repeats of a running stage keep one step and its original start time", () => {
        let trace = recordScreenGuideActivity(null, stage("reading_text", 1_000));
        trace = recordScreenGuideActivity(trace, stage("reading_text", 2_500));

        expect(trace.steps).toHaveLength(1);
        expect(trace.steps[0]).toEqual(expect.objectContaining({ id: "reading_text", atMs: 1_000 }));
    });

    it("fails the same stable step that was running, and keeps earlier stages completed", () => {
        let trace = recordScreenGuideActivity(null, stage("capturing", 1_000));
        trace = recordScreenGuideActivity(trace, stage("answering_chat_gpt", 2_000));
        const failed = finishScreenGuideActivity(trace, "failed", 2_900)!;

        expect(failed.steps.map((step) => [step.id, step.status])).toEqual([
            ["capturing", "completed"],
            ["answering_chat_gpt", "failed"],
        ]);
        expect(failed.status).toBe("failed");
        expect(failed.steps[failed.steps.length - 1]?.label).toBe("Stopped while answering with ChatGPT");
        expect(failed.steps[failed.steps.length - 1]?.durationMs).toBe(900);
    });

    it("cancels the same stable step without inventing a stage", () => {
        let trace = recordScreenGuideActivity(null, stage("hiding_fndr", 1_000));
        trace = recordScreenGuideActivity(trace, stage("capturing", 1_200));
        const cancelled = finishScreenGuideActivity(trace, "cancelled", 1_500)!;

        expect(cancelled.steps.map((step) => [step.id, step.status])).toEqual([
            ["hiding_fndr", "completed"],
            ["capturing", "cancelled"],
        ]);
        expect(cancelled.status).toBe("cancelled");
        expect(cancelled.finishedAtMs).toBe(1_500);
    });

    it("lets a newer generation supersede an unfinished trace and leaves no old step running", () => {
        const old = recordScreenGuideActivity(null, stage("capturing", 1_000, 7));
        const next = recordScreenGuideActivity(old, stage("preparing", 2_000, 8));

        expect(next.id).toBe("screen-guide-8");
        expect(next.steps.map((step) => step.id)).toEqual(["preparing"]);
        expect(JSON.stringify(next)).not.toContain("capturing");
    });

    it("marks an error-phase trace failed on the last emitted stage only", () => {
        const failed = screenGuideActivityTrace(
            { ...stage("reading_text", 1_000), failedAtMs: 1_800 } as never,
            "error",
        );
        expect(failed.steps).toHaveLength(1);
        expect(failed.steps[0]).toEqual(expect.objectContaining({ id: "reading_text", status: "failed" }));
    });

    it("keeps frontend voice boundaries running until the browser reports them", () => {
        let trace = recordScreenGuideFrontendActivity(null, {
            stage: "microphone_access",
            generation: 3,
            status: "running",
            atMs: 1_000,
        });
        expect(trace.steps.map((step) => [step.id, step.status])).toEqual([["microphone_access", "running"]]);

        trace = recordScreenGuideFrontendActivity(trace, {
            stage: "microphone_access",
            generation: 3,
            status: "completed",
            atMs: 1_400,
        });
        trace = recordScreenGuideFrontendActivity(trace, {
            stage: "voice_recording",
            generation: 3,
            status: "running",
            atMs: 1_410,
        });
        trace = recordScreenGuideFrontendActivity(trace, {
            stage: "voice_recording",
            generation: 3,
            status: "completed",
            atMs: 3_000,
        });
        trace = recordScreenGuideFrontendActivity(trace, {
            stage: "voice_transcription",
            generation: 3,
            status: "running",
            atMs: 3_010,
        });

        expect(trace.steps.filter((step) => step.status === "running").map((step) => step.id))
            .toEqual(["voice_transcription"]);
        expect(trace.steps.find((step) => step.id === "voice_transcription")?.evidence).toBe("ipc-boundary");

        // The native turn continues the same generation and settles transcription.
        trace = recordScreenGuideActivity(trace, stage("preparing", 4_000, 3));
        expect(trace.id).toBe("screen-guide-3");
        expect(trace.steps.find((step) => step.id === "voice_transcription")?.status).toBe("completed");
    });

    it("claims a named process only for the stage whose backend event named it", () => {
        for (const id of ALL_STAGES) {
            const trace = recordScreenGuideActivity(null, stage(id, 100));
            const step = trace.steps[0];
            expect(step.evidence).toBe("backend-event");
            expect(step.actor.length).toBeGreaterThan(0);
        }
        const chatGpt = recordScreenGuideActivity(null, stage("answering_chat_gpt", 100)).steps[0];
        expect(chatGpt.actor).toBe("ChatGPT");
        const local = recordScreenGuideActivity(null, stage("answering_on_device", 100)).steps[0];
        expect(local.actor).toBe("FNDR on-device model");
    });

    it("never copies the target app, window title, URL, or path into a step", () => {
        let trace = null as ReturnType<typeof recordScreenGuideActivity> | null;
        ALL_STAGES.forEach((id, index) => {
            trace = recordScreenGuideActivity(trace, stage(id, 100 + index));
        });
        const failed = finishScreenGuideActivity(trace, "failed", 999)!;
        const text = JSON.stringify(failed);
        for (const forbidden of ["Quarterly", "Board-Deck", "/Users", "internal.example", "secret"]) {
            expect(text).not.toContain(forbidden);
        }
        for (const step of failed.steps) expect(step).not.toHaveProperty("detail");
    });
});
