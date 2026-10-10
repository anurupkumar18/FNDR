import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { Task, WorkItem, WorkSetResolution } from "@/shared/ipc/tauri";
import { HomeDeadlines } from "./HomeDeadlines";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const HOUR = 60 * 60 * 1000;
const now = new Date("2026-10-09T10:00:00").getTime();

function task(overrides: Partial<Task>): Task {
    return {
        id: "lab4",
        title: "Lab 4",
        description: "",
        source_app: "manual",
        source_memory_id: "canvas",
        created_at: 0,
        due_date: now + 20 * HOUR,
        is_completed: false,
        is_dismissed: false,
        task_type: "Todo",
        linked_urls: [],
        linked_memory_ids: ["pdf"],
        ...overrides,
    };
}

function item(memoryId: string): WorkItem {
    return { memoryId, label: `Item ${memoryId}`, kind: "url", reopenRank: 4, appName: "Safari", capturedAt: 1 };
}

function route(tasks: Task[] | Error, resolution: WorkSetResolution = {
    kind: "best",
    value: { id: "ws", title: "Lab 4", reason: "", score: 1, items: [item("canvas"), item("pdf"), item("doc")] },
}) {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
        if (command === "get_todos") {
            if (tasks instanceof Error) throw tasks;
            return tasks;
        }
        if (command === "resolve_work_set") return resolution;
        if (command === "open_work_set") {
            return (args as { memoryIds: string[] }).memoryIds.map((memoryId) => ({ memoryId, label: memoryId, ok: true, detail: "Opened", outcome: { kind: "opened" } }));
        }
        if (command === "complete_todo") return true;
        throw new Error(`unexpected ${command}`);
    });
}

beforeEach(() => {
    vi.mocked(invoke).mockReset();
});
afterEach(cleanup);

describe("Deadline cards on Home", () => {
    it("shows a task due within 72 hours with how many items are ready, without opening anything", async () => {
        route([task({}), task({ id: "far", title: "Essay", due_date: now + 100 * HOUR })]);
        render(<HomeDeadlines nowMs={now} />);
        expect(screen.getByRole("status")).toHaveTextContent("Checking what is due");
        expect(await screen.findByRole("heading", { name: "Lab 4 due tomorrow, 3 items ready to reopen" })).toBeInTheDocument();
        expect(screen.queryByText(/Essay/)).not.toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("resolve_work_set", { query: "Lab 4" });
        expect(invoke).not.toHaveBeenCalledWith("open_work_set", expect.anything());
    });

    it("opens the task's set and offers to mark the task done", async () => {
        route([task({})]);
        render(<HomeDeadlines nowMs={now} />);
        fireEvent.click(await screen.findByRole("button", { name: "Open all for Lab 4" }));
        expect(await screen.findByText("Opened 3 of 3")).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Mark this task done" }));
        await waitFor(() => expect(invoke).toHaveBeenCalledWith("complete_todo", { taskId: "lab4" }));
        expect(screen.getByRole("button", { name: "Continue where you left off" })).toBeInTheDocument();
    });

    it("never offers a set that shares nothing with the task", async () => {
        route([task({})], { kind: "best", value: { id: "ws", title: "Other", reason: "", score: 1, items: [item("x")] } });
        render(<HomeDeadlines nowMs={now} />);
        expect(await screen.findByRole("heading", { name: "Lab 4 due tomorrow" })).toBeInTheDocument();
        expect(screen.getByText("Nothing saved to reopen for it yet.")).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: /Open all/ })).not.toBeInTheDocument();
    });

    it("says plainly when nothing is due soon", async () => {
        route([task({ due_date: null })]);
        render(<HomeDeadlines nowMs={now} />);
        expect(await screen.findByText("Nothing is due in the next three days.")).toBeInTheDocument();
        expect(invoke).not.toHaveBeenCalledWith("resolve_work_set", expect.anything());
    });

    it("reports a load failure without backend details", async () => {
        route(new Error("lancedb path /Users/x"));
        render(<HomeDeadlines nowMs={now} />);
        expect(await screen.findByRole("alert")).toHaveTextContent("Deadlines could not be loaded.");
        expect(screen.queryByText(/lancedb/)).not.toBeInTheDocument();
    });
});
