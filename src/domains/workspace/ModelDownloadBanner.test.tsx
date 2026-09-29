import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ModelDownloadStatus } from "@/shared/ipc/onboarding";

const mocks = vi.hoisted(() => ({
    downloadModel: vi.fn(),
    listAvailableModels: vi.fn(),
    refreshAiModels: vi.fn(),
    status: {
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
    } as ModelDownloadStatus,
}));

vi.mock("@/shared/ipc/onboarding", async (importOriginal) => {
    const actual = await importOriginal<typeof import("@/shared/ipc/onboarding")>();
    return {
        ...actual,
        downloadModel: mocks.downloadModel,
        listAvailableModels: mocks.listAvailableModels,
        refreshAiModels: mocks.refreshAiModels,
    };
});

vi.mock("@/shared/hooks/useModelDownloadStatus", () => ({
    useModelDownloadStatus: () => mocks.status,
}));

import { ModelDownloadBanner } from "./ModelDownloadBanner";

const model = {
    id: "qwen3-vl-2b",
    name: "Qwen3-VL 2B",
    description: "Local visual language model",
    size_bytes: 10_000_000,
    size_label: "10 MB",
    quality_label: "Balanced",
    speed_label: "Fast",
    ram_gb: 4,
    recommended: true,
    required: true,
    filename: "qwen.gguf",
    download_url: "https://models.example.test/qwen.gguf",
};

beforeEach(() => {
    mocks.status = {
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
    };
    mocks.downloadModel.mockResolvedValue(undefined);
    mocks.listAvailableModels.mockResolvedValue([model]);
    mocks.refreshAiModels.mockResolvedValue({
        ai_model_available: true,
        ai_model_loaded: true,
        vlm_loaded: true,
        loaded_model_id: model.id,
        loaded_model_path: "/private/model/qwen.gguf",
        model_mode: "lightweight_vlm",
        vlm_status: "loaded",
        vlm_model_id: model.id,
    });
});

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("ModelDownloadBanner activity", () => {
    it("adopts an active backend download after the banner remounts", async () => {
        mocks.status = {
            ...mocks.status,
            state: "downloading",
            model_id: model.id,
            bytes_downloaded: 2_000_000,
            total_bytes: 10_000_000,
            percent: 20,
            updated_at_ms: 50,
        };

        render(<ModelDownloadBanner />);

        const trace = await screen.findByLabelText("Local model activity");
        await waitFor(() => expect(within(trace).getByRole("status")).toHaveTextContent(
            "Downloading Qwen3-VL 2B",
        ));
        fireEvent.click(within(trace).getByRole("button", { name: "Show Local model activity details" }));
        expect(trace).toHaveTextContent("20%");
    });

    it("ignores a retained terminal snapshot until a retry emits newer status", async () => {
        mocks.status = {
            ...mocks.status,
            state: "failed",
            model_id: model.id,
            error: "old failure",
            updated_at_ms: 100,
        };
        const view = render(<ModelDownloadBanner />);

        fireEvent.click(await screen.findByRole("button", { name: /Download Qwen3-VL 2B/i }));
        const trace = await screen.findByLabelText("Local model activity");
        await waitFor(() => expect(within(trace).getByRole("status")).toHaveTextContent(
            "download accepted",
        ));
        expect(trace).not.toHaveTextContent("Model download failed");

        mocks.status = {
            ...mocks.status,
            state: "downloading",
            error: null,
            percent: 10,
            bytes_downloaded: 1_000_000,
            total_bytes: 10_000_000,
            updated_at_ms: 101,
        };
        view.rerender(<ModelDownloadBanner />);

        await waitFor(() => expect(within(trace).getByRole("status")).toHaveTextContent(
            "Downloading Qwen3-VL 2B",
        ));
    });

    it("shows an evidence-backed download trace without exposing backend logs or paths", async () => {
        const view = render(<ModelDownloadBanner />);

        fireEvent.click(await screen.findByRole("button", { name: /Download Qwen3-VL 2B/i }));
        await waitFor(() => expect(mocks.downloadModel).toHaveBeenCalledTimes(1));

        mocks.status = {
            state: "downloading",
            model_id: model.id,
            filename: model.filename,
            download_url: model.download_url,
            destination_path: "/Users/example/Library/Application Support/com.fndr.app/models/qwen.gguf",
            temp_path: "/private/tmp/qwen.partial",
            bytes_downloaded: 4_000_000,
            total_bytes: 10_000_000,
            percent: 40,
            done: false,
            error: null,
            logs: ["Downloading https://secret.example/model to /Users/example/private/model"],
            updated_at_ms: 1_000,
        };
        view.rerender(<ModelDownloadBanner />);

        const trace = screen.getByLabelText("Local model activity");
        expect(within(trace).getByRole("status")).toHaveTextContent("Downloading Qwen3-VL 2B");
        expect(within(trace).getByRole("status")).toHaveTextContent("Model download service");
        expect(trace).toHaveTextContent("Running");

        fireEvent.click(within(trace).getByRole("button", { name: "Show Local model activity details" }));
        expect(trace).toHaveTextContent("40% · 4 MB of 10 MB");
        expect(trace).not.toHaveTextContent("secret.example");
        expect(trace).not.toHaveTextContent("/Users/example");
        expect(trace).not.toHaveTextContent("/private/tmp");
        expect(document.body).not.toHaveTextContent("secret.example");
        expect(document.body).not.toHaveTextContent("/Users/example");
        expect(document.body).not.toHaveTextContent("/private/tmp");

        mocks.status = {
            ...mocks.status,
            state: "failed",
            error: "write /Users/example/private/model failed for https://secret.example/model",
            updated_at_ms: 1_100,
        };
        view.rerender(<ModelDownloadBanner />);

        await waitFor(() => {
            expect(within(trace).getByRole("status")).toHaveTextContent("Model download failed");
        });
        expect(within(trace).getByRole("status")).toHaveTextContent("Failed");
        expect(document.body).not.toHaveTextContent("secret.example");
        expect(document.body).not.toHaveTextContent("/Users/example");
    });

    it("finishes activation after the completed download rerenders the banner", async () => {
        let resolveRuntime!: (value: Awaited<ReturnType<typeof mocks.refreshAiModels>>) => void;
        mocks.refreshAiModels.mockReturnValue(new Promise((resolve) => {
            resolveRuntime = resolve;
        }));
        const view = render(<ModelDownloadBanner />);

        fireEvent.click(await screen.findByRole("button", { name: /Download Qwen3-VL 2B/i }));
        await waitFor(() => expect(mocks.downloadModel).toHaveBeenCalledTimes(1));

        mocks.status = {
            ...mocks.status,
            state: "completed",
            model_id: model.id,
            done: true,
            updated_at_ms: 2_000,
        };
        view.rerender(<ModelDownloadBanner />);

        expect(await screen.findByText("Loading the model into FNDR")).toBeInTheDocument();
        resolveRuntime({
            ai_model_available: true,
            ai_model_loaded: true,
            vlm_loaded: true,
            loaded_model_id: model.id,
            loaded_model_path: "/private/model/qwen.gguf",
            model_mode: "lightweight_vlm",
            vlm_status: "loaded",
            vlm_model_id: model.id,
        });

        expect(await screen.findByText("Local model is ready")).toBeInTheDocument();
    });
});
