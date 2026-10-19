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
    filesFolders: vi.fn(),
    filesSubfolders: vi.fn(),
    filesSuggestedCodeFolders: vi.fn(),
    filesModelImport: vi.fn(),
    filesModelLinks: vi.fn(),
    filesOpenPrivacySettings: vi.fn(),
}));
vi.mock("@/shared/ipc/files", () => files);
vi.mock("@/shared/hooks/useTauriEvent", () => ({ useTauriEvent: () => undefined }));

import { FilesSetup, duration, kindCounts, statusText, timeText } from "./FilesSetup";

const off = {
    enabled: false, folders: [], excluded_folders: [], excluded_types: "", code_folders: [], code_editor: null,
    include_datasets: false, paused: false, hours: { kind: "always" }, index_on_battery: true,
};
const none = { files: 0, done: 0, pending: 0, failed: 0, vectors: 0 };
const status = (state: string, model_ready: boolean) => ({
    state, detail: null, counts: none, code_counts: none, code_bytes: 0, model_ready,
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
        files.filesFolders.mockResolvedValue([]);
        files.filesSubfolders.mockResolvedValue([]);
        files.filesSuggestedCodeFolders.mockResolvedValue(["/Users/a/Developer"]);
        files.filesModelImport.mockResolvedValue([]);
        files.filesModelLinks.mockResolvedValue(["https://a", "https://b"]);
        files.filesOpenPrivacySettings.mockResolvedValue(undefined);
    });

    afterEach(() => cleanup());

    it("offers the usual folders with their first-pass time, and downloads the model only on yes", async () => {
        const onContinue = vi.fn();
        render(<FilesSetup mode="onboarding" onContinue={onContinue} />);
        const documents = await screen.findByRole("checkbox", { name: /Documents/ });
        expect(documents).toBeChecked();
        expect(screen.getByRole("checkbox", { name: /Pictures/ })).not.toBeChecked();
        expect(await screen.findAllByText("10 PDFs, 2 pictures · about 10 min")).toHaveLength(3);
        const developer = await screen.findByRole("checkbox", { name: "Developer" });
        expect(developer).not.toBeChecked();
        fireEvent.click(developer);
        fireEvent.click(screen.getByRole("checkbox", { name: "Index on battery power too" }));
        expect(files.filesModelDownload).not.toHaveBeenCalled();

        fireEvent.click(screen.getByRole("button", { name: "Search these folders" }));
        await waitFor(() =>
            expect(files.filesSettingsSet).toHaveBeenCalledWith({
                ...off,
                enabled: true,
                folders: ["/Users/a/Documents", "/Users/a/Desktop", "/Users/a/Downloads"],
                code_folders: ["/Users/a/Developer"],
                index_on_battery: false,
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
        files.filesFolders.mockResolvedValue([
            { folder: "/Users/a/Documents", reach: "ok", files: 40, done: 12, kinds: { pdf: 30, image: 10 } },
        ]);
        files.filesSubfolders.mockResolvedValue(["/Users/a/Documents/Taxes"]);
        render(<FilesSetup mode="settings" />);
        expect(await screen.findByRole("status")).toHaveTextContent("Reading files · 12 of 40 files read, 1 could not be read");
        expect(await screen.findByRole("option", { name: "Zed" })).toBeInTheDocument();
        expect(await screen.findByText("12 of 40 read · 30 PDFs, 10 pictures")).toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Pause indexing" }));
        await waitFor(() => expect(files.filesSettingsSet).toHaveBeenLastCalledWith(expect.objectContaining({ paused: true })));

        fireEvent.click(screen.getByRole("button", { name: "Subfolders" }));
        fireEvent.click(await screen.findByRole("checkbox", { name: "Taxes" }));
        await waitFor(() =>
            expect(files.filesSettingsSet).toHaveBeenLastCalledWith(
                expect.objectContaining({ excluded_folders: ["/Users/a/Documents/Taxes"] }),
            ),
        );

        fireEvent.change(screen.getByRole("combobox", { name: "Big first passes run" }), { target: { value: "between" } });
        await waitFor(() =>
            expect(files.filesSettingsSet).toHaveBeenLastCalledWith(
                expect.objectContaining({ hours: { kind: "between", start: 1320, end: 420 } }),
            ),
        );
        fireEvent.change(await screen.findByLabelText("From"), { target: { value: "23:30" } });
        await waitFor(() =>
            expect(files.filesSettingsSet).toHaveBeenLastCalledWith(
                expect.objectContaining({ hours: { kind: "between", start: 1410, end: 420 } }),
            ),
        );

        fireEvent.click(screen.getByRole("button", { name: "Remove…" }));
        expect(screen.getByText(/Stop searching Documents\? Its 40 files leave the index\./)).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Remove" }));
        await waitFor(() => expect(files.filesSettingsSet).toHaveBeenLastCalledWith(expect.objectContaining({ folders: [] })));
    });

    it("says when a folder can't be read, and offers the way in", async () => {
        files.filesSettingsGet.mockResolvedValue({ ...off, enabled: true, folders: ["/Users/a/Documents"] });
        files.filesStatus.mockResolvedValue(status("idle", false));
        files.filesFolders.mockResolvedValue([
            { folder: "/Users/a/Documents", reach: "no_access", files: 0, done: 0, kinds: {} },
        ]);
        render(<FilesSetup mode="settings" />);
        fireEvent.click(await screen.findByRole("button", { name: "Allow in Settings…" }));
        await waitFor(() => expect(files.filesOpenPrivacySettings).toHaveBeenCalled());
        expect(screen.getByText(/No access/)).toBeInTheDocument();

        files.filesModelImport.mockResolvedValue(["embeddinggemma-2-Q8_0.gguf"]);
        fireEvent.click(screen.getByRole("button", { name: "Use files downloaded elsewhere…" }));
        expect(await screen.findByText("Using embeddinggemma-2-Q8_0.gguf.")).toBeInTheDocument();
    });

    it("words the status, the kinds and the hours", () => {
        expect(statusText({ ...status("waiting_for_hours", true), detail: "from 22:00" } as never)).toBe("Indexing from 22:00");
        expect(statusText({ ...status("held", true), detail: "waiting while Low Power Mode is on" } as never)).toBe(
            "Waiting while Low Power Mode is on",
        );
        expect(kindCounts({ image: 1, pdf: 3, audio: 0 })).toBe("3 PDFs, 1 picture");
        expect(timeText(22 * 60)).toBe("22:00");
        expect(timeText(9 * 60 + 5)).toBe("09:05");
    });

    it("says how long in words", () => {
        expect(duration(20)).toBe("under a minute");
        expect(duration(360)).toBe("about 6 min");
        expect(duration(4800)).toBe("about 1 h 20 min");
    });
});
