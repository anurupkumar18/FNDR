import { describe, expect, it } from "vitest";
import { classifyUtterance, doRunReducer, initialDoState, isStopPhrase, type DoState } from "./doRun";
import type { ComputerUseEvent } from "@/shared/ipc/tauri";

const PLANNED: ComputerUseEvent = {
    kind: "planned",
    runId: "r1",
    steps: [
        { label: "Open Spotify", action: "open_app", app: "Spotify" },
        { label: "Play Blinding Lights", action: "operate", app: "Spotify" },
        { label: "Search looped transformers", action: "open_url", app: "" },
    ],
};

function run(...inputs: Parameters<typeof doRunReducer>[1][]): DoState {
    return inputs.reduce(doRunReducer, initialDoState);
}

const event = (e: ComputerUseEvent) => ({ type: "event" as const, event: e });

describe("classifyUtterance", () => {
    it("treats short stop phrases as stop at any time", () => {
        expect(classifyUtterance("Stop!", {})).toEqual({ kind: "stop" });
        expect(classifyUtterance("never mind", { running: true })).toEqual({ kind: "stop" });
    });

    it("hears stop behind a polite or hurried lead-in", () => {
        for (const phrase of ["please stop now", "ok stop", "no wait", "hey, cancel"]) {
            expect(classifyUtterance(phrase, { running: true }), phrase).toEqual({ kind: "stop" });
            expect(isStopPhrase(phrase), phrase).toBe(true);
        }
        expect(isStopPhrase("don't stop the music")).toBe(false);
        expect(classifyUtterance("no", { awaitingApproval: true })).toEqual({ kind: "decline" });
    });

    it("keeps longer requests that merely contain stop words", () => {
        expect(classifyUtterance("stop at the second tab and open settings", {})).toEqual({
            kind: "request",
            text: "stop at the second tab and open settings",
        });
    });

    it("reads go only while a plan or redirect waits, and yes or no only while an approval waits", () => {
        expect(classifyUtterance("go", { awaitingStart: true })).toEqual({ kind: "go" });
        expect(classifyUtterance("go", {})).toEqual({ kind: "request", text: "go" });
        expect(classifyUtterance("okay do it", { awaitingApproval: true })).toEqual({ kind: "approve" });
        expect(classifyUtterance("nope", { awaitingApproval: true })).toEqual({ kind: "decline" });
    });

    it("ignores empty speech", () => {
        expect(classifyUtterance("  ...  ", {})).toBeNull();
    });
});

describe("doRunReducer", () => {
    it("walks the example run: heard, planned, three steps, done", () => {
        let state = run(
            { type: "listening" },
            { type: "partial", text: "open Spotify play" },
            { type: "planRequested", runId: "r1", transcript: "open Spotify, play Blinding Lights, then look up looped transformers" },
        );
        expect(state.phase).toBe("planning");
        expect(state.transcript).toContain("looped transformers");
        expect(state.partial).toBe("");

        state = doRunReducer(state, event(PLANNED));
        expect(state.phase).toBe("plan");
        expect(state.steps.map((s) => s.status)).toEqual(["pending", "pending", "pending"]);

        state = doRunReducer(state, event({ kind: "stepStarted", runId: "r1", index: 0, attempt: 1 }));
        expect(state.phase).toBe("running");
        expect(state.current).toBe(0);
        state = doRunReducer(state, event({ kind: "stepDone", runId: "r1", index: 0, ok: true, detail: "Spotify is open" }));
        state = doRunReducer(state, event({ kind: "stepStarted", runId: "r1", index: 1, attempt: 2 }));
        expect(state.steps[1]).toMatchObject({ status: "running", attempt: 2 });
        state = doRunReducer(state, event({ kind: "stepDone", runId: "r1", index: 1, ok: true, detail: "Playing Blinding Lights" }));
        state = doRunReducer(state, event({ kind: "stepDone", runId: "r1", index: 2, ok: true, detail: "The page is open" }));
        state = doRunReducer(state, event({ kind: "finished", runId: "r1", ok: true, summary: "Done." }));
        expect(state.phase).toBe("finished");
        expect(state.steps.map((s) => s.status)).toEqual(["done", "done", "done"]);
        expect(state.result).toEqual({ ok: true, summary: "Done." });
    });

    it("ignores events from a run that was replaced", () => {
        const state = run(
            { type: "planRequested", runId: "r2", transcript: "open Notes" },
            event(PLANNED),
        );
        expect(state.phase).toBe("planning");
        expect(state.steps).toEqual([]);
    });

    it("marks a failed step and keeps the reason", () => {
        const state = run(
            { type: "planRequested", runId: "r1", transcript: "x" },
            event(PLANNED),
            event({ kind: "stepStarted", runId: "r1", index: 1, attempt: 1 }),
            event({ kind: "stepDone", runId: "r1", index: 1, ok: false, detail: "Nothing is playing" }),
            event({ kind: "finished", runId: "r1", ok: false, summary: "Stopped at \"Play\": Nothing is playing" }),
        );
        expect(state.steps[1]).toMatchObject({ status: "failed", detail: "Nothing is playing" });
        expect(state.phase).toBe("finished");
        expect(state.result?.ok).toBe(false);
    });

    it("shows blocked actions and clears an approval once resolved", () => {
        let state = run(
            { type: "planRequested", runId: "r1", transcript: "x" },
            event(PLANNED),
            event({ kind: "stepStarted", runId: "r1", index: 1, attempt: 1 }),
            event({ kind: "blocked", runId: "r1", index: 1, tool: "click", summary: "click \"Buy Premium\"", reason: "sending, deleting and buying are not allowed" }),
            event({ kind: "approval", runId: "r1", requestKey: "k1", tool: "type_text", summary: "type \"hi\" in Notes" }),
        );
        expect(state.actions[0]).toMatchObject({ state: "blocked", summary: "click \"Buy Premium\"" });
        expect(state.approval).toEqual({ requestKey: "k1", summary: "type \"hi\" in Notes" });
        state = doRunReducer(state, event({ kind: "approvalResolved", runId: "r1", requestKey: "k1" }));
        expect(state.approval).toBeNull();
    });

    it("stops at once and drops a pending approval", () => {
        const state = run(
            { type: "planRequested", runId: "r1", transcript: "x" },
            event(PLANNED),
            event({ kind: "approval", runId: "r1", requestKey: "k1", tool: "drag", summary: "drag" }),
            { type: "stopped" },
        );
        expect(state.phase).toBe("stopped");
        expect(state.approval).toBeNull();
        expect(state.runId).toBeNull();
    });

    it("holds speech heard mid-run as a redirect until confirmed", () => {
        let state = run(
            { type: "planRequested", runId: "r1", transcript: "x" },
            event(PLANNED),
            event({ kind: "stepStarted", runId: "r1", index: 0, attempt: 1 }),
            { type: "redirectHeard", text: "actually open Music" },
        );
        expect(state.phase).toBe("running");
        expect(state.redirect).toBe("actually open Music");
        state = doRunReducer(state, { type: "redirectDismissed" });
        expect(state.redirect).toBeNull();
    });

    it("names silence, a denied microphone and a lost sign-in as their own states", () => {
        expect(run({ type: "listening" }, { type: "silence" }).phase).toBe("silence");
        expect(run({ type: "micDenied", message: "Allow the microphone" })).toMatchObject({ phase: "mic_denied", error: "Allow the microphone" });
        const signedOut = run(
            { type: "planRequested", runId: "r1", transcript: "x" },
            event({ kind: "failed", runId: "r1", error: "401 Unauthorized", reconnect: true }),
        );
        expect(signedOut).toMatchObject({ phase: "failed", reconnect: true });
    });
});
