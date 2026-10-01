import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { FndrWrappedPanel } from "./FndrWrappedPanel";

const ipc = vi.hoisted(() => ({
    exportWeeklyWrappedPdf: vi.fn(),
    getWeeklyWrapped: vi.fn(),
    openExportedPdf: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ipc);

const recap = {
    start_date: "2026-09-21",
    end_date: "2026-09-22",
    generated_at_ms: Date.UTC(2026, 8, 22, 12),
    total_captures: 8,
    active_days: 2,
    total_minutes: 45,
    apps: [{ name: "Safari", captures: 6, duration_minutes: 30 }],
    websites: [{ name: "example.com", captures: 4, duration_minutes: 20 }],
    projects_and_topics: [],
    meeting_count: 1,
    document_count: 2,
    open_followups: 1,
    open_tasks: 2,
    busiest_day: { day: "Monday", count: 6 },
    busiest_hour: 10,
    most_revisited_file: null,
};

beforeEach(() => {
    vi.clearAllMocks();
    ipc.getWeeklyWrapped.mockResolvedValue(recap);
    ipc.exportWeeklyWrappedPdf.mockResolvedValue("/tmp/fndr-wrapped.pdf");
    ipc.openExportedPdf.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe("FndrWrappedPanel", () => {
    it("traces recap generation using only aggregate result metadata", async () => {
        render(<FndrWrappedPanel isVisible onClose={vi.fn()} />);

        fireEvent.click(screen.getByRole("button", { name: /start wrapped/i }));

        expect(await screen.findByText("Weekly recap ready")).toBeInTheDocument();
        const trace = screen.getByLabelText("FNDR Wrapped activity");
        fireEvent.click(within(trace).getByRole("button", { name: "Show FNDR Wrapped activity details" }));
        expect(within(trace).getByText("8 captures across 2 active days")).toBeInTheDocument();
        expect(within(trace).getByText(/verified result/i)).toBeInTheDocument();
    });

    it("exposes week choices as a named single-selection control", () => {
        render(<FndrWrappedPanel isVisible onClose={vi.fn()} />);

        const weekChoices = screen.getAllByRole("radio");
        expect(weekChoices.length).toBeGreaterThan(0);
        expect(weekChoices.some((choice) => (choice as HTMLInputElement).checked)).toBe(true);
    });

    it("reports an exported PDF open failure without dropping the recap", async () => {
        ipc.openExportedPdf.mockRejectedValueOnce(new Error("Preview unavailable"));
        render(<FndrWrappedPanel isVisible onClose={vi.fn()} />);

        fireEvent.click(screen.getByRole("button", { name: /start wrapped/i }));
        expect(await screen.findByText(/fndr recorded about/i)).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: /^export$/i }));
        fireEvent.click(await screen.findByRole("button", { name: /open pdf/i }));

        expect(await screen.findByRole("alert")).toHaveTextContent("Preview unavailable");
        expect(screen.getByText(/fndr recorded about/i)).toBeInTheDocument();
    });

    it("keeps selection available and announces recap generation errors", async () => {
        ipc.getWeeklyWrapped.mockRejectedValueOnce(new Error("Recap engine unavailable"));
        render(<FndrWrappedPanel isVisible onClose={vi.fn()} />);

        fireEvent.click(screen.getByRole("button", { name: /start wrapped/i }));

        expect(await screen.findByRole("alert")).toHaveTextContent("Recap engine unavailable");
        expect(screen.getByRole("button", { name: /start wrapped/i })).toBeInTheDocument();
    });

    it("does not let a recap from before close overwrite the reopened selection", async () => {
        let finishLoad!: (value: typeof recap) => void;
        ipc.getWeeklyWrapped.mockReturnValueOnce(new Promise<typeof recap>((resolve) => {
            finishLoad = resolve;
        }));
        const { rerender } = render(<FndrWrappedPanel isVisible onClose={vi.fn()} />);

        fireEvent.click(screen.getByRole("button", { name: /start wrapped/i }));
        await waitFor(() => expect(ipc.getWeeklyWrapped).toHaveBeenCalledOnce());

        rerender(<FndrWrappedPanel isVisible={false} onClose={vi.fn()} />);
        rerender(<FndrWrappedPanel isVisible onClose={vi.fn()} />);
        expect(screen.getByRole("button", { name: /start wrapped/i })).toBeEnabled();

        await act(async () => {
            finishLoad(recap);
        });

        expect(screen.queryByText(/fndr recorded about/i)).not.toBeInTheDocument();
        expect(screen.queryByText("Weekly recap ready")).not.toBeInTheDocument();
        expect(screen.getByRole("button", { name: /start wrapped/i })).toBeEnabled();
    });

    it("traps focus, closes on Escape, and restores the invoking control", async () => {
        const invoker = document.createElement("button");
        invoker.textContent = "Open Wrapped";
        document.body.appendChild(invoker);
        invoker.focus();
        const onClose = vi.fn();
        const { rerender } = render(<FndrWrappedPanel isVisible onClose={onClose} />);

        const dialog = screen.getByRole("dialog", { name: /fndr wrapped/i });
        const closeButton = screen.getByRole("button", { name: /close fndr wrapped/i });
        expect(dialog).toHaveAttribute("aria-modal", "true");
        await waitFor(() => expect(closeButton).toHaveFocus());
        await waitFor(() => expect(screen.getAllByRole("radio").length).toBeGreaterThan(0));

        const focusable = dialog.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled])");
        focusable[focusable.length - 1].focus();
        fireEvent.keyDown(document, { key: "Tab" });
        expect(closeButton).toHaveFocus();

        fireEvent.keyDown(document, { key: "Escape" });
        expect(onClose).toHaveBeenCalledOnce();
        rerender(<FndrWrappedPanel isVisible={false} onClose={onClose} />);
        expect(invoker).toHaveFocus();
        invoker.remove();
    });
});
