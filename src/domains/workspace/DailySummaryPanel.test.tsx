import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { DailySummaryPanel } from "./DailySummaryPanel";

const ipc = vi.hoisted(() => ({
    addTodo: vi.fn(),
    exportDailySummaryPdf: vi.fn(),
    generateDailySummaryForDate: vi.fn(),
    getDailySummaryFollowups: vi.fn(),
    getDailySummaryOverview: vi.fn(),
    openExportedPdf: vi.fn(),
    setTodoCompleted: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ipc);

beforeEach(() => {
    vi.clearAllMocks();
    ipc.generateDailySummaryForDate.mockImplementation(async (date: string) => `Summary for ${date}`);
    ipc.getDailySummaryOverview.mockResolvedValue("");
    ipc.getDailySummaryFollowups.mockResolvedValue([]);
});

afterEach(cleanup);

describe("DailySummaryPanel", () => {
    it("clears content tied to the previous date when the selected date changes", async () => {
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);

        const dateInput = screen.getByLabelText(/select summary date/i) as HTMLInputElement;
        const initialDate = dateInput.value;
        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));
        expect(await screen.findByText(`Summary for ${initialDate}`)).toBeInTheDocument();
        expect(screen.getByRole("button", { name: /download pdf/i })).toBeInTheDocument();

        fireEvent.change(dateInput, {
            target: { value: "2020-01-02" },
        });

        expect(screen.queryByText(`Summary for ${initialDate}`)).toBeNull();
        expect(screen.queryByRole("button", { name: /download pdf/i })).toBeNull();
        expect(screen.getByText(/generate a summary for thursday, january 2, 2020/i)).toBeInTheDocument();
    });

    it("describes follow-ups as current work instead of claiming they came from the selected day", async () => {
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);

        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        expect(await screen.findByText(/no open follow-ups right now/i)).toBeInTheDocument();
        expect(screen.queryByText(/no open follow-ups for this day/i)).toBeNull();
    });

    it("traps focus, closes on Escape, and restores the invoking control", async () => {
        const invoker = document.createElement("button");
        invoker.textContent = "Open Daily Summary";
        document.body.appendChild(invoker);
        invoker.focus();
        const onClose = vi.fn();
        const openMemory = vi.fn();
        const { rerender } = render(
            <DailySummaryPanel isVisible onClose={onClose} onOpenMemoryById={openMemory} />,
        );

        const dialog = screen.getByRole("dialog", { name: /daily summary/i });
        const closeButton = screen.getByRole("button", { name: /close daily summary/i });
        expect(dialog).toHaveAttribute("aria-modal", "true");
        await waitFor(() => expect(closeButton).toHaveFocus());
        await waitFor(() => {
            expect((screen.getByLabelText(/select summary date/i) as HTMLInputElement).value).not.toBe("");
        });

        const focusable = dialog.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled])");
        focusable[focusable.length - 1].focus();
        fireEvent.keyDown(document, { key: "Tab" });
        expect(closeButton).toHaveFocus();

        fireEvent.keyDown(document, { key: "Escape" });
        expect(onClose).toHaveBeenCalledOnce();
        rerender(<DailySummaryPanel isVisible={false} onClose={onClose} onOpenMemoryById={openMemory} />);
        expect(invoker).toHaveFocus();
        invoker.remove();
    });
});
