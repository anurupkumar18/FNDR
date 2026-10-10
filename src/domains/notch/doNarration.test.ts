import { describe, expect, it } from "vitest";
import { doRunReducer, initialDoState, isStopWord, type DoInput, type DoState } from "./doRun";
import { isEchoOfSpeech, narrate, speakable, spokenItem } from "./doNarration";
import type { ComputerUseEvent, WorkItem, WorkSet } from "@/shared/ipc/tauri";

const CANVAS: WorkItem = { memoryId: "canvas", label: "Assignment 3", kind: "url", reopenRank: 4, appName: "Google Chrome", host: "canvas.utah.edu", capturedAt: 3 };
const PDF: WorkItem = { memoryId: "pdf", label: "reading.pdf, page 4", kind: "pdf_page", reopenRank: 7, appName: "Preview", page: 4, capturedAt: 2 };
const DOC: WorkItem = { memoryId: "doc", label: "Lab report - Google Docs", kind: "url", reopenRank: 4, appName: "Google Chrome", host: "docs.google.com", capturedAt: 1 };
const set = (id: string, title: string, items: WorkItem[]): WorkSet => ({ id, title, reason: "Its titles mention assignment", score: 1, items });
const BIO = set("bio", "Biology assignment 2", [CANVAS, PDF, DOC]);
const CHEM = set("chem", "Chemistry assignment 4", [CANVAS, PDF, DOC, { ...DOC, memoryId: "sheet", label: "Data" }]);


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

    it("announces an approval aloud as urgent and asks for a tap, never a spoken answer", () => {
        const said = lines(...start, event({ kind: "approval", runId: "r1", requestKey: "k", tool: "click", summary: "click Add to cart" }))[2];
        expect(said?.urgent).toBe(true);
        expect(said?.text).toBe("I need your okay to click Add to cart. Tap Allow.");
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

describe("narrating a work set", () => {
    const reopen = (item: WorkItem) => ({ label: `Open ${item.label}`, action: "reopen_memory" as const, app: item.appName, item });

    it("says what is opening in plain words", () => {
        const said = lines(
            { type: "planRequested", runId: "r1", transcript: "pull up the assignment" },
            event({ kind: "planned", runId: "r1", autoStart: true, steps: [CANVAS, PDF, DOC].map(reopen) }),
        )[1];
        expect(said?.text).toBe("Opening your Canvas page, the PDF on page 4 and the doc.");
    });

    it("lists a longer set and waits for Start", () => {
        const said = lines(
            { type: "planRequested", runId: "r1", transcript: "pull up the assignment" },
            event({ kind: "planned", runId: "r1", autoStart: false, steps: CHEM.items.map(reopen) }),
        )[1];
        expect(said?.text).toBe("four places to open: your Canvas page, the PDF on page 4, the doc and the doc. Tap Start when you are ready.");
    });

    it("asks which one and leaves the choices on screen", () => {
        const said = lines(
            { type: "planRequested", runId: "r1", transcript: "the assignment" },
            event({ kind: "choose", runId: "r1", options: [BIO, CHEM] }),
        )[1];
        expect(said?.text).toBe("Which one? The choices are on screen.");
    });

    it("says how the windows were arranged ahead of what opened, and why not when they were not", () => {
        const finish = (summary: string) =>
            lines(
                { type: "planRequested", runId: "r1", transcript: "pull up the assignment side by side" },
                event({ kind: "planned", runId: "r1", autoStart: true, steps: [CANVAS, PDF].map(reopen) }),
                event({ kind: "finished", runId: "r1", ok: true, summary }),
            )[2]?.text;
        expect(finish("Arranged side by side. Opened: Assignment 3, reading.pdf, page 4.")).toMatch(/^Done\. Arranged side by side\. Opened/);
        expect(finish("Not arranged: FNDR is private right now. Opened: Assignment 3.")).toMatch(
            /^Done\. Not arranged: FNDR is private right now\. Opened: Assignment 3\./,
        );
    });

    it("says a saved set's name and how to bring it back", () => {
        const said = lines(
            { type: "planRequested", runId: "r2", transcript: "save this as capstone demo prep" },
            event({ kind: "finished", runId: "r2", ok: true, summary: "Saved as \u201ccapstone demo prep\u201d. Say \u201copen capstone demo prep\u201d to bring it back." }),
        )[1];
        expect(said?.text).toBe("Done. Saved as \u201ccapstone demo prep\u201d. Say \u201copen capstone demo prep\u201d to bring it back.");
    });

    it("names files, folders and apps", () => {
        expect(spokenItem({ ...DOC, kind: "file", label: "budget.xlsx" })).toBe("the file budget.xlsx");
        expect(spokenItem({ ...DOC, kind: "folder", label: "Lab 3" })).toBe("the Lab 3 folder");
        expect(spokenItem({ ...DOC, kind: "app", label: "Slack" })).toBe("Slack");
        expect(spokenItem({ ...DOC, host: "github.com" })).toBe("your GitHub page");
    });
});

describe("speakable", () => {
    it("says a link by its host and an email address by what it is", () => {
        expect(speakable("Opened https://canvas.utah.edu/courses/123/assignments?x=1 for you.")).toBe("Opened a link on canvas.utah.edu for you.");
        expect(speakable("Go to www.example.com now")).toBe("Go to a link on example.com now");
        expect(speakable("Wrote to kunj@example.com.")).toBe("Wrote to an email address.");
    });

    it("hides secrets and token-shaped strings", () => {
        for (const secret of ["sk-proj-abc123", "ghp_16C7e42F292c6912E7710c838347Ae178B4a", "AKIAIOSFODNN7EXAMPLE", "dGhpcyBpcyBhIHNlY3JldCB0b2tlbg==", "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b"]) {
            expect(speakable(`Typed ${secret} into the field.`), secret).toBe("Typed a hidden value into the field.");
        }
    });

    it("shortens long numbers and file paths, and keeps short numbers", () => {
        expect(speakable("Called 8015551234567.")).toBe("Called a long number.");
        expect(speakable("Saved on page 42.")).toBe("Saved on page 42.");
        expect(speakable("Opened /Users/kunj/Documents/Lab 3/report.pdf.")).toBe("Opened report.pdf.");
        expect(speakable("Opened ~/Downloads/data.csv")).toBe("Opened data.csv");
    });

    it("says a list of more than three items as a count", () => {
        expect(speakable("I left out: Open Notes, Type hello, Save, Close the window.")).toBe("I left out: four items, shown on screen.");
        expect(speakable("Opened: Assignment 3, the doc and the PDF.")).toBe("Opened: Assignment 3, the doc and the PDF.");
    });

    it("never says a stop word, so FNDR's own voice cannot stop a run", () => {
        expect(speakable("Step one of two: Stop the timer.")).not.toMatch(/\bstop\b/i);
        expect(speakable("Cancel my subscription")).not.toMatch(/\bcancel\b/i);
        expect(speakable("Stopped.")).toBe("Stopped.");
    });
});

describe("narration never contains a stop word", () => {
    const risky = [
        { label: "Stop", action: "operate" as const, app: "Timer" },
        { label: "Cancel the order", action: "operate" as const, app: "Safari" },
        { label: "Stop the timer", action: "operate" as const, app: "Clock" },
    ];
    const reopen = (item: WorkItem) => ({ label: `Open ${item.label}`, action: "reopen_memory" as const, app: item.appName, item });
    const runs: DoInput[][] = [
        [{ type: "planRequested", runId: "r1", transcript: "stop" }, event({ kind: "planned", runId: "r1", autoStart: true, steps: risky })],
        [{ type: "planRequested", runId: "r1", transcript: "cancel it" }, event({ kind: "planned", runId: "r1", autoStart: false, steps: risky })],
        [
            { type: "planRequested", runId: "r1", transcript: "x" },
            event({ kind: "planned", runId: "r1", autoStart: true, steps: risky }),
            event({ kind: "stepStarted", runId: "r1", index: 0, attempt: 1 }),
            event({ kind: "stepStarted", runId: "r1", index: 0, attempt: 2 }),
            event({ kind: "stepStarted", runId: "r1", index: 1, attempt: 1 }),
            event({ kind: "approval", runId: "r1", requestKey: "k", tool: "click", summary: "click Stop" }),
            event({ kind: "blocked", runId: "r1", index: 1, tool: "click", summary: "click Cancel", reason: "no" }),
            event({ kind: "stepDone", runId: "r1", index: 1, ok: false, detail: "stop" }),
            event({ kind: "finished", runId: "r1", ok: false, summary: "Stop" }),
        ],
        [{ type: "planRequested", runId: "r1", transcript: "x" }, event({ kind: "failed", runId: "r1", error: "cancel", reconnect: false })],
        [{ type: "error", message: "Stop" }],
        [
            { type: "planRequested", runId: "r1", transcript: "stop" },
            event({ kind: "choose", runId: "r1", options: [set("s", "Stop", [CANVAS])] }),
        ],
        [{ type: "planRequested", runId: "r1", transcript: "x" }, event({ kind: "planned", runId: "r1", autoStart: true, steps: [{ ...CANVAS, label: "stop" }].map(reopen) })],
    ];

    it("runs every narration branch through the stop-word matcher and finds no match", () => {
        let count = 0;
        for (const inputs of runs) {
            for (const said of lines(...inputs)) {
                if (!said) continue;
                count += 1;
                const spoken = speakable(said.text);
                expect(spoken, spoken).not.toMatch(/\b(stop|cancel)\b/i);
                for (const sentence of spoken.split(/[.!?:;,]+/)) expect(isStopWord(sentence), sentence).toBe(false);
            }
        }
        expect(count).toBeGreaterThanOrEqual(10);
    });
});
