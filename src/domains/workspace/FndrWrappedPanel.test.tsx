import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
