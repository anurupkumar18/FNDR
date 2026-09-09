import { describe, expect, it } from "vitest";
import { VOICE_RECORDING } from "@/shared/utils/config";
import {
    initialScreenGuideState,
    isLongEnoughVoiceClip,
    screenGuideReducer,
    toScreenGuideHistory,
} from "../screenGuideState";

describe("screenGuideState", () => {
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
