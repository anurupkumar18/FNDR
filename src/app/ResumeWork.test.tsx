import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ResumeThread, WorkItem } from "@/shared/ipc/tauri";
import { ResumeWork } from "./ResumeWork";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function thread(title = "Parser"): ResumeThread {
    return {
        key: `key:${title}`,
        title,
        last_state: "Fixed weekday aliases",
        age_minutes: 12,
        next_steps: [],
        suggested_next_steps: [{ title: "Add the regression test", source_memory_id: "earlier", confidence: 0.7 }],
        evidence: ["earlier", "latest"],
        pack: { items: [{ memory_id: "earlier", text: "Add the regression test", ts_ms: 1 }], dropped_for_budget: 0, estimated_tokens: 6 },
    };
}

function item(memoryId: string): WorkItem {
    return { memoryId, label: `Item ${memoryId}`, kind: "url", reopenRank: 4, appName: "Safari", capturedAt: 1 };
}

type Resume = ResumeThread[] | Error | Promise<ResumeThread[]>;
let resumeRows: Resume[] = [];

function route() {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
        const payload = args as Record<string, unknown> | undefined;
        switch (command) {
            case "resume_work": {
                const next = resumeRows.length > 1 ? resumeRows.shift()! : resumeRows[0] ?? [];
                if (next instanceof Error) throw next;
                return next;
            }
            case "what_changed_since":
                return {
                    thread_key: payload?.threadKey, title: "", since_ms: 0, first_view: true, new_memories: 3,
                    page_count: 3, pages: ["A", "B", "C"], file_count: 0, files: [], task_count: 0, tasks: [],
                    commit_count: 0, newest_memory_id: null,
                };
            case "mark_thread_seen": return undefined;
            case "get_todos": return [];
            case "list_named_sets": return [];
            case "routine_offers": return [];
            case "resolve_work_set":
                return { kind: "best", value: { id: "ws", title: "Parser", reason: "", score: 1, items: [item("earlier"), item("latest")] } };
            case "open_work_set":
                return (payload?.memoryIds as string[]).map((memoryId) => ({ memoryId, label: `Item ${memoryId}`, ok: true, detail: "Opened", outcome: { kind: "opened" } }));
            case "reopen_memory": return { kind: "opened" };
            default: throw new Error(`unexpected ${command}`);
        }
    });
}

function calls(command: string) {
    return vi.mocked(invoke).mock.calls.filter((call) => call[0] === command);
}

function recentWork() {
    return within(screen.getByRole("region", { name: "Pick up where you left off" }));
}

beforeEach(() => {
    vi.mocked(invoke).mockReset();
    resumeRows = [[thread()]];
    route();
});
afterEach(cleanup);

describe("Resume Work on Home", () => {
    it("loads recent work through the existing IPC and opens exact cited memories", async () => {
        const onOpenMemory = vi.fn();
        const onOpenVault = vi.fn();
        render(<ResumeWork onOpenMemory={onOpenMemory} onOpenVault={onOpenVault} />);

        expect(recentWork().getByRole("status")).toHaveTextContent("Loading recent work");
        expect(await screen.findByText("Fixed weekday aliases")).toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("resume_work", { hours: 24, budgetTokens: 800 });
        expect(screen.getByText("12 min ago")).toBeInTheDocument();
        expect(screen.getByText("Suggested next step")).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "View latest source for Parser" }));
        expect(onOpenMemory).toHaveBeenLastCalledWith("latest");
        fireEvent.click(screen.getByRole("button", { name: "View source for suggested step: Add the regression test" }));
        expect(onOpenMemory).toHaveBeenLastCalledWith("earlier");
        fireEvent.click(screen.getByRole("button", { name: "Open Vault" }));
        expect(onOpenVault).toHaveBeenCalledOnce();
        expect(calls("resume_work")).toHaveLength(1);
    });

    it("never resolves or opens a set on page load", async () => {
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        await screen.findByText("Fixed weekday aliases");
        await screen.findByText("Nothing is due in the next three days.");
        expect(calls("resolve_work_set")).toHaveLength(0);
        expect(calls("open_work_set")).toHaveLength(0);
        expect(calls("reopen_memory")).toHaveLength(0);
    });

    it("opens a thread's work set through the shared resolver and continues at its latest source", async () => {
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        fireEvent.click(await screen.findByRole("button", { name: "Open all for Parser" }));
        expect(await screen.findByText("Opened 2 of 2")).toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("resolve_work_set", { query: "Parser" });
        expect(screen.queryByRole("button", { name: "Mark this task done" })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Continue where you left off" }));
        await waitFor(() => expect(invoke).toHaveBeenCalledWith("reopen_memory", { memoryId: "latest" }));
    });

    it("passes a layout when Arrange side by side is on", async () => {
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        fireEvent.click(screen.getByRole("checkbox", { name: "Arrange side by side when a set opens" }));
        fireEvent.click(await screen.findByRole("button", { name: "Open all for Parser" }));
        await screen.findByText(/Opened 2 of 2/);
        expect(invoke).toHaveBeenCalledWith("open_work_set", { memoryIds: ["earlier", "latest"], layout: "left_right_split" });
    });

    it("shows what changed in a thread by its key", async () => {
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        expect(await screen.findByRole("button", { name: "3 new pages in the last day" })).toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("what_changed_since", { threadKey: "key:Parser" });
    });

    it("shows the other Home sections with their honest empty states", async () => {
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        expect(await screen.findByRole("heading", { name: "Due soon" })).toBeInTheDocument();
        expect(await screen.findByText(/No saved sets yet/)).toBeInTheDocument();
        expect(screen.getByRole("heading", { name: "Your sets" })).toBeInTheDocument();
    });

    it("shows only three threads without presenting uncited suggestions", async () => {
        const first = thread("First");
        first.suggested_next_steps[0].source_memory_id = "missing";
        resumeRows = [[first, thread("Second"), thread("Third"), thread("Fourth")]];
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);

        await screen.findByRole("heading", { name: "First" });
        expect(recentWork().getAllByRole("heading", { level: 3 })).toHaveLength(3);
        expect(screen.queryByRole("heading", { name: "Fourth" })).not.toBeInTheDocument();
        expect(screen.getAllByText("Suggested next step")).toHaveLength(2);
    });

    it("teaches the next action when no recent memories are available", async () => {
        resumeRows = [[]];
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        expect(await screen.findByText(/No recent work yet/)).toHaveTextContent("find older memories");
        expect(screen.queryByRole("heading", { name: "Parser" })).not.toBeInTheDocument();
    });

    it("offers retry after an error without exposing backend details", async () => {
        resumeRows = [new Error("private filesystem path"), [thread()]];
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        expect(await recentWork().findByRole("alert")).toHaveTextContent("Recent work couldn’t be loaded");
        expect(screen.queryByText(/private filesystem/)).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Retry" }));
        expect(await screen.findByText("Fixed weekday aliases")).toBeInTheDocument();
        expect(recentWork().queryByRole("alert")).not.toBeInTheDocument();
    });

    it("refreshes from current rows and removes old citations while loading", async () => {
        let completeRefresh!: (rows: ResumeThread[]) => void;
        resumeRows = [[thread()], new Promise<ResumeThread[]>((resolve) => { completeRefresh = resolve; })];
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        await screen.findByText("Fixed weekday aliases");
        fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
        expect(screen.queryByRole("button", { name: "View latest source for Parser" })).not.toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Refresh" })).toBeDisabled();
        await act(async () => completeRefresh([]));
        expect(await screen.findByText(/No recent work yet/)).toBeInTheDocument();
    });

    it("reloads when Home reopens and ignores an older unmounted request", async () => {
        let completeOld!: (rows: ResumeThread[]) => void;
        resumeRows = [new Promise<ResumeThread[]>((resolve) => { completeOld = resolve; }), [thread("Current")]];
        const first = render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        first.unmount();
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        await screen.findByRole("heading", { name: "Current" });
        await act(async () => completeOld([thread("Stale")]));
        expect(screen.queryByRole("heading", { name: "Stale" })).not.toBeInTheDocument();
        await waitFor(() => expect(calls("resume_work")).toHaveLength(2));
    });
});
