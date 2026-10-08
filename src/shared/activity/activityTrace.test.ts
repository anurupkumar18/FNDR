import { describe, expect, it } from "vitest";
import {
    beginActivityTrace,
    recordActivityStep,
    type ActivityTraceStep,
} from "./activityTrace";

const started: ActivityTraceStep = {
    id: "request",
    label: "Request sent",
    actor: "FNDR",
    status: "running",
    evidence: "ipc-boundary",
    atMs: 1_000,
};

describe("activityTrace", () => {
    it("records only observed steps in event order", () => {
        const trace = recordActivityStep(
            beginActivityTrace({ id: "search-1", title: "Memory search", startedAtMs: 1_000 }),
            started,
        );

        expect(trace.steps).toEqual([started]);
        expect(trace.status).toBe("running");
    });

    it("updates an emitted step instead of inventing a duplicate", () => {
        const trace = recordActivityStep(
            recordActivityStep(
                beginActivityTrace({ id: "search-1", title: "Memory search", startedAtMs: 1_000 }),
                started,
            ),
            {
                ...started,
                status: "completed",
                atMs: 1_240,
                durationMs: 240,
                detail: "3 memories returned",
            },
        );

        expect(trace.steps).toHaveLength(1);
        expect(trace.steps[0]).toMatchObject({
            status: "completed",
            durationMs: 240,
            detail: "3 memories returned",
        });
        expect(trace.status).toBe("completed");
        expect(trace.finishedAtMs).toBe(1_240);
    });

    it("keeps a failure as the terminal observed state", () => {
        const trace = recordActivityStep(
            recordActivityStep(
                beginActivityTrace({ id: "guide-4", title: "Screen Guide", startedAtMs: 2_000 }),
                started,
            ),
            {
                id: "target",
                label: "Could not verify the target window",
                actor: "macOS Accessibility",
                status: "failed",
                evidence: "backend-event",
                atMs: 2_640,
            },
        );

        expect(trace.steps.map((step) => step.id)).toEqual(["request", "target"]);
        expect(trace.status).toBe("failed");
        expect(trace.finishedAtMs).toBe(2_640);
    });

    it("moves an updated older step to the current position", () => {
        let trace = recordActivityStep(
            beginActivityTrace({ id: "flow-1", title: "Observed flow", startedAtMs: 1_000 }),
            started,
        );
        trace = recordActivityStep(trace, {
            id: "handoff",
            label: "Waiting for handoff",
            actor: "FNDR",
            status: "waiting",
            evidence: "frontend-event",
            atMs: 1_100,
        });
        trace = recordActivityStep(trace, {
            ...started,
            label: "Request failed",
            status: "failed",
            atMs: 1_200,
        });

        expect(trace.steps.map((step) => step.id)).toEqual(["handoff", "request"]);
        expect(trace.steps[trace.steps.length - 1]?.label).toBe("Request failed");
        expect(trace.status).toBe("failed");
    });

    it("keeps a bounded recent history when a producer emits many observed steps", () => {
        let trace = beginActivityTrace({
            id: "download-1",
            title: "Model download",
            startedAtMs: 1_000,
        });

        for (let index = 0; index < 40; index += 1) {
            trace = recordActivityStep(trace, {
                id: `chunk-${index}`,
                label: `Observed chunk ${index}`,
                actor: "Model download service",
                status: "running",
                evidence: "backend-event",
                atMs: 1_000 + index,
            });
        }

        expect(trace.steps).toHaveLength(24);
        expect(trace.steps[0]?.id).toBe("chunk-16");
        expect(trace.steps[23]?.id).toBe("chunk-39");
    });
});
