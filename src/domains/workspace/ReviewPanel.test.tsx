import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ReviewPanel, reviewChoices } from "./ReviewPanel";
const ipc = vi.hoisted(() => ({ getReviewDates: vi.fn(), getWeeklyWrapped: vi.fn(), generateDailySummaryForDate: vi.fn(), getDailySummaryOverview: vi.fn(), getDailySummaryFollowups: vi.fn(), exportWeeklyWrappedPdf: vi.fn(), exportDailySummaryPdf: vi.fn(), openExportedPdf: vi.fn() }));
vi.mock("@/shared/ipc/tauri", () => ipc);
beforeEach(() => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 9, 22, 12));
    vi.clearAllMocks();
    ipc.getReviewDates.mockResolvedValue(["2026-09-22", "2026-07-03"]);
    ipc.exportDailySummaryPdf.mockResolvedValue("/Downloads/day.pdf");
    ipc.exportWeeklyWrappedPdf.mockResolvedValue("/Downloads/review.pdf");
    ipc.generateDailySummaryForDate.mockResolvedValue("Reviewed the project plan.");
    ipc.getDailySummaryOverview.mockResolvedValue("Project work");
    ipc.getDailySummaryFollowups.mockResolvedValue([]);
    ipc.getWeeklyWrapped.mockResolvedValue({ start_date: "2026-09-01", end_date: "2026-09-30", generated_at_ms: Date.now(), total_captures: 4, active_days: 1, total_minutes: 4, apps: [], websites: [], projects_and_topics: [], meeting_count: 0, document_count: 0, open_followups: 0, open_tasks: 0, busiest_day: null, busiest_hour: null, most_revisited_file: null });
});
afterEach(() => { cleanup(); vi.useRealTimers(); });
it("offers only periods with captures, omitting gaps and future dates", () => {
    const dates = ["2026-07-03", "2026-09-22", "2026-09-23", "2027-01-01"];
    expect(reviewChoices(dates, "month", "2026-10-22").map((x) => x.startDate)).toEqual(["2026-09-01", "2026-07-01"]);
    expect(reviewChoices(dates, "week", "2026-10-22").map((x) => x.startDate)).toEqual(["2026-09-21", "2026-06-29"]);
    expect(reviewChoices(dates, "day", "2026-10-22")).toHaveLength(3);
    expect(reviewChoices([], "month", "2026-10-22")).toEqual([]);
});
it("generates a day with the existing engine and returns to all three choices", async () => {
    render(<ReviewPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Generate day review" }));
    expect(await screen.findByText("Reviewed the project plan.")).toBeInTheDocument();
    expect(ipc.generateDailySummaryForDate).toHaveBeenCalledWith("2026-09-22");
    fireEvent.click(screen.getByRole("button", { name: /Back to review selection/ }));
    expect(screen.getByRole("button", { name: "Month" })).toBeInTheDocument();
});
it("routes Month through the recap engine with a full calendar range and can go back", async () => {
    render(<ReviewPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
    await screen.findByRole("button", { name: "Generate day review" });
    fireEvent.click(screen.getByRole("button", { name: "Month" }));
    expect(screen.getAllByRole("option")).toHaveLength(2);
    fireEvent.click(screen.getByRole("button", { name: "Generate month review" }));
    await waitFor(() => expect(ipc.getWeeklyWrapped).toHaveBeenCalledWith("2026-09-01", "2026-09-30"));
    expect((await screen.findAllByText("MONTH OVERVIEW")).length).toBeGreaterThan(0);
    fireEvent.click(screen.getByRole("button", { name: /Back to review selection/ }));
    expect(screen.getByRole("button", { name: "Month" })).toHaveAttribute("aria-pressed", "true");
});
it("does not offer generation for an empty capture history", async () => {
    ipc.getReviewDates.mockResolvedValue([]);
    render(<ReviewPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
    expect(await screen.findByText(/No captured activity yet/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Generate/ })).not.toBeInTheDocument();
});

it("caps current ranges, handles leap months and weeks across a year boundary", () => {
    expect(reviewChoices(["2026-10-22"], "month", "2026-10-22")).toEqual([{ startDate: "2026-10-01", endDate: "2026-10-22" }]);
    expect(reviewChoices(["2024-02-29"], "month", "2026-10-22")).toEqual([{ startDate: "2024-02-01", endDate: "2024-02-29" }]);
    expect(reviewChoices(["2026-01-01"], "week", "2026-10-22")).toEqual([{ startDate: "2025-12-29", endDate: "2026-01-04" }]);
    expect(reviewChoices(["2026-02-30", "invalid"], "day", "2026-10-22")).toEqual([]);
});

it("generates the selected older day rather than today", async () => {
    render(<ReviewPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
    fireEvent.change(await screen.findByRole("combobox", { name: "Available days" }), { target: { value: "2026-07-03" } });
    fireEvent.click(screen.getByRole("button", { name: "Generate day review" }));
    await waitFor(() => expect(ipc.generateDailySummaryForDate).toHaveBeenCalledWith("2026-07-03"));
    expect(screen.queryByRole("textbox", { name: /date/i })).not.toBeInTheDocument();
});

it("routes the selected week once and preserves it when returning", async () => {
    render(<ReviewPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
    await screen.findByRole("button", { name: "Generate day review" });
    fireEvent.click(screen.getByRole("button", { name: "Week" }));
    fireEvent.change(screen.getByRole("combobox", { name: "Available weeks" }), { target: { value: "2026-06-29" } });
    fireEvent.click(screen.getByRole("button", { name: "Generate week review" }));
    await screen.findAllByText("WEEK OVERVIEW");
    expect(ipc.getWeeklyWrapped).toHaveBeenCalledTimes(1);
    expect(ipc.getWeeklyWrapped).toHaveBeenCalledWith("2026-06-29", "2026-07-05");
    fireEvent.click(screen.getByRole("button", { name: /Back to review selection/ }));
    expect(screen.getByRole("combobox", { name: "Available weeks" })).toHaveValue("2026-06-29");
});

it("retries unavailable capture dates without offering an invented period", async () => {
    ipc.getReviewDates.mockRejectedValueOnce(new Error("Capture dates unavailable"));
    render(<ReviewPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Capture dates unavailable");
    expect(screen.queryByRole("button", { name: /Generate/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("combobox", { name: "Available days" })).toBeInTheDocument();
});

for (const period of ["day", "week", "month"] as const) {
    it(`shows the same header buttons, with no Update on a past ${period}`, async () => {
        render(<ReviewPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        await screen.findByRole("button", { name: "Generate day review" });
        fireEvent.click(screen.getByRole("button", { name: period[0].toUpperCase() + period.slice(1) }));
        fireEvent.click(screen.getByRole("button", { name: `Generate ${period} review` }));
        await screen.findByRole("button", { name: /Back to review selection/ });
        const header = document.querySelector(".wrapped-results-actions")!;
        expect([...header.querySelectorAll("button")].map((b) => b.getAttribute("aria-label") ?? b.textContent)).toEqual([
            "← Back to review selection",
            "Export",
            "Close FNDR Review",
        ]);
        expect(screen.queryByRole("button", { name: /Generate Summary|Download PDF/ })).not.toBeInTheDocument();
    });
}

it("exports the day summary as a PDF", async () => {
    render(<ReviewPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Generate day review" }));
    await screen.findByText("Reviewed the project plan.");
    fireEvent.click(screen.getByRole("button", { name: "Export" }));
    await waitFor(() => expect(ipc.exportDailySummaryPdf).toHaveBeenCalledWith("2026-09-22", "Reviewed the project plan."));
    expect(await screen.findByRole("button", { name: "Open PDF" })).toBeInTheDocument();
});

for (const [period, start, end] of [["week", "2026-09-01", "2026-09-30"], ["month", "2026-09-01", "2026-09-30"]] as const) {
    it(`exports the ${period} review as a PDF with its figures`, async () => {
        ipc.getWeeklyWrapped.mockResolvedValue({ ...(await ipc.getWeeklyWrapped()), start_date: start, end_date: end, apps: [{ name: "Safari", captures: 3, duration_minutes: 125 }], busiest_day: { day: "2026-09-22", count: 4 } });
        render(<ReviewPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        await screen.findByRole("button", { name: "Generate day review" });
        fireEvent.click(screen.getByRole("button", { name: period === "week" ? "Week" : "Month" }));
        fireEvent.click(screen.getByRole("button", { name: `Generate ${period} review` }));
        await screen.findAllByText(`${period.toUpperCase()} OVERVIEW`);
        fireEvent.click(screen.getByRole("button", { name: "Export" }));
        await waitFor(() => expect(ipc.exportWeeklyWrappedPdf).toHaveBeenCalled());
        const [from, to, text] = ipc.exportWeeklyWrappedPdf.mock.calls[0];
        expect([from, to]).toEqual([start, end]);
        expect(text).toContain("Captured memories: 4.");
        expect(text).toContain("1. Safari - 2h 5m");
        expect(text).toContain("Busiest day: 2026-09-22 (4 captures).");
    });
}

it("offers Update only for periods that include today", async () => {
    ipc.getReviewDates.mockResolvedValue(["2026-10-22", "2026-09-22"]);
    render(<ReviewPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Week" }));
    fireEvent.click(screen.getByRole("button", { name: "Generate week review" }));
    expect(await screen.findByRole("button", { name: "Update recap" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Back to review selection/ }));
    fireEvent.change(screen.getByRole("combobox", { name: "Available weeks" }), { target: { value: "2026-09-21" } });
    fireEvent.click(screen.getByRole("button", { name: "Generate week review" }));
    await screen.findByRole("button", { name: "Export" });
    expect(screen.queryByRole("button", { name: "Update recap" })).not.toBeInTheDocument();
});

it("does not rebuild a review when the parent re-renders", async () => {
    const props = { isVisible: true, onClose: vi.fn(), onOpenMemoryById: vi.fn() };
    const { rerender } = render(<ReviewPanel {...props} />);
    fireEvent.click(await screen.findByRole("button", { name: "Month" }));
    fireEvent.click(screen.getByRole("button", { name: "Generate month review" }));
    await screen.findAllByText("MONTH OVERVIEW");
    rerender(<ReviewPanel {...props} />);
    rerender(<ReviewPanel {...props} />);
    await Promise.resolve();
    expect(ipc.getWeeklyWrapped).toHaveBeenCalledTimes(1);
});
