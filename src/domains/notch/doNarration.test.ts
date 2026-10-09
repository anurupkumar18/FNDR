import { describe, expect, it } from "vitest";
import { doRunReducer, initialDoState, type DoInput, type DoState } from "./doRun";
import { isEchoOfSpeech, narrate } from "./doNarration";
import type { ComputerUseEvent } from "@/shared/ipc/tauri";

const event = (e: ComputerUseEvent): DoInput => ({ type: "event", event: e });

const STEPS = [
    { label: "Open Spotify", action: "open_app" as const, app: "Spotify" },
    { label: "Play Blinding Lights", action: "operate" as const, app: "Spotify" },
];

/** Applies inputs one by one and returns what would be said after each. */
function lines(...inputs: DoInput[]) {
    let state: DoState = initialDoState;
    return inputs.map((input) => {
        const next = doRunReducer(state, input);
        const said = narrate(state, next);
        state = next;
        return said;
    });
}

const start: DoInput[] = [
    { type: "planRequested", runId: "r1", transcript: "open Spotify and play Blinding Lights" },
    event({ kind: "planned", runId: "r1", autoStart: true, steps: STEPS }),
];

describe("narrate", () => {
    it("says nothing while listening, hearing or planning", () => {
        expect(lines({ type: "listening" }, { type: "partial", text: "open" })).toEqual([null, null]);
        expect(lines({ type: "heard", text: "open Spotify" })).toEqual([null]);
        expect(lines({ type: "planRequested", runId: "r1", transcript: "x" }, event({ kind: "planning", runId: "r1", usedMemories: 2 }))).toEqual([
            null,
            null,
        ]);
    });

    it("says what it understood and that it is starting when the plan can run by itself", () => {
        const said = lines(...start)[1];
        expect(said?.text).toBe("Understood: open Spotify and play Blinding Lights. two steps. Starting.");
        expect(said?.urgent).toBe(false);
    });

    it("tells the person to tap Start when the plan can need a yes", () => {
        const said = lines(start[0], event({ kind: "planned", runId: "r1", autoStart: false, steps: STEPS }))[1];
        expect(said?.text).toContain("Tap Start when you are ready.");
        expect(said?.text).toContain("Some steps may ask first.");
    });

    it("says each step as it starts, and a retry as a retry", () => {
        const said = lines(...start, event({ kind: "stepStarted", runId: "r1", index: 1, attempt: 1 }), event({ kind: "stepStarted", runId: "r1", index: 1, attempt: 2 }));
        expect(said[2]?.text).toBe("Step two of two: Play Blinding Lights.");
        expect(said[3]?.text).toBe("Trying that step again.");
    });

    it("announces an approval aloud as urgent and offers a spoken no, never a spoken yes", () => {
        const said = lines(...start, event({ kind: "approval", runId: "r1", requestKey: "k", tool: "click", summary: "click Add to cart" }))[2];
        expect(said?.urgent).toBe(true);
        expect(said?.text).toBe("I need your okay to click Add to cart. Tap Allow, or say no.");
    });

    it("says what it refused to do", () => {
        const said = lines(...start, event({ kind: "blocked", runId: "r1", index: 0, tool: "type", summary: "type into a password field", reason: "never" }))[2];
        expect(said?.text).toBe("I left this out: type into a password field. FNDR does not do that.");
        expect(said?.urgent).toBe(true);
    });

    it("says a failed step and why, shortened", () => {
        const long = "x".repeat(400);
        const said = lines(...start, event({ kind: "stepDone", runId: "r1", index: 0, ok: false, detail: long }))[2];
        expect(said?.text.startsWith("That step did not work: ")).toBe(true);
        expect(said!.text.length).toBeLessThan(200);
    });

    it("finishes by saying how much FNDR checked itself and what the model only reported", () => {
        const allChecked = lines(
            ...start,
            event({ kind: "stepDone", runId: "r1", index: 0, ok: true, detail: "", checked: true }),
            event({ kind: "stepDone", runId: "r1", index: 1, ok: true, detail: "", checked: true }),
            event({ kind: "finished", runId: "r1", ok: true, summary: "Playing Blinding Lights." }),
        )[4];
        expect(allChecked?.text).toBe("Done. Playing Blinding Lights. I checked every step myself. Nothing is undone automatically.");

        const reported = lines(
            ...start,
            event({ kind: "stepDone", runId: "r1", index: 0, ok: true, detail: "", checked: true }),
            event({ kind: "stepDone", runId: "r1", index: 1, ok: true, detail: "" }),
            event({ kind: "finished", runId: "r1", ok: true, summary: "Playing." }),
        )[4];
        expect(reported?.text).toContain("I checked one of two steps myself; the rest are as the model reported.");

        const none = lines(
            ...start,
            event({ kind: "stepDone", runId: "r1", index: 0, ok: true, detail: "" }),
            event({ kind: "finished", runId: "r1", ok: true, summary: "Opened." }),
        )[3];
        expect(none?.text).toContain("I could not check these steps; they are as the model reported.");
    });

    it("names the steps it did not get to, and does not call an unfinished run done", () => {
        const said = lines(
            ...start,
            event({ kind: "stepDone", runId: "r1", index: 0, ok: true, detail: "", checked: true }),
            event({ kind: "finished", runId: "r1", ok: false, summary: "Stopped at the second step." }),
        )[3];
        expect(said?.text.startsWith("I could not finish. Stopped at the second step.")).toBe(true);
        expect(said?.text).toContain("I left out: Play Blinding Lights.");
        expect(said?.text).not.toContain("Done.");
    });

    it("says a failure, and nothing on a stop", () => {
        const failed = lines(...start, event({ kind: "failed", runId: "r1", error: "ChatGPT sign-in expired", reconnect: true }))[2];
        expect(failed?.text).toBe("I could not do that. ChatGPT sign-in expired");
        const stopped = lines(...start, { type: "stopped" })[2];
        expect(stopped).toBeNull();
    });

    it("speaks a typed or tool error once, without repeating it for the same state", () => {
        const first = lines(...start, { type: "error", message: "Codex is not signed in." });
        expect(first[2]?.text).toBe("I could not do that. Codex is not signed in.");
        const state = doRunReducer(initialDoState, { type: "error", message: "x" });
        expect(narrate(state, state)).toBeNull();
    });
});

describe("isEchoOfSpeech", () => {
    const spoken = "Understood: open Spotify and play Blinding Lights. two steps. Starting.";

    it("recognizes the notch hearing its own voice", () => {
        expect(isEchoOfSpeech("open Spotify and play blinding lights", spoken)).toBe(true);
        expect(isEchoOfSpeech("Understood open Spotify", spoken)).toBe(true);
        expect(isEchoOfSpeech("2 steps starting", spoken)).toBe(true);
    });

    it("keeps real speech that is not what was just said", () => {
        expect(isEchoOfSpeech("open the browser and look up cats", spoken)).toBe(false);
        expect(isEchoOfSpeech("", spoken)).toBe(false);
        expect(isEchoOfSpeech("anything", "")).toBe(false);
    });

    it("never swallows a stop", () => {
        expect(isEchoOfSpeech("stop", "I will stop here. Stop is easy.")).toBe(false);
        expect(isEchoOfSpeech("please cancel", "please cancel this")).toBe(false);
    });
});
