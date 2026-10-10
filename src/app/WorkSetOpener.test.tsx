import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { WorkItem, WorkItemOutcome } from "@/shared/ipc/tauri";
import { WorkSetOpener } from "./WorkSetOpener";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function item(memoryId: string): WorkItem {
    return { memoryId, label: `Page ${memoryId}`, kind: "url", reopenRank: 4, appName: "Safari", capturedAt: 1 };
}

function opened(memoryId: string): WorkItemOutcome {
    return { memoryId, label: `Page ${memoryId}`, kind: "url", ok: true, detail: "Opened", outcome: { kind: "opened" } };
}

function routeOpens(outcomes: Record<string, WorkItemOutcome>) {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
        if (command === "open_work_set") {
            const ids = (args as { memoryIds: string[] }).memoryIds;
            return ids.map((id) => outcomes[id] ?? opened(id));
        }
        if (command === "complete_todo") return true;
        if (command === "reopen_memory") return { kind: "opened" };
        throw new Error(`unexpected ${command}`);
    });
}

beforeEach(() => {
    vi.mocked(invoke).mockReset();
});
afterEach(cleanup);

describe("Open all", () => {
    it("never resolves or opens anything until the person taps", () => {
        const load = vi.fn();
        render(<WorkSetOpener name="Parser" load={load} />);
        expect(load).not.toHaveBeenCalled();
        expect(invoke).not.toHaveBeenCalled();
        expect(screen.getByRole("button", { name: "Open all for Parser" })).toBeEnabled();
    });

    it("opens three items on the first tap, one at a time, and shows each outcome", async () => {
        routeOpens({
            b: { memoryId: "b", label: "Notes.pdf", kind: "file", ok: true, detail: "Opened from its new place: Notes.pdf", outcome: { kind: "opened_moved", new_path: "/x/Notes.pdf" } },
            c: { memoryId: "c", label: "Canvas", kind: "url", ok: false, detail: "Actions are turned off in Settings." },
        });
        render(<WorkSetOpener name="Parser" load={async () => [item("a"), item("b"), item("c")]} />);
        fireEvent.click(screen.getByRole("button", { name: "Open all for Parser" }));

        expect(await screen.findByText("Opened 2 of 3")).toBeInTheDocument();
        expect(vi.mocked(invoke).mock.calls.map((call) => (call[1] as { memoryIds: string[] }).memoryIds)).toEqual([["a"], ["b"], ["c"]]);
        const rows = screen.getAllByRole("listitem");
        expect(rows[0]).toHaveTextContent("Page aOpened");
        expect(rows[1]).toHaveTextContent("Moved, opened from its new place");
        expect(rows[2]).toHaveTextContent("Needs permission");
        expect(rows[2]).toHaveTextContent("Actions are turned off in Settings.");
    });

    it("asks for one more tap before opening more than three items", async () => {
        routeOpens({});
        render(<WorkSetOpener name="Lab 4" load={async () => ["a", "b", "c", "d"].map(item)} />);
        fireEvent.click(screen.getByRole("button", { name: "Open all for Lab 4" }));

        const confirm = await screen.findByRole("button", { name: "Open 4 items" });
        expect(screen.getByText("Page d")).toBeInTheDocument();
        expect(invoke).not.toHaveBeenCalled();
        await waitFor(() => expect(confirm).toHaveFocus());
        fireEvent.click(confirm);
        expect(await screen.findByText("Opened 4 of 4")).toBeInTheDocument();
        expect(invoke).toHaveBeenCalledTimes(4);
    });

    it("cancels at the question without opening anything", async () => {
        render(<WorkSetOpener name="Lab 4" load={async () => ["a", "b", "c", "d"].map(item)} />);
        fireEvent.click(screen.getByRole("button", { name: "Open all for Lab 4" }));
        fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
        expect(screen.queryByRole("button", { name: "Open 4 items" })).not.toBeInTheDocument();
        expect(invoke).not.toHaveBeenCalled();
    });

    it("stops between items and says which were not opened", async () => {
        let finishFirst!: (rows: WorkItemOutcome[]) => void;
        vi.mocked(invoke).mockReturnValueOnce(new Promise<WorkItemOutcome[]>((resolve) => { finishFirst = resolve; }));
        render(<WorkSetOpener name="Parser" load={async () => [item("a"), item("b")]} />);
        fireEvent.click(screen.getByRole("button", { name: "Open all for Parser" }));
        fireEvent.click(await screen.findByRole("button", { name: "Stop" }));
        await act(async () => finishFirst([opened("a")]));

        expect(await screen.findByText("Stopped after 1 of 2")).toBeInTheDocument();
        expect(screen.getAllByRole("listitem")[1]).toHaveTextContent("Not opened, stopped");
        expect(invoke).toHaveBeenCalledOnce();
    });

    it("says why when there is nothing to reopen", async () => {
        render(<WorkSetOpener name="Parser" load={async () => ({ why: "Nothing saved to reopen for this thread yet." })} />);
        fireEvent.click(screen.getByRole("button", { name: "Open all for Parser" }));
        expect(await screen.findByText("Nothing saved to reopen for this thread yet.")).toHaveAttribute("role", "status");
        expect(invoke).not.toHaveBeenCalled();
    });

    it("closes the loop: marks the task done and continues at the latest source", async () => {
        routeOpens({});
        const onTaskDone = vi.fn();
        render(
            <WorkSetOpener
                name="Lab 4"
                load={async () => [item("a")]}
                taskId="task-1"
                continueMemoryId="latest"
                onTaskDone={onTaskDone}
            />,
        );
        fireEvent.click(screen.getByRole("button", { name: "Open all for Lab 4" }));
        fireEvent.click(await screen.findByRole("button", { name: "Mark this task done" }));
        expect(await screen.findByText("Marked done")).toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("complete_todo", { taskId: "task-1" });
        expect(onTaskDone).toHaveBeenCalledOnce();

        fireEvent.click(screen.getByRole("button", { name: "Continue where you left off" }));
        expect(await screen.findByText("Opened the latest source")).toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("reopen_memory", { memoryId: "latest" });
    });

    it("offers no task action when the set did not come from a task", async () => {
        routeOpens({});
        render(<WorkSetOpener name="Parser" load={async () => [item("a")]} />);
        fireEvent.click(screen.getByRole("button", { name: "Open all for Parser" }));
        await screen.findByText("Opened 1 of 1");
        expect(screen.queryByRole("button", { name: "Mark this task done" })).not.toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Continue where you left off" })).not.toBeInTheDocument();
    });
});
