import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { StatsPanel } from "./StatsPanel";

const ipc = vi.hoisted(() => ({
    getStats: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ipc);

const stats = {
    total_records: 12,
    total_days: 2,
    apps: [{ name: "Safari", count: 8 }],
    today_count: 4,
    unique_apps: 2,
    unique_sessions: 2,
    unique_window_titles: 4,
    unique_urls: 3,
    unique_domains: 2,
    records_with_url: 6,
    records_with_screenshot: 12,
    records_with_clean_text: 12,
    records_last_hour: 1,
    records_last_24h: 4,
    records_last_7d: 12,
    avg_records_per_active_day: 6,
    avg_records_per_hour: 1,
    focus_app_share_pct: 66.7,
    app_switches: 3,
    app_switch_rate_per_hour: 1.5,
    avg_gap_minutes: 3,
    longest_gap_minutes: 12,
    first_capture_ts: Date.UTC(2026, 8, 21, 9),
    last_capture_ts: Date.UTC(2026, 8, 22, 12),
    capture_span_hours: 27,
    current_streak_days: 2,
    longest_streak_days: 2,
    avg_ocr_confidence: 0.92,
    low_confidence_records: 1,
    avg_noise_score: 0.08,
    high_noise_records: 0,
    avg_ocr_blocks: 3,
    llm_count: 0,
    vlm_count: 0,
    fallback_count: 12,
    other_summary_count: 0,
    top_domains: [{ domain: "example.com", count: 4 }],
    busiest_day: { day: "Monday", count: 8 },
    quietest_day: { day: "Tuesday", count: 4 },
    busiest_hour: { hour: 10, count: 4 },
    hourly_distribution: [{ hour: 10, count: 4 }],
    weekday_distribution: [{ weekday: "Monday", count: 8 }],
    daypart_distribution: [{ daypart: "Morning", count: 12 }],
};

beforeEach(() => {
    vi.clearAllMocks();
    ipc.getStats.mockResolvedValue(stats);
});

afterEach(cleanup);

describe("StatsPanel", () => {
    it("does not expose static grid cards as fake buttons", async () => {
        render(<StatsPanel isVisible onClose={vi.fn()} />);

        expect(await screen.findByText("Live Pulse Board")).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: /bring live pulse board to front/i })).toBeNull();
    });

    it("makes stacked cards keyboard-operable and names the active card", async () => {
        render(<StatsPanel isVisible onClose={vi.fn()} />);

        await screen.findByText("Live Pulse Board");
        fireEvent.click(screen.getByRole("button", { name: /stack cards/i }));

        const insightsCard = screen.getByRole("button", { name: /bring intelligence brief to front/i });
        fireEvent.keyDown(insightsCard, { key: "Enter" });

        expect(insightsCard).toHaveAttribute("aria-pressed", "true");
    });

    it("shows a purposeful empty state when no captures exist", async () => {
        ipc.getStats.mockResolvedValueOnce({ ...stats, total_records: 0, apps: [], top_domains: [] });
        render(<StatsPanel isVisible onClose={vi.fn()} />);

        expect(await screen.findByText(/no captured activity yet/i)).toBeInTheDocument();
        expect(screen.queryByText("Live Pulse Board")).toBeNull();
    });

    it("traps focus, closes on Escape, and restores the invoking control", async () => {
        const invoker = document.createElement("button");
        invoker.textContent = "Open Stats";
        document.body.appendChild(invoker);
        invoker.focus();
        const onClose = vi.fn();
        const { rerender } = render(<StatsPanel isVisible onClose={onClose} />);

        const dialog = screen.getByRole("dialog", { name: /activity stats/i });
        const closeButton = screen.getByRole("button", { name: /close activity stats/i });
        expect(dialog).toHaveAttribute("aria-modal", "true");
        await waitFor(() => expect(closeButton).toHaveFocus());
        await screen.findByText("Live Pulse Board");

        const focusable = dialog.querySelectorAll<HTMLElement>("button:not([disabled])");
        focusable[focusable.length - 1].focus();
        fireEvent.keyDown(document, { key: "Tab" });
        expect(focusable[0]).toHaveFocus();

        fireEvent.keyDown(document, { key: "Escape" });
        expect(onClose).toHaveBeenCalledOnce();
        rerender(<StatsPanel isVisible={false} onClose={onClose} />);
        expect(invoker).toHaveFocus();
        invoker.remove();
    });
});
