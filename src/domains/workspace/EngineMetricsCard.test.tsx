import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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

    it("announces the initial load instead of rendering a blank diagnostics card", () => {
        mocks.getRuntimeMetrics.mockReturnValue(new Promise(() => {}));

        render(<EngineMetricsCard enabled />);

        expect(screen.getByRole("status")).toHaveTextContent(
            "Loading live engine diagnostics",
        );
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

        expect(await screen.findByRole("alert")).toHaveTextContent(
            "metrics bridge unavailable",
        );
        fireEvent.click(screen.getByRole("button", { name: "Retry engine diagnostics" }));

        await waitFor(() => expect(mocks.getRuntimeMetrics).toHaveBeenCalledTimes(2));
        expect(await screen.findByText("No latency samples yet.")).toBeInTheDocument();
        expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });

    it("does not silently omit memory-review status when its request fails", async () => {
        mocks.getRuntimeMetrics.mockResolvedValue(runtimeSnapshot());
        mocks.getMemoryReviewStatus.mockRejectedValue(new Error("worker status unavailable"));

        render(<EngineMetricsCard enabled />);

        expect(await screen.findByText("unavailable")).toHaveAccessibleDescription(
            "worker status unavailable",
        );
    });
});
