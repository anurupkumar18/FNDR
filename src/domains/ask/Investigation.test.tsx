import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { CaseFile, InvestigateStep } from "@/shared/ipc/tauri";
import { caseFileMarkdown, Investigation, stepCopy } from "./Investigation";

const ipc = vi.hoisted(() => ({
    investigate: vi.fn(),
    investigateStop: vi.fn(),
    openWorkSet: vi.fn(),
    addTodo: vi.fn(),
}));
vi.mock("@/shared/ipc/tauri", async (importOriginal) => ({
    ...(await importOriginal<typeof import("@/shared/ipc/tauri")>()),
    ...ipc,
}));

const events = vi.hoisted(() => new Map<string, (payload: unknown) => void>());
vi.mock("@/shared/hooks/useTauriEvent", () => ({
    useTauriEvent: (name: string, handler: (payload: unknown) => void) => {
        events.set(name, handler);
    },
}));

function emit(payload: unknown) {
    act(() => events.get("investigate://step")?.(payload));
}

function step(overrides: Partial<InvestigateStep>): InvestigateStep {
    return {
        index: 0,
        kind: "search",
        status: "completed",
        found: 12,
        added: 8,
        total: 8,
        marked: 0,
        routes: ["vector", "keyword"],
        ms: 400,
        ...overrides,
    };
}

const DAY = 86_400_000;
const NOW = Date.UTC(2026, 10, 11, 18);

const steps: InvestigateStep[] = [
    step({}),
    step({ index: 1, kind: "thread", found: 16, added: 8, total: 16, routes: [], ms: 50 }),
    step({ index: 2, kind: "todos", found: 0, added: 0, total: 16, routes: [], ms: 1 }),
    step({ index: 3, kind: "check", found: 16, added: 49, total: 16, marked: 1, routes: [], ms: 5 }),
];

const caseFile: CaseFile = {
    goal: "Catch me up on the Smart Reminders launch",
    outcome: "found",
    title: "Smart Reminders launch",
    stats: { memories: 16, apps: ["Slack", "Google Chrome"], firstMs: NOW - 6 * DAY, lastMs: NOW, ms: 460 },
    timeline: Array.from({ length: 8 }, (_, index) => ({
        memoryId: `m${index}`,
        atMs: NOW - (7 - index) * DAY,
        app: "Slack",
        title: `Thread ${index}`,
        line: `Line number ${index}.`,
    })),
    decisions: [{ text: "Keep SMS reminders out of v1", atMs: NOW - 6 * DAY, memoryId: "m0", app: "Google Chrome", also: 1 }],
    open: [
        { text: "Present the recommendation", atMs: NOW, memoryId: "m7", app: "Keynote" },
        { text: "Send the deck", atMs: NOW - DAY, memoryId: "m6", app: "Slack", todoId: "t1" },
    ],
    done: [
        {
            text: "Close LL-1482",
            atMs: NOW - 2 * DAY,
            memoryId: "m5",
            clause: "confirmed the LL-1482 fix is live in production",
            doneAtMs: NOW,
            doneMemoryId: "m7",
            doneApp: "Slack",
        },
    ],
    problems: [{ text: "18% of reminder emails landing in spam", atMs: NOW - 4 * DAY, memoryId: "m2", app: "Slack", also: 0 }],
    places: [1, 2, 3, 4].map((index) => ({
        memoryId: `m${index}`,
        label: `Place ${index}`,
        kind: "url",
        reopenRank: 4,
        appName: "Google Chrome",
        capturedAt: NOW,
    })),
    steps,
};

function renderRun(onOpenMemoryById = vi.fn(), onDismiss = vi.fn()) {
    render(<Investigation goal={caseFile.goal} onOpenMemoryById={onOpenMemoryById} onDismiss={onDismiss} />);
    return { onOpenMemoryById, onDismiss };
}

function runId(): string {
    return ipc.investigate.mock.calls[0][1] as string;
}

beforeEach(() => {
    ipc.investigateStop.mockResolvedValue(undefined);
});

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    events.clear();
});

describe("Investigation", () => {
    it("shows each step as its event arrives, with counts and no memory text", async () => {
        ipc.investigate.mockReturnValue(new Promise(() => {}));
        renderRun();
        expect(ipc.investigate).toHaveBeenCalledWith(caseFile.goal, expect.stringContaining("investigate-"));

        emit({ runId: runId(), ...step({ status: "running", found: 0, added: 0, total: 0 }) });
        const trace = screen.getByRole("region", { name: "Investigation steps" });
        expect(within(trace).getAllByText("Searching memory").length).toBeGreaterThan(0);

        emit({ runId: runId(), ...steps[0] });
        emit({ runId: runId(), ...steps[1] });
        expect(within(trace).getByText("Searched memory")).toBeInTheDocument();
        expect(within(trace).getByText("12 found, 8 kept · vector + keyword")).toBeInTheDocument();
        expect(within(trace).getByText("16 memories in the thread, 8 new")).toBeInTheDocument();
        // While it runs there is a Stop, and no case file yet.
        expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
        expect(screen.queryByRole("article")).not.toBeInTheDocument();
    });

    it("ignores the steps of another run", () => {
        ipc.investigate.mockReturnValue(new Promise(() => {}));
        renderRun();
        emit({ runId: "someone-else", ...steps[0] });
        expect(screen.queryByRole("region", { name: "Investigation steps" })).not.toBeInTheDocument();
    });

    it("renders the case file with a citation on every line", async () => {
        ipc.investigate.mockResolvedValue(caseFile);
        const { onOpenMemoryById } = renderRun();

        const file = await screen.findByRole("article", { name: "Case file: Smart Reminders launch" });
        expect(within(file).getByText(/16 memories · Slack, Google Chrome ·/)).toBeInTheDocument();

        fireEvent.click(within(file).getByRole("button", { name: /Keep SMS reminders out of v1/ }));
        expect(onOpenMemoryById).toHaveBeenLastCalledWith("m0");

        // A step recorded as done shows the later clause and opens that later memory.
        const done = within(file).getByRole("region", { name: "Recorded as done" });
        fireEvent.click(within(done).getByRole("button", { name: /Close LL-1482/ }));
        expect(onOpenMemoryById).toHaveBeenLastCalledWith("m7");
        expect(within(done).getByText("“confirmed the LL-1482 fix is live in production”")).toBeInTheDocument();

        // The timeline shows the latest six until asked for all.
        const happened = within(file).getByRole("region", { name: "What happened" });
        expect(within(happened).getAllByRole("listitem")).toHaveLength(6);
        fireEvent.click(within(happened).getByRole("button", { name: "Show all 8" }));
        expect(within(happened).getAllByRole("listitem")).toHaveLength(8);

        // The steps the file carries are shown even when no event was heard.
        fireEvent.click(screen.getByRole("button", { name: "Show Investigation steps details" }));
        expect(screen.getByText("16 memories, 49 lines, 1 recorded as done")).toBeInTheDocument();
    });

    it("asks once more before opening more than three places, then shows what opened", async () => {
        ipc.investigate.mockResolvedValue(caseFile);
        ipc.openWorkSet.mockResolvedValue([
            { memoryId: "m1", label: "Place 1", ok: true, detail: "Opened" },
            { memoryId: "m2", label: "Place 2", ok: false, detail: "The file has moved" },
        ]);
        renderRun();

        fireEvent.click(await screen.findByRole("button", { name: "Open 4 places" }));
        expect(ipc.openWorkSet).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole("button", { name: "Open all 4" }));

        await waitFor(() => expect(ipc.openWorkSet).toHaveBeenCalledWith(["m1", "m2", "m3", "m4"]));
        const opened = await screen.findByRole("list", { name: "What opened" });
        expect(within(opened).getByText("Opened")).toBeInTheDocument();
        expect(within(opened).getByText("The file has moved")).toBeInTheDocument();
    });

    it("opens three places or fewer on the first tap", async () => {
        ipc.investigate.mockResolvedValue({ ...caseFile, places: caseFile.places.slice(0, 2) });
        ipc.openWorkSet.mockResolvedValue([]);
        renderRun();
        fireEvent.click(await screen.findByRole("button", { name: "Open 2 places" }));
        await waitFor(() => expect(ipc.openWorkSet).toHaveBeenCalledWith(["m1", "m2"]));
    });

    it("adds a next step to To-dos on a tap, and never offers that for a to-do", async () => {
        ipc.investigate.mockResolvedValue(caseFile);
        ipc.addTodo.mockResolvedValue({});
        renderRun();

        const add = await screen.findByRole("button", { name: "Add to To-dos: Present the recommendation" });
        expect(screen.queryByRole("button", { name: "Add to To-dos: Send the deck" })).not.toBeInTheDocument();
        fireEvent.click(add);
        await waitFor(() => expect(ipc.addTodo).toHaveBeenCalledWith("Present the recommendation", "Todo"));
        expect(await screen.findByRole("button", { name: "Add to To-dos: Present the recommendation" })).toHaveTextContent("Added");
    });

    it("says nothing matched, with how many searches ran and no case file", async () => {
        ipc.investigate.mockResolvedValue({
            ...caseFile,
            outcome: "nothing_found",
            timeline: [],
            decisions: [],
            open: [],
            done: [],
            problems: [],
            places: [],
            steps: [
                step({ status: "degraded", note: "weak_match", added: 0 }),
                step({ index: 1, kind: "widen", status: "degraded", note: "weak_match", added: 0 }),
            ],
        });
        const { onDismiss } = renderRun();

        expect(await screen.findByText("Nothing in memory matches")).toBeInTheDocument();
        expect(screen.getByText(/FNDR ran 2 searches and found no strong match/)).toBeInTheDocument();
        expect(screen.queryByRole("article")).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Ask something else" }));
        expect(onDismiss).toHaveBeenCalledTimes(1);
    });

    it("asks for a topic, and says when FNDR is private", async () => {
        ipc.investigate.mockResolvedValue({ ...caseFile, outcome: "needs_topic", steps: [] });
        renderRun();
        expect(await screen.findByText("Name the work to investigate")).toBeInTheDocument();
        cleanup();

        ipc.investigate.mockResolvedValue({ ...caseFile, outcome: "private", steps: [] });
        renderRun();
        expect(await screen.findByText("FNDR is private right now")).toBeInTheDocument();
    });

    it("stops the run on Stop and when the panel goes away", async () => {
        ipc.investigate.mockReturnValue(new Promise(() => {}));
        const { unmount } = render(<Investigation goal="the launch" onOpenMemoryById={() => {}} onDismiss={() => {}} />);
        emit({ runId: runId(), ...step({ status: "running" }) });

        fireEvent.click(screen.getByRole("button", { name: "Stop" }));
        expect(ipc.investigateStop).toHaveBeenCalledWith(runId());
        expect(screen.getByRole("button", { name: "Stopping" })).toBeDisabled();

        unmount();
        expect(ipc.investigateStop).toHaveBeenCalledTimes(2);
    });

    it("says so when the run fails", async () => {
        ipc.investigate.mockRejectedValue(new Error("store closed"));
        renderRun();
        expect(await screen.findByText("The investigation could not run")).toBeInTheDocument();
    });
});

describe("stepCopy", () => {
    it("names every kind of step from counts alone", () => {
        expect(stepCopy(step({ note: "weak_match", status: "degraded", found: 1 }))).toEqual({
            label: "First search found no strong match",
            detail: "1 weak result · vector + keyword",
        });
        expect(stepCopy(step({ kind: "widen" })).label).toBe("Widened search found the work");
        expect(stepCopy(step({ kind: "thread", note: "no_thread", status: "degraded" })).label).toBe(
            "No work thread joins these memories",
        );
        expect(stepCopy(step({ kind: "probe", inquiry: "decisions", status: "running" })).label).toBe("Probing for decisions");
        expect(stepCopy(step({ kind: "probe", inquiry: "open", note: "nothing_new" })).label).toBe(
            "Probe for open items found nothing new",
        );
        expect(stepCopy(step({ kind: "links", added: 2 }))).toEqual({ label: "Followed stored links", detail: "2 new" });
        expect(stepCopy(step({ kind: "todos", added: 1 })).detail).toBe("1 tied to this work");
        expect(stepCopy(step({ kind: "check", note: "tool_failed" })).label).toBe("A step could not read memory");
    });
});

describe("caseFileMarkdown", () => {
    it("writes the sections that hold lines, with the later clause for a done step", () => {
        const text = caseFileMarkdown(caseFile);
        expect(text.startsWith("# Smart Reminders launch\n")).toBe(true);
        expect(text).toContain("## Next steps on record\n- Present the recommendation\n- Send the deck");
        expect(text).toMatch(/- Close LL-1482 \(later, .*: "confirmed the LL-1482 fix is live in production"\)/);
        expect(text).toContain("## Places\n- Place 1 (Google Chrome)");
        expect(caseFileMarkdown({ ...caseFile, problems: [] })).not.toContain("## Problems seen");
    });
});
