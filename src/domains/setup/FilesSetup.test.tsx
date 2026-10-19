import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const files = vi.hoisted(() => ({
    FILES_STATUS_EVENT: "files://status",
    FILES_DOWNLOAD_EVENT: "files://download",
    filesSettingsGet: vi.fn(),
    filesSettingsSet: vi.fn(),
    filesStatus: vi.fn(),
    filesSuggestedFolders: vi.fn(),
    filesEstimate: vi.fn(),
    filesModelDownload: vi.fn(),
    filesPickFolder: vi.fn(),
    filesEditors: vi.fn(),
    filesForget: vi.fn(),
}));
vi.mock("@/shared/ipc/files", () => files);
vi.mock("@/shared/hooks/useTauriEvent", () => ({ useTauriEvent: () => undefined }));

import { FilesSetup, duration } from "./FilesSetup";

const off = {
    enabled: false, folders: [], excluded_folders: [], excluded_types: "", code_folders: [], code_editor: null,
    include_datasets: false, paused: false, hours: { kind: "always" },
};
const status = (state: string, model_ready: boolean) => ({
    state, detail: null, counts: { files: 0, done: 0, pending: 0, failed: 0, vectors: 0 }, model_ready,
    helper_found: true, reader_found: true,
});

describe("FilesSetup", () => {
    beforeEach(() => {
        for (const mock of Object.values(files)) if (typeof mock === "function") mock.mockReset();
        files.filesSettingsGet.mockResolvedValue(off);
        files.filesStatus.mockResolvedValue(status("off", false));
        files.filesSettingsSet.mockResolvedValue(status("needs_model", false));
        files.filesSuggestedFolders.mockResolvedValue(["/Users/a/Documents", "/Users/a/Desktop", "/Users/a/Downloads", "/Users/a/Pictures"]);
        files.filesEstimate.mockImplementation((folders: string[]) =>
            Promise.resolve({
                folders: folders.map((folder) => ({ folder, files: { pdf: 10, image: 2 }, seconds: 600, later_seconds: 0, skipped: {} })),
                seconds: 600 * folders.length,
                later_seconds: 0,
            }),
        );
        files.filesModelDownload.mockResolvedValue(undefined);
        files.filesEditors.mockResolvedValue([{ id: "dev.zed.Zed", name: "Zed" }]);
    });

    afterEach(() => cleanup());

    it("offers the usual folders with their first-pass time, and downloads the model only on yes", async () => {
        const onContinue = vi.fn();
        render(<FilesSetup mode="onboarding" onContinue={onContinue} />);
        const documents = await screen.findByRole("checkbox", { name: /Documents/ });
        expect(documents).toBeChecked();
        expect(screen.getByRole("checkbox", { name: /Pictures/ })).not.toBeChecked();
        expect(await screen.findAllByText("12 files · about 10 min")).toHaveLength(3);
        expect(files.filesModelDownload).not.toHaveBeenCalled();

        fireEvent.click(screen.getByRole("button", { name: "Search these folders" }));
        await waitFor(() =>
            expect(files.filesSettingsSet).toHaveBeenCalledWith({
                ...off,
                enabled: true,
                folders: ["/Users/a/Documents", "/Users/a/Desktop", "/Users/a/Downloads"],
            }),
        );
        await waitFor(() => expect(files.filesModelDownload).toHaveBeenCalled());
        expect(onContinue).toHaveBeenCalled();
    });

    it("lets the person decline without saving anything", async () => {
        const onContinue = vi.fn();
        render(<FilesSetup mode="onboarding" onContinue={onContinue} />);
        fireEvent.click(await screen.findByRole("button", { name: "Not now" }));
        expect(onContinue).toHaveBeenCalled();
        expect(files.filesSettingsSet).not.toHaveBeenCalled();
        expect(files.filesModelDownload).not.toHaveBeenCalled();
    });

    it("keeps every choice in Settings", async () => {
        files.filesSettingsGet.mockResolvedValue({ ...off, enabled: true, folders: ["/Users/a/Documents"] });
        files.filesStatus.mockResolvedValue({ ...status("indexing", true), counts: { files: 40, done: 12, pending: 28, failed: 1, vectors: 90 } });
        files.filesSettingsSet.mockResolvedValue(status("indexing", true));
        render(<FilesSetup mode="settings" />);
        expect(await screen.findByRole("status")).toHaveTextContent("Reading files · 12 of 40 files read, 1 could not be read");
        expect(await screen.findByRole("option", { name: "Zed" })).toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Pause indexing" }));
        await waitFor(() => expect(files.filesSettingsSet).toHaveBeenLastCalledWith(expect.objectContaining({ paused: true })));

        fireEvent.click(screen.getByRole("checkbox", { name: /Documents/ }));
        await waitFor(() => expect(files.filesSettingsSet).toHaveBeenLastCalledWith(expect.objectContaining({ folders: [] })));
    });

    it("says how long in words", () => {
        expect(duration(20)).toBe("under a minute");
        expect(duration(360)).toBe("about 6 min");
        expect(duration(4800)).toBe("about 1 h 20 min");
    });
});
