import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { TodoPanel } from "./TodoPanel";

const ipc = vi.hoisted(() => ({
    addTodo: vi.fn(),
    completeTodo: vi.fn(),
    dismissTodo: vi.fn(),
    generateDailyBriefing: vi.fn(),
    getTodos: vi.fn(),
    updateTodo: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ipc);

const suggestions = vi.hoisted(() => ({ acceptSuggestion: vi.fn() }));
vi.mock("./todoSuggestions", async (importOriginal) => ({
    ...(await importOriginal<typeof import("./todoSuggestions")>()),
    acceptSuggestion: suggestions.acceptSuggestion,
}));

const todoTask = {
    id: "task-1",
    title: "Review the demo flow",
    description: "",
    source_app: "FNDR",
    source_memory_id: null,
    created_at: Date.UTC(2026, 8, 22, 12),
    due_date: null,
    is_completed: false,
    is_dismissed: false,
    task_type: "Todo" as const,
    linked_urls: [],
    linked_memory_ids: [],
};

const suggestion = {
    ...todoTask,
    id: "task-s",
    title: "Send Priya the draft report",
    description: "can you send me the draft report by Friday",
    source_app: "Memory:Mail",
    source_memory_id: "mem-1",
    task_type: "Followup" as const,
};

beforeEach(() => {
    vi.clearAllMocks();
    ipc.getTodos.mockResolvedValue([todoTask]);
    ipc.generateDailyBriefing.mockResolvedValue("");
    ipc.completeTodo.mockResolvedValue(true);
});

afterEach(cleanup);

describe("TodoPanel", () => {
    it("says what will appear when there are no tasks and no briefing", async () => {
        ipc.getTodos.mockResolvedValueOnce([]);
        render(<TodoPanel isVisible onClose={vi.fn()} />);

        expect(await screen.findByText(/Nothing on your list/)).toBeInTheDocument();
        expect(screen.getByText(/FNDR suggests it here with\s+the words it read/)).toBeInTheDocument();
        expect(screen.queryByText("My tasks")).toBeNull();
        expect(screen.queryByText("Suggested")).toBeNull();
    });

    it("traces the daily briefing request without exposing its text", async () => {
        ipc.generateDailyBriefing.mockResolvedValueOnce("Prioritize the release checklist.");
        render(<TodoPanel isVisible onClose={vi.fn()} />);

        const trace = await screen.findByLabelText("Daily briefing activity");
        expect(within(trace).getByText("Daily briefing ready")).toBeInTheDocument();
        fireEvent.click(within(trace).getByRole("button", { name: "Show Daily briefing activity details" }));
        expect(within(trace).getByText(/verified result/i)).toBeInTheDocument();
        expect(within(trace).queryByText("Prioritize the release checklist.")).toBeNull();
    });

    it("shows no briefing block when there is nothing to brief on", async () => {
        render(<TodoPanel isVisible onClose={vi.fn()} />);

        await screen.findByText(todoTask.title);
        await waitFor(() => expect(screen.queryByText(/generating your summary/i)).toBeNull());
        expect(screen.queryByText(/today's briefing/i)).toBeNull();
        expect(screen.queryByLabelText("Daily briefing activity")).toBeNull();
    });

    it("keeps the task list usable when an edit fails", async () => {
        ipc.updateTodo.mockRejectedValueOnce(new Error("Could not save that title"));
        render(<TodoPanel isVisible onClose={vi.fn()} />);

        expect(await screen.findByText(todoTask.title)).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: `Edit ${todoTask.title}` }));
        fireEvent.change(screen.getByRole("textbox", { name: `Task title for ${todoTask.title}` }), {
            target: { value: "Updated title" },
        });
        fireEvent.click(screen.getByRole("button", { name: /save task/i }));

        expect(await screen.findByRole("alert")).toHaveTextContent("Could not save that title");
        expect(screen.getByText(todoTask.title)).toBeInTheDocument();
        expect(screen.getByRole("button", { name: `Edit ${todoTask.title}` })).toBeInTheDocument();
    });

    it("keeps what FNDR noticed apart from the person's own tasks, with the words it saw", async () => {
        ipc.getTodos.mockResolvedValueOnce([todoTask, suggestion]);
        render(<TodoPanel isVisible onClose={vi.fn()} />);

        const mine = await screen.findByRole("region", { name: /my tasks/i });
        expect(within(mine).getByText(todoTask.title)).toBeInTheDocument();
        expect(within(mine).queryByText(suggestion.title)).toBeNull();

        const suggested = screen.getByRole("region", { name: /suggested/i });
        expect(within(suggested).getByText(suggestion.title)).toBeInTheDocument();
        expect(within(suggested).getByText(/can you send me the draft report by Friday/)).toBeInTheDocument();
        expect(within(suggested).getByText(/seen in mail/i)).toBeInTheDocument();
        // A suggestion is not a commitment: it cannot be marked done.
        expect(within(suggested).queryByRole("button", { name: /done/i })).toBeNull();
    });

    it("moves an accepted suggestion into the person's tasks", async () => {
        ipc.getTodos.mockResolvedValueOnce([suggestion]);
        suggestions.acceptSuggestion.mockResolvedValueOnce({ ...suggestion, source_app: "Accepted:Mail" });
        render(<TodoPanel isVisible onClose={vi.fn()} />);

        fireEvent.click(await screen.findByRole("button", { name: `Add ${suggestion.title} to my tasks` }));

        await waitFor(() => expect(suggestions.acceptSuggestion).toHaveBeenCalledWith(suggestion));
        const mine = await screen.findByRole("region", { name: /my tasks/i });
        expect(await within(mine).findByText(suggestion.title)).toBeInTheDocument();
        expect(screen.queryByRole("region", { name: /suggested/i })).toBeNull();
    });

    it("drops a suggestion that is not a task without completing it", async () => {
        ipc.getTodos.mockResolvedValueOnce([suggestion]);
        ipc.dismissTodo.mockResolvedValueOnce(true);
        render(<TodoPanel isVisible onClose={vi.fn()} />);

        fireEvent.click(await screen.findByRole("button", { name: `${suggestion.title} is not a task` }));

        await waitFor(() => expect(ipc.dismissTodo).toHaveBeenCalledWith(suggestion.id));
        expect(ipc.completeTodo).not.toHaveBeenCalled();
        expect(screen.queryByText(suggestion.title)).toBeNull();
    });

    it("shows a task's type and age in one short line", async () => {
        ipc.getTodos.mockResolvedValueOnce([
            { ...todoTask, task_type: "Reminder", created_at: Date.now() - 3 * 60 * 60_000 },
        ]);
        render(<TodoPanel isVisible onClose={vi.fn()} />);

        const mine = await screen.findByRole("region", { name: /my tasks/i });
        expect(within(mine).getByText("Reminder · 3 h ago")).toBeInTheDocument();
    });

    it("marks Done tasks complete rather than dismissing them", async () => {
        render(<TodoPanel isVisible onClose={vi.fn()} />);

        await screen.findByText(todoTask.title);
        fireEvent.click(screen.getByRole("button", { name: `Mark ${todoTask.title} done` }));

        await waitFor(() => expect(ipc.completeTodo).toHaveBeenCalledWith(todoTask.id));
        expect(ipc.dismissTodo).not.toHaveBeenCalled();
        expect(screen.queryByText(todoTask.title)).toBeNull();
    });

    it("labels fields, traps focus, closes on Escape, and restores its invoker", async () => {
        const invoker = document.createElement("button");
        invoker.textContent = "Open To-dos";
        document.body.appendChild(invoker);
        invoker.focus();
        const onClose = vi.fn();
        const { rerender } = render(<TodoPanel isVisible onClose={onClose} />);

        const dialog = screen.getByRole("dialog", { name: /to-dos/i });
        const closeButton = screen.getByRole("button", { name: /close to-dos/i });
        expect(dialog).toHaveAttribute("aria-modal", "true");
        expect(screen.getByRole("textbox", { name: /new task title/i })).toBeInTheDocument();
        expect(screen.getByRole("combobox", { name: /task type/i })).toBeInTheDocument();
        await waitFor(() => expect(closeButton).toHaveFocus());
        await screen.findByText(todoTask.title);

        const focusable = dialog.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), select:not([disabled])");
        focusable[focusable.length - 1].focus();
        fireEvent.keyDown(document, { key: "Tab" });
        expect(closeButton).toHaveFocus();

        fireEvent.keyDown(document, { key: "Escape" });
        expect(onClose).toHaveBeenCalledOnce();
        rerender(<TodoPanel isVisible={false} onClose={onClose} />);
        expect(invoker).toHaveFocus();
        invoker.remove();
    });
});
