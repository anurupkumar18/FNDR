import { describe, expect, it } from "vitest";
import {
    CUE_MS,
    CUE_REPEAT_MS,
    classifyUtterance,
    doRunReducer,
    initialDoState,
    isDeaf,
    isStopWord,
    micFor,
    resultNote,
    turnOf,
    workSetSummary,
    type DoInput,
    type DoState,
    type TurnState,
} from "./doRun";
import type { ComputerUseEvent, WorkItem, WorkSet } from "@/shared/ipc/tauri";

const CANVAS: WorkItem = { memoryId: "canvas", label: "Assignment 3", kind: "url", reopenRank: 4, appName: "Google Chrome", host: "canvas.utah.edu", capturedAt: 3 };
const PDF: WorkItem = { memoryId: "pdf", label: "reading.pdf, page 4", kind: "pdf_page", reopenRank: 7, appName: "Preview", page: 4, capturedAt: 2 };
const DOC: WorkItem = { memoryId: "doc", label: "Lab report - Google Docs", kind: "url", reopenRank: 4, appName: "Google Chrome", host: "docs.google.com", capturedAt: 1 };
const set = (id: string, title: string, items: WorkItem[]): WorkSet => ({ id, title, reason: "Its titles mention assignment", score: 1, items });
const BIO = set("bio", "Biology assignment 2", [CANVAS, PDF, DOC]);
const CHEM = set("chem", "Chemistry assignment 4", [CANVAS, PDF, DOC, { ...DOC, memoryId: "sheet", label: "Data" }]);


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

describe("the stop word", () => {
    it("matches stop or cancel alone, behind a lead-in, or with a short tail, up to four words", () => {
        const accepted = [
            "stop",
            "Stop!",
            "cancel",
            "stop it",
            "stop that",
            "stop now",
            "stop please",
            "cancel that please",
            "please stop",
            "okay stop",
            "ok, cancel",
            "no stop",
            "no, stop it",
            "FNDR stop",
            "hey FNDR, cancel",
            "hey fndr stop now",
            "fndr cancel please",
        ];
        for (const phrase of accepted) expect(isStopWord(phrase), phrase).toBe(true);
    });

    it("drops everything else, including sentences that contain stop", () => {
        const rejected = [
            "don't stop",
            "don't stop the music",
            "stop sign",
            "unstoppable",
            "stopped",
            "stops",
            "cancelled",
            "stop at the second tab and open settings",
            "please stop the timer now",
            "okay so stop it now please",
            "hey fndr stop it please",
            "wait",
            "pause",
            "hold on",
            "never mind",
            "nevermind",
            "abort",
            "just stop",
            "stop stop stop stop stop",
            "go",
            "yes",
            "no",
            "",
            "  ...  ",
        ];
        for (const phrase of rejected) expect(isStopWord(phrase), phrase).toBe(false);
    });
});

describe("classifyUtterance", () => {
    it("reads only the stop word and a request; spoken go, yes and no do nothing", () => {
        expect(classifyUtterance("Stop!")).toEqual({ kind: "stop" });
        expect(classifyUtterance("please cancel")).toEqual({ kind: "stop" });
        expect(classifyUtterance("open Spotify")).toEqual({ kind: "request", text: "open Spotify" });
        for (const phrase of ["go", "yes", "no", "okay do it", "allow", "never mind", "wait"]) {
            expect(classifyUtterance(phrase), phrase).toBeNull();
        }
    });

    it("keeps longer requests that merely contain stop words", () => {
        expect(classifyUtterance("stop at the second tab and open settings")).toEqual({
            kind: "request",
            text: "stop at the second tab and open settings",
        });
    });

    it("ignores empty speech", () => {
        expect(classifyUtterance("  ...  ")).toBeNull();
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

    it("holds what was heard as its own step before any plan exists", () => {
        const heard = doRunReducer(initialDoState, { type: "heard", text: "play some jazz" });
        expect(heard.phase).toBe("heard");
        expect(heard.transcript).toBe("play some jazz");
        expect(heard.runId).toBeNull();
        expect(doRunReducer(heard, { type: "stopped" }).phase).toBe("stopped");
    });

    it("says how many steps FNDR saw for itself, and that nothing is undone for the person", () => {
        const step = (checked: boolean) => ({
            label: "x",
            action: "operate" as const,
            app: "Spotify",
            status: "done" as const,
            attempt: 1,
            checked,
        });
        expect(resultNote([step(true), step(true)])).toBe("FNDR checked every step. Nothing here is undone automatically.");
        expect(resultNote([step(true), step(false), step(false)])).toBe(
            "FNDR checked 1 of 3 steps; the rest are as the model reported. Nothing here is undone automatically.",
        );
        expect(resultNote([step(false)])).toMatch(/^FNDR could not check these steps/);
        expect(resultNote([])).toBe("");
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

describe("work sets", () => {
    const offered = () => run({ type: "planRequested", runId: "r1", transcript: "the assignment I was working on" }, event({ kind: "choose", runId: "r1", options: [BIO, CHEM] }));

    it("shows the sets to choose between, and a pick becomes a plan card listing every place", () => {
        const choosing = offered();
        expect(choosing.phase).toBe("choose");
        expect(choosing.options.map((o) => o.title)).toEqual(["Biology assignment 2", "Chemistry assignment 4"]);
        const picked = doRunReducer(choosing, { type: "workSetChosen", set: BIO });
        expect(picked.phase).toBe("plan");
        expect(picked.options).toEqual([]);
        expect(picked.steps.map((s) => s.label)).toEqual(["Open Assignment 3", "Open reading.pdf, page 4", "Open Lab report - Google Docs"]);
        expect(picked.steps.every((s) => s.action === "reopen_memory")).toBe(true);
        expect(picked.autoStart).toBe(true);
        expect(doRunReducer(choosing, { type: "workSetChosen", set: CHEM }).autoStart).toBe(false);
    });

    it("stays on the chooser when listening is asked for, and Stop clears it", () => {
        const choosing = offered();
        expect(doRunReducer(choosing, { type: "listening" })).toBe(choosing);
        const stopped = doRunReducer(choosing, { type: "stopped" });
        expect(stopped.phase).toBe("stopped");
        expect(stopped.options).toEqual([]);
    });

    it("marks each place from what FNDR saw when it opened it", () => {
        const picked = doRunReducer(offered(), { type: "workSetChosen", set: BIO });
        const opening = doRunReducer(picked, { type: "workSetOpening" });
        expect(opening.phase).toBe("running");
        const done = doRunReducer(opening, {
            type: "workSetOpened",
            outcomes: [
                { memoryId: "canvas", label: "Assignment 3", ok: true, detail: "Opened" },
                { memoryId: "pdf", label: "reading.pdf, page 4", ok: false, detail: "reading.pdf is no longer there" },
                { memoryId: "doc", label: "Lab report - Google Docs", ok: true, detail: "Opened" },
            ],
        });
        expect(done.phase).toBe("finished");
        expect(done.steps.map((s) => [s.status, s.checked])).toEqual([["done", true], ["failed", false], ["done", true]]);
        expect(done.result).toEqual({
            ok: false,
            summary: "Opened: Assignment 3, Lab report - Google Docs. Not opened: reading.pdf, page 4 (reading.pdf is no longer there).",
        });
    });

    it("narrows by words, picks the one clear set, or says why there is none", () => {
        expect(doRunReducer(offered(), { type: "workSetResolved", resolution: { kind: "best", value: CHEM } }).phase).toBe("plan");
        expect(doRunReducer(offered(), { type: "workSetResolved", resolution: { kind: "ambiguous", value: [CHEM, BIO] } }).options[0].id).toBe("chem");
        const none = doRunReducer(offered(), { type: "workSetResolved", resolution: { kind: "none", value: { why: "Nothing matches." } } });
        expect([none.phase, none.error]).toEqual(["failed", "Nothing matches."]);
        expect(workSetSummary([{ memoryId: "a", label: "A", ok: true, detail: "Opened" }])).toBe("Opened: A.");
    });
});

describe("turn-taking", () => {
    const planning = (): DoState => run({ type: "planRequested", runId: "r1", transcript: "open Spotify" });
    const working = (): DoState => run({ type: "planRequested", runId: "r1", transcript: "x" }, event(PLANNED), event({ kind: "stepStarted", runId: "r1", index: 0, attempt: 1 }));
    const finished = (): DoState => doRunReducer(working(), event({ kind: "finished", runId: "r1", ok: true, summary: "Done." }));

    it("names every state of the spec from the run's phase", () => {
        const cases: [DoState, TurnState][] = [
            [initialDoState, "idle"],
            [run({ type: "listening" }), "listening"],
            [run({ type: "listening" }, { type: "partial", text: "open" }), "hearing"],
            [run({ type: "heard", text: "open Spotify" }), "thinking"],
            [planning(), "thinking"],
            [doRunReducer(planning(), event({ kind: "choose", runId: "r1", options: [BIO, CHEM] })), "awaiting_choice"],
            [doRunReducer(planning(), event({ ...PLANNED, autoStart: false } as ComputerUseEvent)), "awaiting_start"],
            [doRunReducer(planning(), event({ ...PLANNED, autoStart: true } as ComputerUseEvent)), "working"],
            [working(), "working"],
            [doRunReducer(working(), event({ kind: "approval", runId: "r1", requestKey: "k", tool: "click", summary: "click Buy" })), "awaiting_approval"],
            [finished(), "finishing"],
            [doRunReducer(finished(), { type: "speechStarted" }), "speaking"],
            [doRunReducer(finished(), { type: "readoutDone" }), "idle"],
            [doRunReducer(working(), { type: "stopped" }), "interrupted"],
            [run({ type: "listening" }, { type: "silence" }), "error"],
            [run({ type: "micDenied", message: "x" }), "error"],
            [run({ type: "voiceUnavailable", message: "x" }), "error"],
            [doRunReducer(planning(), { type: "error", message: "offline" }), "error"],
        ];
        for (const [state, turn] of cases) expect(turnOf(state), `${state.phase} -> ${turn}`).toBe(turn);
    });

    it("opens the microphone only to hear a request, listens for the stop word while deaf, and closes it otherwise", () => {
        expect(micFor("idle")).toBe("closed");
        expect(micFor("listening")).toBe("open");
        expect(micFor("hearing")).toBe("open");
        for (const turn of ["thinking", "awaiting_choice", "awaiting_start", "working", "awaiting_approval", "finishing", "speaking"] as TurnState[]) {
            expect(isDeaf(turn), turn).toBe(true);
            expect(micFor(turn), turn).toBe("stop_only");
        }
        for (const turn of ["idle", "listening", "hearing", "interrupted", "error"] as TurnState[]) expect(isDeaf(turn), turn).toBe(false);
        expect(micFor("interrupted")).toBe("closed");
        expect(micFor("error")).toBe("closed");
    });

    it("drops a heard request and partial text in every deaf state", () => {
        const deaf: DoState[] = [
            run({ type: "heard", text: "open Spotify" }),
            planning(),
            doRunReducer(planning(), event({ kind: "choose", runId: "r1", options: [BIO, CHEM] })),
            doRunReducer(planning(), event(PLANNED)),
            working(),
            doRunReducer(working(), event({ kind: "approval", runId: "r1", requestKey: "k", tool: "click", summary: "click Buy" })),
            finished(),
            doRunReducer(finished(), { type: "speechStarted" }),
        ];
        const inputs: DoInput[] = [
            { type: "heard", text: "actually open Music" },
            { type: "partial", text: "actually open" },
        ];
        for (const state of deaf) {
            for (const input of inputs) expect(doRunReducer(state, input), `${turnOf(state)} ${input.type}`).toBe(state);
        }
    });

    it("still takes a request after the readout, after a stop and after an error", () => {
        for (const state of [doRunReducer(finished(), { type: "readoutDone" }), doRunReducer(working(), { type: "stopped" })]) {
            expect(doRunReducer(state, { type: "heard", text: "open Notes" })).toMatchObject({ phase: "heard", transcript: "open Notes" });
        }
    });

    it("cues 'Working on it' for ignored speech while deaf, at most once every six seconds", () => {
        let state = doRunReducer(working(), { type: "ignoredSpeech", at: 1000 });
        expect(state.cue).toBe(true);
        state = doRunReducer(state, { type: "cueEnded" });
        expect(state.cue).toBe(false);
        expect(doRunReducer(state, { type: "ignoredSpeech", at: 1000 + CUE_REPEAT_MS - 1 }).cue).toBe(false);
        expect(doRunReducer(state, { type: "ignoredSpeech", at: 1000 + CUE_REPEAT_MS }).cue).toBe(true);
        expect(CUE_MS).toBe(2500);
        const listening = run({ type: "listening" });
        expect(doRunReducer(listening, { type: "ignoredSpeech", at: 1 })).toBe(listening);
    });

    it("tracks FNDR's own voice without changing the run", () => {
        const speaking = doRunReducer(working(), { type: "speechStarted" });
        expect(speaking.speaking).toBe(true);
        expect(turnOf(speaking)).toBe("working");
        expect(doRunReducer(speaking, { type: "speechEnded" }).speaking).toBe(false);
    });
});
