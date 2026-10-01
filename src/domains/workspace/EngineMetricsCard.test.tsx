import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { RuntimeMetricsSnapshot } from "@/shared/ipc/tauri";

const mocks = vi.hoisted(() => ({
    getMemoryReviewStatus: vi.fn(),
    getRuntimeMetrics: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => mocks);

import { EngineMetricsCard } from "./EngineMetricsCard";

function runtimeSnapshot(): RuntimeMetricsSnapshot {
    return {
        generated_at_ms: 1_790_115_600_000,
        process_rss_bytes: 0,
        capture: {
            frames_captured: 0,
            frames_dropped: 0,
            last_capture_time_ms: 0,
        },
        embedding: {
            backend: "local",
            degraded: false,
            detail: "ready",
            model_name: "bge-small",
            dimension: 384,
            clip_session_loaded: false,
            last_clip_infer_ms: 0,
        },
        inference: {
            ai_model_available: false,
            ai_model_loaded: false,
            loaded_model_id: null,
        },
        aggregates: {},
        counters: {},
        recent: [],
        system: {
            generated_at_ms: 1_790_115_600_000,
            sample_interval_ms: 1_000,
            process_cpu: {
                cpu_percent: 0,
                user_time_ms: 0,
                system_time_ms: 0,
                threads: 4,
            },
            process_memory: {
                rss_bytes: 0,
                virtual_bytes: 0,
                phys_footprint_bytes: 0,
                lifetime_max_phys_footprint_bytes: 0,
            },
            process_io: {
                disk_bytes_read: 0,
                disk_bytes_written: 0,
                disk_read_rate_bps: 0,
                disk_write_rate_bps: 0,
            },
            process_energy: {
                idle_wakeups: 0,
                interrupt_wakeups: 0,
                billed_system_time_ns: 0,
                label: "low",
            },
            host_cpu: {
                cpu_percent_total: 0,
                cpu_percent_per_core: [0, 25],
            },
            host_memory: {
                page_size_bytes: 4096,
                free_bytes: 0,
                active_bytes: 0,
                inactive_bytes: 0,
                wired_bytes: 0,
                compressed_bytes: 0,
                total_bytes: 0,
                pressure_label: "low",
            },
            gpu: {
                device_utilization_percent: null,
                renderer_utilization_percent: null,
                in_use_system_memory_bytes: 0,
                recovery_count: 0,
            },
            model_memory: [],
        },
    };
}

describe("EngineMetricsCard", () => {
    beforeEach(() => {
        mocks.getRuntimeMetrics.mockReset();
        mocks.getMemoryReviewStatus.mockReset().mockResolvedValue({
            queue_depth: 0,
            last_review_at_ms: 0,
            last_error_kind: null,
            worker_enabled: true,
            pressure_blocked: false,
        });
    });

    afterEach(() => cleanup());

    it("shows the observed diagnostics request while the first refresh is pending", async () => {
        mocks.getRuntimeMetrics.mockReturnValue(new Promise(() => {}));

        render(<EngineMetricsCard enabled />);

        const trace = await screen.findByRole("region", {
            name: "Engine diagnostics activity",
        });
        expect(within(trace).getByText("Requesting engine diagnostics")).toBeInTheDocument();
        expect(within(trace).getByText("Running")).toBeInTheDocument();
    });

    it("shows only safe aggregate metadata from a completed diagnostics refresh", async () => {
        const snapshot = runtimeSnapshot();
        snapshot.aggregates = {
            retrieval: {
                n: 2,
                sum_ms: 12,
                ewma_ms: 5,
                p50_ms: 4,
                p95_ms: 8,
                max_ms: 8,
                avg_ms: 6,
            },
        };
        snapshot.recent = [
            { op: "retrieval", ms: 5, ts_ms: 1_790_115_600_000, meta: null },
        ];
        mocks.getRuntimeMetrics.mockResolvedValue(snapshot);

        render(<EngineMetricsCard enabled />);

        const trace = await screen.findByRole("region", {
            name: "Engine diagnostics activity",
        });
        expect(await within(trace).findByText("Engine diagnostics refreshed")).toBeInTheDocument();
        fireEvent.click(within(trace).getByRole("button", { name: "Show Engine diagnostics activity details" }));
        expect(within(trace).getByText(/1 latency group · 1 recent operation/i)).toBeInTheDocument();
        expect(trace).toHaveTextContent("Verified result");
        expect(within(trace).queryByText("Running")).not.toBeInTheDocument();
        expect(within(trace).queryByText(/retrieval/i)).not.toBeInTheDocument();
    });

    it("does not leak native errors through the diagnostics trace or fallback", async () => {
        mocks.getRuntimeMetrics.mockRejectedValue(
            new Error("failed at /Users/person/vault.db via private.example"),
        );

        render(<EngineMetricsCard enabled />);

        const alert = await screen.findByRole("alert");
        expect(alert).toHaveTextContent("Engine diagnostics could not load");
        expect(alert).not.toHaveTextContent(/vault\.db|private\.example/);
        const trace = screen.getByRole("region", { name: "Engine diagnostics activity" });
        expect(within(trace).getByText("Engine diagnostics refresh failed")).toBeInTheDocument();
        expect(within(trace).getByText("Failed")).toBeInTheDocument();
    });

    it("explains empty successful snapshots and preserves real zero values", async () => {
        mocks.getRuntimeMetrics.mockResolvedValue(runtimeSnapshot());

        render(<EngineMetricsCard enabled />);

        expect(await screen.findByText("No latency samples yet.")).toBeInTheDocument();
        expect(screen.getByText("No recent operations yet.")).toBeInTheDocument();
        expect(screen.getAllByText("0 B").length).toBeGreaterThan(0);
        expect(screen.getByRole("meter", { name: "CPU core 0 usage" })).toHaveAttribute(
            "aria-valuenow",
            "0",
        );
    });

    it("keeps failures actionable with a manual retry", async () => {
        mocks.getRuntimeMetrics
            .mockRejectedValueOnce(new Error("metrics bridge unavailable"))
            .mockResolvedValueOnce(runtimeSnapshot());

        render(<EngineMetricsCard enabled />);

        expect(await screen.findByRole("alert")).toHaveTextContent("Engine diagnostics could not load");
        expect(screen.getByRole("alert")).not.toHaveTextContent("metrics bridge unavailable");
        fireEvent.click(screen.getByRole("button", { name: "Retry engine diagnostics" }));

        await waitFor(() => expect(mocks.getRuntimeMetrics).toHaveBeenCalledTimes(2));
        expect(await screen.findByText("No latency samples yet.")).toBeInTheDocument();
        expect(screen.queryByRole("alert")).not.toBeInTheDocument();
        const trace = screen.getByRole("region", { name: "Engine diagnostics activity" });
        expect(within(trace).getByText("Engine diagnostics refreshed")).toBeInTheDocument();
    });

    it("does not silently omit memory-review status when its request fails", async () => {
        mocks.getRuntimeMetrics.mockResolvedValue(runtimeSnapshot());
        mocks.getMemoryReviewStatus.mockRejectedValue(new Error("worker status unavailable"));

        render(<EngineMetricsCard enabled />);

        expect(await screen.findByText("unavailable")).toHaveAccessibleDescription(
            "Memory review status is unavailable.",
        );
        expect(screen.queryByText("worker status unavailable")).not.toBeInTheDocument();
    });
});
