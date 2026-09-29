import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Onboarding } from "./Onboarding";
import type { ModelDownloadStatus } from "@/shared/ipc/onboarding";
import type { ActivityTraceEvidence } from "@/shared/activity/activityTrace";

const listAvailableModels = vi.fn();
const downloadModel = vi.fn();
const saveOnboardingState = vi.fn();
const getOnboardingState = vi.fn();
const checkPermissions = vi.fn();
const refreshAiModels = vi.fn();

vi.mock("@/shared/ipc/onboarding", () => ({
    getOnboardingState: (...args: unknown[]) => getOnboardingState(...args),
    saveOnboardingState: (...args: unknown[]) => saveOnboardingState(...args),
    requestBiometricAuth: vi.fn(),
    checkPermissions: (...args: unknown[]) => checkPermissions(...args),
    openSystemSettings: vi.fn(),
    listAvailableModels: (...args: unknown[]) => listAvailableModels(...args),
    downloadModel: (...args: unknown[]) => downloadModel(...args),
    refreshAiModels: (...args: unknown[]) => refreshAiModels(...args),
}));

const downloadStatusValue: ModelDownloadStatus & {
    activity_evidence: ActivityTraceEvidence;
} = {
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
    activity_evidence: "backend-event",
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
    refreshAiModels.mockResolvedValue({ ai_model_available: true });
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
    downloadStatusValue.destination_path = null;
    downloadStatusValue.bytes_downloaded = 0;
    downloadStatusValue.total_bytes = 0;
    downloadStatusValue.percent = 0;
    downloadStatusValue.done = false;
    downloadStatusValue.error = null;
    downloadStatusValue.logs = [];
    downloadStatusValue.updated_at_ms = 0;
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
        const view = render(<Onboarding onComplete={() => {}} />);
        await waitFor(() => expect(downloadModel).toHaveBeenCalledTimes(1));

        downloadStatusValue.state = "completed";
        downloadStatusValue.model_id = "minilm-l6-v2";
        downloadStatusValue.done = true;
        downloadStatusValue.updated_at_ms = 1;
        view.rerender(<Onboarding onComplete={() => {}} />);

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

    it("shows observed model activity without exposing raw logs or local paths", async () => {
        listAvailableModels.mockResolvedValue([qwenInfo(), minilmInfo()]);
        downloadModel.mockResolvedValue(undefined);
        downloadStatusValue.state = "downloading";
        downloadStatusValue.model_id = "minilm-l6-v2";
        downloadStatusValue.bytes_downloaded = 45_000_000;
        downloadStatusValue.total_bytes = 90_000_000;
        downloadStatusValue.percent = 50;
        downloadStatusValue.destination_path = "/Users/private/models/all-MiniLM-L6-v2.onnx";
        downloadStatusValue.logs = ["raw downloader output containing a private path"];
        downloadStatusValue.updated_at_ms = 1_000;

        render(<Onboarding onComplete={() => {}} />);

        const activity = await screen.findByRole("region", { name: "Model setup activity" });
        expect(activity).toHaveTextContent("Downloading MiniLM · Search Embedder");
        expect(activity).toHaveTextContent("Model download service");
        expect(activity).toHaveTextContent("50%");
        expect(activity).toHaveTextContent("Live backend event");
        expect(activity).not.toHaveTextContent("/Users/private/models");
        expect(activity).not.toHaveTextContent("raw downloader output");
    });

    it("shows a safe failure state instead of backend error details", async () => {
        listAvailableModels.mockResolvedValue([qwenInfo(), minilmInfo()]);
        downloadModel.mockResolvedValue(undefined);
        const view = render(<Onboarding onComplete={() => {}} />);
        await waitFor(() => expect(downloadModel).toHaveBeenCalledTimes(1));

        downloadStatusValue.state = "failed";
        downloadStatusValue.model_id = "minilm-l6-v2";
        downloadStatusValue.error = "failed writing /Users/private/models/search.onnx";
        downloadStatusValue.updated_at_ms = 1_000;
        view.rerender(<Onboarding onComplete={() => {}} />);

        expect(await screen.findByRole("region", { name: "Model setup activity" })).toHaveTextContent(
            "Model download failed",
        );
        expect(screen.getByRole("alert")).toHaveTextContent(
            "The model download failed. Retry it or check your network connection.",
        );
        expect(screen.queryByText(/Users\/private\/models/)).toBeNull();
    });

    it("advances after a downloaded choice finishes activation", async () => {
        let resolveRuntime!: (value: { ai_model_available: boolean }) => void;
        refreshAiModels.mockReturnValue(new Promise((resolve) => {
            resolveRuntime = resolve;
        }));
        listAvailableModels.mockResolvedValue([qwenInfo(), minilmInfo(true)]);
        downloadModel.mockResolvedValue(undefined);
        const view = render(<Onboarding onComplete={() => {}} />);

        fireEvent.click(await screen.findByRole("button", { name: /Download Qwen3-VL · 2B/i }));
        await waitFor(() => expect(downloadModel).toHaveBeenCalledTimes(1));

        downloadStatusValue.state = "completed";
        downloadStatusValue.model_id = "qwen3-vl-2b";
        downloadStatusValue.done = true;
        downloadStatusValue.updated_at_ms = 2_000;
        view.rerender(<Onboarding onComplete={() => {}} />);

        const trace = await screen.findByRole("region", { name: "Model setup activity" });
        expect(within(trace).getByRole("status")).toHaveTextContent("Loading the model into FNDR");
        resolveRuntime({ ai_model_available: true });

        await waitFor(() => expect(saveOnboardingState).toHaveBeenCalledWith(
            expect.objectContaining({
                step: "permissions",
                model_downloaded: true,
                model_id: "qwen3-vl-2b",
            }),
        ));
    });
});

describe("Onboarding persistence", () => {
    it("shows the observed System Settings handoff without implying a grant", async () => {
        getOnboardingState.mockResolvedValue({
            step: "permissions",
            biometric_enabled: false,
            screen_permission: false,
            accessibility_permission: false,
            model_downloaded: false,
            model_id: null,
            display_name: null,
        });

        render(<Onboarding onComplete={() => {}} />);

        fireEvent.click((await screen.findAllByRole("button", { name: "Grant" }))[0]);
        const trace = await screen.findByRole("region", { name: "Permission check activity" });
        expect(trace).toHaveTextContent("System Settings opened");
        expect(trace).not.toHaveTextContent("Permission granted");
    });

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
