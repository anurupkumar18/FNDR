import { describe, expect, it } from "vitest";
import { VOICE_RECORDING } from "@/shared/utils/config";
import {
    initialScreenGuideState,
    isLongEnoughVoiceClip,
    recordScreenGuideActivity,
    recordScreenGuideFrontendActivity,
    finishScreenGuideActivity,
    screenGuideActivityCopy,
    screenGuideReducer,
    toScreenGuideHistory,
} from "../screenGuideState";

describe("screenGuideState", () => {
    it("uses only the typed stage and process name for activity copy", () => {
        expect(screenGuideActivityCopy({
            stage: "verifying_target",
            targetApp: "Google Chrome",
        })).toBe("Verifying the target window");
        expect(screenGuideActivityCopy({
            stage: "reading_text",
            targetApp: "must-not-appear",
        })).toBe("Reading visible text");
        expect(screenGuideActivityCopy({
            stage: "answering_on_device",
            targetApp: null,
        }, true)).toBe("Stopped while answering with the on-device model");
        expect(screenGuideActivityCopy({
            stage: "checking_on_device_model",
            targetApp: null,
        })).toBe("Checking on-device model availability");
        expect(screenGuideActivityCopy({
            stage: "using_grounded_fallback",
            targetApp: null,
        })).toBe("Using OCR-grounded fallback");
        expect(screenGuideActivityCopy({
            stage: "speech_started",
            targetApp: null,
        })).toBe("Started macOS speech");
    });

    it("keeps model availability, fallback, and speech launch as distinct observed stages", () => {
        let trace = recordScreenGuideActivity(null, {
            stage: "checking_on_device_model",
            targetApp: null,
            generation: 12,
            atMs: 1_000,
        });
        trace = recordScreenGuideActivity(trace, {
            stage: "using_grounded_fallback",
            targetApp: null,
            generation: 12,
            atMs: 1_050,
        });
        trace = recordScreenGuideActivity(trace, {
            stage: "speech_started",
            targetApp: null,
            generation: 12,
            atMs: 1_100,
        });

        expect(trace.steps).toMatchObject([
            {
                id: "checking_on_device_model",
                label: "Checking on-device model availability",
                actor: "FNDR model runtime",
                status: "completed",
            },
            {
                id: "using_grounded_fallback",
                label: "Using OCR-grounded fallback",
                actor: "FNDR grounding",
                status: "completed",
            },
            {
                id: "speech_started",
                label: "Started macOS speech",
                actor: "macOS speech",
                status: "completed",
            },
        ]);
    });

    it("records microphone, recording, and transcription boundaries without content", () => {
        let trace = recordScreenGuideFrontendActivity(null, {
            stage: "microphone_access",
            generation: 7,
            status: "running",
            atMs: 1_000,
        });
        trace = recordScreenGuideFrontendActivity(trace, {
            stage: "microphone_access",
            generation: 7,
            status: "completed",
            atMs: 1_025,
        });
        trace = recordScreenGuideFrontendActivity(trace, {
            stage: "voice_recording",
            generation: 7,
            status: "running",
            atMs: 1_030,
        });
        trace = recordScreenGuideFrontendActivity(trace, {
            stage: "voice_recording",
            generation: 7,
            status: "completed",
            atMs: 1_430,
        });
        trace = recordScreenGuideFrontendActivity(trace, {
            stage: "voice_transcription",
            generation: 7,
            status: "running",
            atMs: 1_440,
        });
        trace = recordScreenGuideFrontendActivity(trace, {
            stage: "voice_transcription",
            generation: 7,
            status: "completed",
            atMs: 1_700,
        });

        expect(trace.steps).toMatchObject([
            { id: "microphone_access", label: "Microphone connected", status: "completed" },
            { id: "voice_recording", label: "Recording stopped", status: "completed" },
            { id: "voice_transcription", label: "Transcription ready", status: "completed" },
        ]);
        expect(JSON.stringify(trace)).not.toContain("question");
        expect(JSON.stringify(trace)).not.toContain("transcript text");
    });

    it("keeps the ordered observed stages for one turn and fails only the active stage", () => {
        let trace = recordScreenGuideActivity(null, {
            stage: "preparing",
            targetApp: null,
            generation: 9,
            atMs: 1_000,
        });
        trace = recordScreenGuideActivity(trace, {
            stage: "hiding_fndr",
            targetApp: null,
            generation: 9,
            atMs: 1_120,
        });
        trace = recordScreenGuideActivity(trace, {
            stage: "verifying_target",
            targetApp: "Google Chrome",
            generation: 9,
            atMs: 1_240,
        });

        expect(trace.steps.map(({ id, status }) => ({ id, status }))).toEqual([
            { id: "preparing", status: "completed" },
            { id: "hiding_fndr", status: "completed" },
            { id: "verifying_target", status: "running" },
        ]);
        expect(trace.steps[1]?.durationMs).toBe(120);

        trace = finishScreenGuideActivity(trace, "failed", 1_360)!;
        expect(trace.steps).toHaveLength(3);
        expect(trace.steps[2]).toMatchObject({
            id: "verifying_target",
            status: "failed",
            label: "Stopped while verifying the target window",
            durationMs: 120,
        });
    });

    it("resets the trace when a newer Screen Guide generation starts", () => {
        const first = recordScreenGuideActivity(null, {
            stage: "preparing",
            targetApp: null,
            generation: 1,
            atMs: 1_000,
        });
        const second = recordScreenGuideActivity(first, {
            stage: "preparing",
            targetApp: null,
            generation: 2,
            atMs: 2_000,
        });

        expect(second.id).toBe("screen-guide-2");
        expect(second.steps).toHaveLength(1);
        expect(second.startedAtMs).toBe(2_000);
    });

    it("uses processing copy that is truthful for both screen and file questions", () => {
        const state = screenGuideReducer(initialScreenGuideState, {
            type: "thinking",
            question: "Find my I-20 document",
        });

        expect(state.message).toBe("Finding the answer on this Mac…");
    });

    it("moves from listening to an answer and keeps only the latest ten exchanges", () => {
        let state = screenGuideReducer(initialScreenGuideState, { type: "listening" });
        expect(state.phase).toBe("listening");

        state = screenGuideReducer(state, { type: "transcribing" });
        expect(state.phase).toBe("transcribing");

        for (let index = 0; index < 11; index += 1) {
            const question = `Question ${index}`;
            state = screenGuideReducer(state, { type: "thinking", question });
            state = screenGuideReducer(state, {
                type: "answered",
                question,
                response: {
                    answer: `Answer ${index}`,
                    point_cue: index === 10 ? { x: 0.75, y: 0.25, label: "Continue" } : null,
                },
            });
        }

        expect(state.phase).toBe("answer");
        expect(state.answer).toBe("Answer 10");
        expect(state.pointCue).toEqual({ x: 0.75, y: 0.25, label: "Continue" });
        expect(state.exchanges).toHaveLength(10);
        expect(state.exchanges[0].question).toBe("Question 1");
        expect(toScreenGuideHistory(state.exchanges)).toEqual([
            { role: "user", content: "Question 1" },
            { role: "assistant", content: "Answer 1" },
            { role: "user", content: "Question 2" },
            { role: "assistant", content: "Answer 2" },
            { role: "user", content: "Question 3" },
            { role: "assistant", content: "Answer 3" },
            { role: "user", content: "Question 4" },
            { role: "assistant", content: "Answer 4" },
            { role: "user", content: "Question 5" },
            { role: "assistant", content: "Answer 5" },
            { role: "user", content: "Question 6" },
            { role: "assistant", content: "Answer 6" },
            { role: "user", content: "Question 7" },
            { role: "assistant", content: "Answer 7" },
            { role: "user", content: "Question 8" },
            { role: "assistant", content: "Answer 8" },
            { role: "user", content: "Question 9" },
            { role: "assistant", content: "Answer 9" },
            { role: "user", content: "Question 10" },
            { role: "assistant", content: "Answer 10" },
        ]);
    });

    it("uses the shared voice threshold and exposes errors without retaining a stale cue", () => {
        expect(isLongEnoughVoiceClip(100, 100 + VOICE_RECORDING.minDurationMs - 1)).toBe(false);
        expect(isLongEnoughVoiceClip(100, 100 + VOICE_RECORDING.minDurationMs)).toBe(true);

        const answered = screenGuideReducer(initialScreenGuideState, {
            type: "answered",
            question: "Where next?",
            response: {
                answer: "Open settings.",
                point_cue: { x: 0.5, y: 0.5, label: "Settings" },
            },
        });
        const failed = screenGuideReducer(answered, {
            type: "failed",
            message: "Microphone access failed.",
        });

        expect(failed.phase).toBe("error");
        expect(failed.message).toBe("Microphone access failed.");
        expect(failed.pointCue).toBeNull();
    });
});
