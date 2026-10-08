import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ActivityTrace } from "./ActivityTrace";
import type { ActivityTraceSnapshot } from "@/shared/activity/activityTrace";

const trace: ActivityTraceSnapshot = {
    id: "screen-guide-9",
    title: "Screen Guide activity",
    status: "running",
    startedAtMs: 1_000,
    finishedAtMs: null,
    steps: [
        {
            id: "prepare",
            label: "Request prepared",
            actor: "FNDR",
            status: "completed",
            evidence: "ipc-boundary",
            atMs: 1_000,
            durationMs: 4,
        },
        {
            id: "target",
            label: "Verifying Google Chrome",
            actor: "macOS Accessibility",
            status: "running",
            evidence: "backend-event",
            atMs: 1_004,
        },
    ],
};

afterEach(cleanup);

describe("ActivityTrace", () => {
    it("shows the latest observed step without presenting hidden reasoning", () => {
        render(<ActivityTrace trace={trace} />);

        const liveStatus = screen.getByRole("status");
        const disclosure = screen.getByRole("button", {
            name: "Show Screen Guide activity details",
        });

        expect(liveStatus).toHaveTextContent("Verifying Google Chrome");
        expect(liveStatus).toHaveTextContent("macOS Accessibility");
        expect(liveStatus).toHaveTextContent("Running");
        expect(liveStatus).not.toContainElement(disclosure);
        expect(screen.queryByText("Request prepared")).not.toBeInTheDocument();
        expect(disclosure).toHaveAttribute(
            "aria-expanded",
            "false",
        );
    });

    it("expands to the timestamped evidence-backed trace", () => {
        render(<ActivityTrace trace={trace} />);

        fireEvent.click(screen.getByRole("button", {
            name: "Show Screen Guide activity details",
        }));

        expect(screen.getByText("Request prepared")).toBeInTheDocument();
        expect(screen.getByText(/Request boundary/)).toBeInTheDocument();
        expect(screen.getByText(/Live backend event/)).toBeInTheDocument();
        const steps = screen.getAllByRole("listitem");
        expect(within(steps[0]).getByText("Completed")).toBeVisible();
        expect(within(steps[1]).getByText("Running")).toBeVisible();
        expect(screen.getByText(/No prompts, captured text, or private model reasoning/)).toBeInTheDocument();
    });

    it("omits disclosure controls on a passive surface", () => {
        render(<ActivityTrace trace={trace} showDetails={false} />);

        expect(screen.getByRole("status")).toHaveTextContent("Verifying Google Chrome");
        expect(screen.queryByRole("button", { name: /activity details/i })).not.toBeInTheDocument();
        expect(screen.queryByText("Request prepared")).not.toBeInTheDocument();
    });

    it("can defer live-region ownership to a nearby terminal result", () => {
        render(<ActivityTrace trace={trace} announce={false} />);

        expect(screen.getByLabelText("Screen Guide activity")).toHaveTextContent(
            "Verifying Google Chrome",
        );
        expect(screen.queryByRole("status")).not.toBeInTheDocument();
    });

    it("labels a backend snapshot as backend status", () => {
        render(<ActivityTrace
            trace={{
                ...trace,
                steps: [{
                    ...trace.steps[0],
                    evidence: "backend-snapshot",
                }],
            }}
            defaultExpanded
        />);

        expect(screen.getByText(/Backend status/)).toBeInTheDocument();
    });

    it("distinguishes disclosure controls when multiple traces share a surface", () => {
        render(<>
            <ActivityTrace trace={trace} />
            <ActivityTrace trace={{ ...trace, id: "summary-1", title: "Summary activity" }} />
        </>);

        expect(screen.getByRole("button", {
            name: "Show Screen Guide activity details",
        })).toBeVisible();
        expect(screen.getByRole("button", {
            name: "Show Summary activity details",
        })).toBeVisible();
    });

    it("renders an unavailable time without serializing an invalid timestamp", () => {
        render(<ActivityTrace
            trace={{
                ...trace,
                steps: [{
                    ...trace.steps[0],
                    atMs: Number.POSITIVE_INFINITY,
                }],
            }}
            defaultExpanded
        />);

        const time = screen.getByText("Time unavailable");
        expect(time).not.toHaveAttribute("datetime");
    });
});
