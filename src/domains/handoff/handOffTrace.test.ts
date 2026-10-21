import { describe, expect, it } from "vitest";
import type { HandOffEvent, HandOffRecord } from "@/shared/ipc/tauri";
import { applyHandOffEvent, beginHandOffTrace, finishHandOffTrace } from "./handOffTrace";

const last = <T,>(items: T[]): T | undefined => items[items.length - 1];

// The order of events in spike S6's real turn.
const S6: HandOffEvent[] = [
    { kind: "ranCode", ok: true, ms: 0 },
    { kind: "tokens", total: 11449 },
    { kind: "rateLimits", primary: 67, secondary: 89 },
    { kind: "readFile", name: "q4-onboarding-survey.csv" },
    { kind: "ranCode", ok: true, ms: 12104 },
    { kind: "readFile", name: "q4-onboarding-survey.csv" },
    { kind: "ranCode", ok: true, ms: 875 },
    { kind: "wroteFile", path: "work/readout-spec.json" },
    { kind: "ranCode", ok: true, ms: 900 },
    { kind: "viewedImage", path: "out/readout-chart.png" },
    { kind: "wroteFile", path: "work/readout-spec.json" },
    { kind: "answered", text: "{}" },
    { kind: "idle" },
];

function record(overrides: Partial<HandOffRecord> = {}): HandOffRecord {
    return {
        id: "j1", createdAtMs: 1, request: "r", goal: "doc", memoryIds: [], fileMemoryIds: [],
        folder: "/f", status: "ready", reason: null, finishedAtMs: 2, bytesSent: 10,
        checked: 4, unsupported: 0, notChecked: 0, deliverables: ["out/readout.docx"], rating: null,
        ...overrides,
    };
}

function play(events: HandOffEvent[]) {
    return events.reduce((trace, event, i) => applyHandOffEvent(trace, event, 1_000 + i), beginHandOffTrace(1_000));
}

describe("hand-off trace", () => {
    it("turns S6's turn into a few counted steps without counts of tokens", () => {
        const trace = play(S6);
        expect(trace.steps.map((s) => [s.label, s.detail ?? ""])).toEqual([
            ["Handed off to the worker", ""],
            ["Read the attached files", "2 reads"],
            ["Ran analysis code", "4 runs"],
            ["Looked over what it made", ""],
            ["Saved its working notes", "2 files"],
            ["Listed its claims and sources", ""],
        ]);
    });

    it("never names a file, attached or made", () => {
        const trace = play([
            { kind: "readFile", name: "salaries-2026.csv" },
            { kind: "wroteFile", path: "out/q4-readout.docx" },
        ]);
        const text = JSON.stringify(trace.steps);
        expect(text).not.toContain("salaries");
        expect(text).not.toContain("q4-readout");
        expect(last(trace.steps)).toMatchObject({ label: "Wrote what it was asked for", detail: "1 file" });
    });

    it("keeps a failed run visible after later ones pass", () => {
        const trace = play([
            { kind: "ranCode", ok: false, ms: 5 },
            { kind: "ranCode", ok: true, ms: 5 },
        ]);
        expect(trace.steps.find((s) => s.id === "code")?.status).toBe("degraded");
    });

    it("ends on FNDR's check, by the record's status", () => {
        const done = finishHandOffTrace(play(S6), record(), 9_000);
        expect(done.status).toBe("completed");
        expect(last(done.steps)).toMatchObject({ actor: "FNDR", label: "Checked 4 claims: 4 with their sources" });
        const gaps = finishHandOffTrace(play(S6), record({ status: "ready_with_gaps", checked: 3, unsupported: 1 }), 9_000);
        expect(gaps.status).toBe("degraded");
        const stopped = finishHandOffTrace(play(S6), record({ status: "stopped", reason: "Codex app-server exited: /Users/sam/x" }), 9_000);
        expect(last(stopped.steps)).toMatchObject({ label: "Stopped" });
        expect(JSON.stringify(stopped.steps)).not.toContain("/Users/sam");
        const never = finishHandOffTrace(beginHandOffTrace(1), null, 2);
        expect(last(never.steps)).toMatchObject({ status: "failed", label: "Did not start" });
    });
});
