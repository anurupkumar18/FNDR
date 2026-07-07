import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { Onboarding } from "./Onboarding";

const listAvailableModels = vi.fn();
const downloadModel = vi.fn();
const saveOnboardingState = vi.fn();

vi.mock("@/shared/ipc/onboarding", () => ({
    getOnboardingState: vi.fn().mockResolvedValue({
        step: "model_download",
        biometric_enabled: false,
        screen_permission: false,
        accessibility_permission: false,
        model_downloaded: false,
        model_id: null,
        display_name: null,
    }),
    saveOnboardingState: (...args: unknown[]) => saveOnboardingState(...args),
    requestBiometricAuth: vi.fn(),
    checkPermissions: vi.fn().mockResolvedValue({
        screen_recording: false,
        accessibility: false,
        microphone: false,
    }),
    openSystemSettings: vi.fn(),
    listAvailableModels: (...args: unknown[]) => listAvailableModels(...args),
    downloadModel: (...args: unknown[]) => downloadModel(...args),
    refreshAiModels: vi.fn().mockResolvedValue({ ai_model_available: true }),
}));

const downloadStatusValue = {
    state: "idle",
    model_id: null as string | null,
    filename: null,
    download_url: null,
    destination_path: null,
    temp_path: null,
    bytes_downloaded: 0,
    total_bytes: 0,
    percent: 0,
    done: false,
    error: null,
    logs: [] as string[],
    updated_at_ms: 0,
};

vi.mock("@/shared/hooks/useModelDownloadStatus", () => ({
    useModelDownloadStatus: () => downloadStatusValue,
}));

vi.mock("@/shared/hooks/usePolling", () => ({
    usePolling: vi.fn(),
}));

function qwenInfo(downloaded = false) {
    return {
        id: "qwen3-vl-2b",
        name: "Qwen3-VL · 2B",
        description: "Multimodal memory model.",
        size_bytes: 1_500_000_000,
        size_label: "~1.5 GB",
        quality_label: "Excellent",
        speed_label: "Balanced",
        ram_gb: 3.5,
        recommended: true,
        required: false,
        filename: "Qwen3VL-2B-Instruct-Q4_K_M.gguf",
        download_url: downloaded ? "already_downloaded" : "https://example.test/qwen.gguf",
    };
}

function minilmInfo(downloaded = false) {
    return {
        id: "minilm-l6-v2",
        name: "MiniLM · Search Embedder",
        description: "Required search embedding model (384-d).",
        size_bytes: 90_387_606,
        size_label: "~90 MB",
        quality_label: "Required",
        speed_label: "Fast",
        ram_gb: 0.5,
        recommended: false,
        required: true,
        filename: "all-MiniLM-L6-v2.onnx",
        download_url: downloaded ? "already_downloaded" : "https://example.test/model.onnx",
    };
}

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    downloadStatusValue.state = "idle";
    downloadStatusValue.model_id = null;
});

describe("Onboarding model step", () => {
    it("auto-downloads the required embedder and offers only optional models as choices", async () => {
        listAvailableModels.mockResolvedValue([qwenInfo(), minilmInfo()]);
        downloadModel.mockResolvedValue(undefined);

        render(<Onboarding onComplete={() => {}} />);

        await waitFor(() => {
            expect(downloadModel).toHaveBeenCalledWith(
                "minilm-l6-v2",
                "https://example.test/model.onnx",
                "all-MiniLM-L6-v2.onnx",
            );
        });

        // The required embedder is not a user choice card.
        expect(await screen.findByText("Qwen3-VL · 2B")).toBeInTheDocument();
        expect(screen.queryByText("MiniLM · Search Embedder")).toBeNull();
    });

    it("stays on the model step when the embedder download completes", async () => {
        listAvailableModels
            .mockResolvedValueOnce([qwenInfo(), minilmInfo()])
            .mockResolvedValue([qwenInfo(), minilmInfo(true)]);
        downloadModel.mockResolvedValue(undefined);
        downloadStatusValue.state = "completed";
        downloadStatusValue.model_id = "minilm-l6-v2";

        render(<Onboarding onComplete={() => {}} />);

        // Embedder completion refreshes the registry instead of advancing.
        await waitFor(() => {
            expect(listAvailableModels.mock.calls.length).toBeGreaterThanOrEqual(2);
        });
        expect(saveOnboardingState).not.toHaveBeenCalled();
    });

    it("does not auto-download when the embedder is already installed", async () => {
        listAvailableModels.mockResolvedValue([qwenInfo(), minilmInfo(true)]);

        render(<Onboarding onComplete={() => {}} />);

        expect(await screen.findByText("Qwen3-VL · 2B")).toBeInTheDocument();
        expect(downloadModel).not.toHaveBeenCalled();
    });
});
