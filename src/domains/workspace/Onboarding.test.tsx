import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Onboarding } from "./Onboarding";

const listAvailableModels = vi.fn();
const downloadModel = vi.fn();
const saveOnboardingState = vi.fn();
const getOnboardingState = vi.fn();
const checkPermissions = vi.fn();

vi.mock("@/shared/ipc/onboarding", () => ({
    getOnboardingState: (...args: unknown[]) => getOnboardingState(...args),
    saveOnboardingState: (...args: unknown[]) => saveOnboardingState(...args),
    requestBiometricAuth: vi.fn(),
    checkPermissions: (...args: unknown[]) => checkPermissions(...args),
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

beforeEach(() => {
    getOnboardingState.mockResolvedValue({
        step: "model_download",
        biometric_enabled: false,
        screen_permission: false,
        accessibility_permission: false,
        model_downloaded: false,
        model_id: null,
        display_name: null,
    });
    checkPermissions.mockResolvedValue({
        screen_recording: false,
        accessibility: false,
        microphone: false,
    });
    saveOnboardingState.mockResolvedValue(undefined);
});

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

describe("Onboarding persistence", () => {
    it("keeps the current step visible when completing onboarding cannot be saved", async () => {
        getOnboardingState.mockResolvedValue({
            step: "permissions",
            biometric_enabled: false,
            screen_permission: false,
            accessibility_permission: false,
            model_downloaded: false,
            model_id: null,
            display_name: null,
        });
        saveOnboardingState.mockRejectedValueOnce(new Error("disk full"));
        const onComplete = vi.fn();

        render(<Onboarding onComplete={onComplete} />);

        fireEvent.click(await screen.findByRole("button", { name: "Skip for now" }));

        expect(await screen.findByRole("alert")).toHaveTextContent(
            "FNDR couldn't save your setup. Please try again.",
        );
        expect(screen.getByRole("heading", { name: "Grant a few permissions" })).toBeInTheDocument();
        expect(onComplete).not.toHaveBeenCalled();
    });
});

describe("Onboarding privacy disclosures", () => {
    it("introduces FNDR as local-first and names its network exceptions", async () => {
        getOnboardingState.mockResolvedValue({
            step: "welcome",
            biometric_enabled: false,
            screen_permission: false,
            accessibility_permission: false,
            model_downloaded: false,
            model_id: null,
            display_name: null,
        });

        render(<Onboarding onComplete={() => {}} />);

        expect(await screen.findByText(/stores your captured memory on this Mac/i)).toBeInTheDocument();
        expect(screen.getByText(/model downloads.*optional integrations/i)).toBeInTheDocument();
        expect(screen.queryByText(/nothing leaves it/i)).toBeNull();
    });

    it("explains capture controls without promising automatic protection", async () => {
        getOnboardingState.mockResolvedValue({
            step: "privacy_promise",
            biometric_enabled: false,
            screen_permission: false,
            accessibility_permission: false,
            model_downloaded: false,
            model_id: null,
            display_name: null,
        });

        render(<Onboarding onComplete={() => {}} />);

        expect(await screen.findByRole("heading", { name: "How FNDR handles your data" })).toBeInTheDocument();
        expect(await screen.findByText("Local-first, with clear exceptions")).toBeInTheDocument();
        expect(screen.getByText(/downloading models connects to Hugging Face/i)).toBeInTheDocument();
        expect(screen.getByText("Capture controls")).toBeInTheDocument();
        expect(screen.getByText(/review your blocklist and pause capture/i)).toBeInTheDocument();
        expect(screen.getByText(/deletion controls to remove saved memories/i)).toBeInTheDocument();
        expect(screen.queryByText(/nothing leaves your Mac/i)).toBeNull();
        expect(screen.queryByText(/doesn't share/i)).toBeNull();
        expect(screen.queryByText(/automatic privacy/i)).toBeNull();
        expect(screen.queryByText(/in one tap/i)).toBeNull();
    });

    it("describes screen capture as a capability rather than claiming every screen is stored", async () => {
        getOnboardingState.mockResolvedValue({
            step: "biometrics",
            biometric_enabled: false,
            screen_permission: false,
            accessibility_permission: false,
            model_downloaded: false,
            model_id: null,
            display_name: null,
        });

        render(<Onboarding onComplete={() => {}} />);

        expect(await screen.findByText(/can store screen snapshots and extracted text/i)).toBeInTheDocument();
        expect(screen.queryByText(/stores everything you see/i)).toBeNull();
    });

    it("discloses the model host before a model download", async () => {
        listAvailableModels.mockResolvedValue([qwenInfo(), minilmInfo(true)]);

        render(<Onboarding onComplete={() => {}} />);

        expect(await screen.findByText(/downloading connects to Hugging Face/i)).toBeInTheDocument();
        expect(screen.getByText(/supported analysis runs on this Mac/i)).toBeInTheDocument();
        expect(screen.getByText(/on-device transcription/i)).toBeInTheDocument();
        expect(screen.queryByText(/privacy-first transcription/i)).toBeNull();
    });
});
