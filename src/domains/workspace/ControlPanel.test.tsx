import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ControlPanel } from "./ControlPanel";

const checkUpdateMock = vi.fn();

vi.mock("@tauri-apps/plugin-updater", () => ({
    check: (...args: unknown[]) => checkUpdateMock(...args),
}));

vi.mock("@tauri-apps/plugin-process", () => ({
    relaunch: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ({
    PRIVACY_ALERTS_EVENT: "privacy://alerts",
    CAPTURE_STATUS_EVENT: "capture://status",
    deleteAllData: vi.fn(),
    deleteOlderThan: vi.fn(),
    addSiteToBlocklist: vi.fn(),
    dismissPrivacyAlert: vi.fn(),
    fndrQualityStatus: vi.fn().mockResolvedValue({
        stored_count: 0,
        dropped_count: 0,
        flagged_count: 0,
    }),
    getBlocklist: vi.fn().mockResolvedValue([]),
    getAutofillSettings: vi.fn().mockResolvedValue({
        enabled: true,
        shortcut: "Alt+F",
        lookback_days: 90,
        auto_inject_threshold: 0.9,
        prefer_typed_injection: true,
        max_candidates: 4,
    }),
    getContextRuntimeStatus: vi.fn().mockResolvedValue({
        status: "healthy",
        active_project: null,
        current_context_pack_id: null,
        latest_pack_summary: "",
        tokens_used: 0,
        last_generated_at: null,
        recent_pack_count: 0,
        activity_event_count: 0,
        decision_count: 0,
        runtime_tables_ready: true,
    }),
    getMemoryRepairProgress: vi.fn(),
    getStorageHealth: vi.fn().mockResolvedValue({
        memory_db_bytes: 1024,
        frames_bytes: 0,
        models_bytes: 2048,
        dev_build_cache_bytes: 0,
        runtime_total_bytes: 3072,
        measured_at_ms: 0,
    }),
    getStorageReclaimProgress: vi.fn(),
    getMcpServerStatus: vi.fn().mockResolvedValue({
        running: false,
        host: "127.0.0.1",
        port: 8799,
        endpoint: "http://127.0.0.1:8799/mcp",
        require_auth: false,
        auth_mode: "disabled for localhost",
        last_error: null,
    }),
    getPrivacyAlerts: vi.fn().mockResolvedValue([]),
    getRetentionDays: vi.fn().mockResolvedValue(7),
    pauseCapture: vi.fn(),
    resumeCapture: vi.fn(),
    reclaimMemoryStorage: vi.fn(),
    runMemoryRepairBackfill: vi.fn(),
    setBlocklist: vi.fn(),
    setAutofillSettings: vi.fn(),
    setRetentionDays: vi.fn(),
    startMcpServer: vi.fn(),
    stopMcpServer: vi.fn(),
}));

vi.mock("@/shared/ipc/onboarding", () => ({
    deleteAiModel: vi.fn(),
    downloadModel: vi.fn(),
    getModelDownloadStatus: vi.fn().mockResolvedValue({
        state: "idle",
        model_id: null,
        filename: null,
        download_url: null,
        destination_path: null,
        temp_path: null,
        bytes_downloaded: 0,
        total_bytes: 0,
        percent: 0,
        done: false,
        error: null,
        logs: [],
        updated_at_ms: 0,
    }),
    getOnboardingState: vi.fn().mockResolvedValue({
        step: "complete",
        model_downloaded: true,
        display_name: null,
        biometric_enabled: false,
    }),
    listAvailableModels: vi.fn().mockResolvedValue([]),
    onDownloadStatus: vi.fn().mockResolvedValue(() => {}),
    refreshAiModels: vi.fn(),
    saveOnboardingState: vi.fn(),
}));

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("ControlPanel", () => {
    it("shows one demo-safe settings sheet without destructive or model-management controls", async () => {
        render(<ControlPanel status={null} compact={true} />);

        const settingsButton = screen.getByRole("button", { name: /open settings/i });
        fireEvent.click(settingsButton);

        expect(await screen.findByRole("heading", { name: /profile/i })).toBeInTheDocument();
        expect(screen.getByRole("heading", { name: /capture status/i })).toBeInTheDocument();
        expect(screen.getByRole("heading", { name: /privacy alerts/i })).toBeInTheDocument();
        expect(screen.getByRole("heading", { name: /blocked apps & sites/i })).toBeInTheDocument();
        expect(screen.getByRole("heading", { name: /local models/i })).toBeInTheDocument();
        expect(await screen.findByText(/no active privacy alerts/i)).toBeInTheDocument();
        expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
        expect(screen.queryByText(/danger zone/i)).toBeNull();
        expect(screen.queryByRole("button", { name: /delete/i })).toBeNull();
        expect(screen.queryByRole("button", { name: /download/i })).toBeNull();
        expect(screen.queryByText(/auto-fill/i)).toBeNull();
        expect(screen.queryByText(/mcp server/i)).toBeNull();
    });

    it("shows a read-only warning when capture cannot use its embedding model", async () => {
        render(<ControlPanel status={statusWithEmbedder("unavailable", true)} compact={true} />);

        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));

        expect(
            await screen.findByText(/capture is paused until the embedding model/i),
        ).toBeInTheDocument();
    });

    it("does not show the capture-paused warning when the embedder is healthy", async () => {
        render(<ControlPanel status={statusWithEmbedder("real", false)} compact={true} />);

        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));

        expect(await screen.findByRole("heading", { name: /local models/i })).toBeInTheDocument();
        expect(screen.queryByText(/capture is paused until the embedding model/i)).toBeNull();
    });
});

function statusWithEmbedder(backend: string, degraded: boolean) {
    return {
        is_capturing: false,
        is_paused: false,
        is_incognito: false,
        frames_captured: 0,
        frames_dropped: 0,
        last_capture_time: 0,
        ai_model_available: true,
        ai_model_loaded: false,
        loaded_model_id: null,
        embedding_backend: backend,
        embedding_degraded: degraded,
        embedding_detail: "",
        embedding_model_name: "all-MiniLM-L6-v2",
        embedding_dimension: 384,
        pipeline: {
            evaluated: 0,
            stored_ocr_path: 0,
            stored_visual_path: 0,
            stored_url_only: 0,
            stored_total: 0,
            skipped_blocklist: 0,
            skipped_self_app: 0,
            skipped_surface_policy: 0,
            skipped_perceptual_dup: 0,
            skipped_semantic_dup: 0,
            skipped_ocr_failed: 0,
            skipped_low_signal_text: 0,
            skipped_noise: 0,
            skipped_grounding: 0,
            skipped_stacked_extraction: 0,
            skipped_visual_small: 0,
            skipped_visual_novelty: 0,
            skipped_visual_compose_failed: 0,
            skipped_screen_capture_failed: 0,
            skipped_embedder_unavailable: 0,
            skipped_total: 0,
            last_skip_reason: null,
            last_skip_app: null,
            last_skip_timestamp_ms: null,
        },
    };
}
