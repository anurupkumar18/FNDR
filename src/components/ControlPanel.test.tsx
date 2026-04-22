import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ControlPanel } from "./ControlPanel";

vi.mock("../api/tauri", () => ({
    deleteAllData: vi.fn(),
    deleteOlderThan: vi.fn(),
    getBlocklist: vi.fn().mockResolvedValue([]),
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
        last_error: null,
    }),
    getPrivacyAlerts: vi.fn().mockResolvedValue([]),
    getRetentionDays: vi.fn().mockResolvedValue(7),
    pauseCapture: vi.fn(),
    resumeCapture: vi.fn(),
    reclaimMemoryStorage: vi.fn(),
    runMemoryRepairBackfill: vi.fn(),
    setBlocklist: vi.fn(),
    setRetentionDays: vi.fn(),
    startMcpServer: vi.fn(),
    stopMcpServer: vi.fn(),
}));

vi.mock("../api/onboarding", () => ({
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
    it("exposes privacy alerts inside settings privacy", async () => {
        render(<ControlPanel status={null} compact={true} />);

        const settingsButton = screen.getByRole("button", { name: /open settings/i });
        expect(settingsButton).toBeInTheDocument();

        fireEvent.click(settingsButton);
        fireEvent.click(screen.getByRole("button", { name: /privacy/i }));

        expect(await screen.findByText(/no active privacy alerts/i)).toBeInTheDocument();
    });
});
