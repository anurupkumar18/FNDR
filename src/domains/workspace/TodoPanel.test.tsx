import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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

beforeEach(() => {
    vi.clearAllMocks();
    ipc.getTodos.mockResolvedValue([todoTask]);
    ipc.generateDailyBriefing.mockResolvedValue("");
    ipc.completeTodo.mockResolvedValue(true);
});

afterEach(cleanup);

describe("TodoPanel", () => {
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

    it("lets an explicitly selected empty stage stay selected", async () => {
        ipc.getTodos.mockResolvedValueOnce([{ ...todoTask, task_type: "Reminder" }]);
        render(<TodoPanel isVisible onClose={vi.fn()} />);

        await waitFor(() => expect(screen.getByRole("button", { name: /reminder tasks, 1/i })).toBeInTheDocument());
        fireEvent.click(screen.getByRole("button", { name: /reminder tasks/i }));
        expect(await screen.findByText(todoTask.title)).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: /to-do tasks/i }));

        const todoStage = screen.getByRole("button", { name: /to-do tasks/i });
        await waitFor(() => expect(todoStage).toHaveAttribute("aria-pressed", "true"));
        expect(screen.getByText(/no to-dos right now/i)).toBeInTheDocument();
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
