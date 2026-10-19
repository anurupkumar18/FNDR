import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const ipcMocks = vi.hoisted(() => ({
    copyClipboardEntry: vi.fn(),
    dismissOmnibar: vi.fn(),
    fndrAnswer: vi.fn(),
    getClipboardHistory: vi.fn(),
    omnibarOpenMemory: vi.fn(),
    notchDoRequest: vi.fn(),
    notchDoSkills: vi.fn(),
    pasteClipboardEntry: vi.fn(),
    searchMemoryCards: vi.fn(),
}));

const fileMocks = vi.hoisted(() => ({
    filesSearch: vi.fn(),
    filesOpen: vi.fn(),
    filesReveal: vi.fn(),
    filesCopy: vi.fn(),
    filesPreview: vi.fn(),
    filesStatus: vi.fn(),
    filesSettingsGet: vi.fn(),
    filesThumbnails: vi.fn(),
    filesQuickLook: vi.fn(),
    filesOpenInWindow: vi.fn(),
    filesEditors: vi.fn(),
}));

vi.mock("@/shared/hooks/useTauriEvent", () => ({ useTauriEvent: () => undefined }));
vi.mock("@/shared/ipc/tauri", () => ({ ...ipcMocks, OMNIBAR_FOCUS_EVENT: "omnibar://focus" }));
vi.mock("@/shared/ipc/files", () => ({ ...fileMocks, FILES_STATUS_EVENT: "files://status" }));

import { OmnibarApp } from "./OmnibarApp";

const leasePage = {
    path: "/Users/a/Documents/lease.pdf",
    kind: "pdf",
    score: 4,
    z: 3,
    cosine: 0.6,
    keyword_rank: 1,
    segment: "page",
    segment_id: 7,
    loc: 3,
    loc_end: null,
    excerpt: "…other «pets» need written consent…",
    modified: 0,
    size: 1,
    info: null,
    focus: null,
};

const memory = {
    id: "m1",
    title: "Lease email",
    summary: "The landlord replied about pets.",
    action: "",
    context: [],
    timestamp: Date.now(),
    app_name: "Mail",
    window_title: "Inbox",
    score: 0.9,
    source_count: 1,
    raw_snippets: [],
};

describe("Quick Find with Files search", () => {
    beforeEach(() => {
        for (const mock of [...Object.values(ipcMocks), ...Object.values(fileMocks)]) mock.mockReset();
        ipcMocks.searchMemoryCards.mockResolvedValue([memory]);
        ipcMocks.notchDoSkills.mockResolvedValue([]);
        ipcMocks.dismissOmnibar.mockResolvedValue(undefined);
        fileMocks.filesSearch.mockResolvedValue({ hits: [leasePage], strong: 1, meaning: true });
        fileMocks.filesOpen.mockResolvedValue("searching pets");
        fileMocks.filesReveal.mockResolvedValue(undefined);
        fileMocks.filesCopy.mockResolvedValue(undefined);
        fileMocks.filesPreview.mockResolvedValue("data:image/jpeg;base64,AAAA");
        fileMocks.filesStatus.mockResolvedValue({
            state: "idle",
            detail: null,
            counts: { files: 1, done: 1, pending: 0, failed: 0, vectors: 1 },
            model_ready: true,
            helper_found: true,
            reader_found: true,
        });
        fileMocks.filesSettingsGet.mockResolvedValue({ enabled: true, folders: ["/Users/a"], code_folders: [] });
        fileMocks.filesThumbnails.mockResolvedValue({ "/Users/a/Documents/lease.pdf": "data:image/png;base64,AAAA" });
        fileMocks.filesQuickLook.mockImplementation((path?: string) => Promise.resolve(Boolean(path)));
        fileMocks.filesOpenInWindow.mockResolvedValue(undefined);
        fileMocks.filesEditors.mockResolvedValue([]);
    });

    afterEach(() => cleanup());

    it("lists files after memories and opens one at its page", async () => {
        render(<OmnibarApp />);
        const box = screen.getByRole("searchbox", { name: "Search your memory" });
        fireEvent.change(box, { target: { value: "pets lease" } });

        await screen.findByText("Lease email");
        const file = await screen.findByRole("option", { name: /lease\.pdf/ });
        expect(file).toHaveTextContent("p. 3");
        expect(file.querySelector("mark")?.textContent).toBe("pets");
        expect(screen.getByText("Files")).toBeInTheDocument();

        fireEvent.keyDown(box, { key: "ArrowDown" });
        await waitFor(() => expect(file).toHaveAttribute("aria-selected", "true"));
        expect(screen.getByText("⇧↵ Finder")).toBeInTheDocument();
        expect(screen.getByText("↵ open at p. 3")).toBeInTheDocument();
        expect(file.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,AAAA");
        fireEvent.keyDown(box, { key: "Enter" });
        await waitFor(() =>
            expect(fileMocks.filesOpen).toHaveBeenCalledWith("/Users/a/Documents/lease.pdf", 3, undefined, ["pets", "lease"]),
        );
        await waitFor(() => expect(ipcMocks.dismissOmnibar).toHaveBeenCalled());
        expect(ipcMocks.omnibarOpenMemory).not.toHaveBeenCalled();
    });

    it("searches files only after file:, and reveals, copies and previews the selected file", async () => {
        render(<OmnibarApp />);
        const box = screen.getByRole("searchbox", { name: "Search your memory" });
        fireEvent.change(box, { target: { value: "file: pets" } });
        const file = await screen.findByRole("option", { name: /lease\.pdf/ });
        expect(ipcMocks.searchMemoryCards).not.toHaveBeenCalled();
        await waitFor(() => expect(file).toHaveAttribute("aria-selected", "true"));

        fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
        await waitFor(() => expect(fileMocks.filesReveal).toHaveBeenCalledWith("/Users/a/Documents/lease.pdf"));

        fireEvent.keyDown(box, { key: "c", metaKey: true });
        await waitFor(() => expect(fileMocks.filesCopy).toHaveBeenCalledWith("/Users/a/Documents/lease.pdf"));
        expect(await screen.findByText("Copied the file. Paste it anywhere.")).toBeInTheDocument();

        fireEvent.keyDown(box, { key: "y", metaKey: true });
        expect(await screen.findByRole("region", { name: "Preview of lease.pdf" })).toBeInTheDocument();
        await waitFor(() => expect(fileMocks.filesPreview).toHaveBeenCalledWith("/Users/a/Documents/lease.pdf", "pdf", 3, undefined));
    });

    it("keeps memory search working when Files search fails", async () => {
        fileMocks.filesSearch.mockRejectedValue("Files search is not running");
        render(<OmnibarApp />);
        fireEvent.change(screen.getByRole("searchbox", { name: "Search your memory" }), { target: { value: "pets" } });
        await screen.findByText("Lease email");
        await waitFor(() => expect(fileMocks.filesSearch).toHaveBeenCalled());
        expect(screen.queryByText("Files")).not.toBeInTheDocument();
        expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });

    it("types words at once, then the full search on a pause", async () => {
        render(<OmnibarApp />);
        fireEvent.change(screen.getByRole("searchbox", { name: "Search your memory" }), { target: { value: "file: pets" } });
        await waitFor(() => expect(fileMocks.filesSearch).toHaveBeenCalledWith("file: pets", undefined, 40, true));
        await waitFor(() => expect(fileMocks.filesSearch).toHaveBeenCalledWith("file: pets", undefined, 40));
    });

    it("folds weaker matches, previews with Space once arrowed in, and closes the preview before Quick Find", async () => {
        const weaker = { ...leasePage, path: "/Users/a/Documents/old-lease.pdf", segment_id: 8 };
        fileMocks.filesSearch.mockResolvedValue({ hits: [leasePage, weaker], strong: 1, meaning: true });
        render(<OmnibarApp />);
        const box = screen.getByRole("searchbox", { name: "Search your memory" });
        fireEvent.change(box, { target: { value: "file: pets" } });
        const fold = await screen.findByRole("option", { name: "Show 1 weaker match" });
        expect(screen.queryByRole("option", { name: /old-lease\.pdf/ })).not.toBeInTheDocument();
        expect(await screen.findByText("1 result · 1 weaker")).toBeInTheDocument();

        fireEvent.keyDown(box, { key: "ArrowDown" });
        await waitFor(() => expect(fold).toHaveAttribute("aria-selected", "true"));
        fireEvent.keyDown(box, { key: "Enter" });
        expect(await screen.findByRole("option", { name: /old-lease\.pdf/ })).toBeInTheDocument();
        expect(fold).toHaveAttribute("aria-expanded", "true");

        fireEvent.keyDown(box, { key: "ArrowUp" });
        fireEvent.keyDown(box, { key: " " });
        await waitFor(() => expect(fileMocks.filesQuickLook).toHaveBeenCalledWith("/Users/a/Documents/lease.pdf"));
        fireEvent.keyDown(box, { key: "Escape" });
        await waitFor(() => expect(fileMocks.filesQuickLook).toHaveBeenLastCalledWith(undefined));
        expect(ipcMocks.dismissOmnibar).not.toHaveBeenCalled();

        fireEvent.keyDown(box, { key: "o", metaKey: true });
        await waitFor(() => expect(fileMocks.filesOpenInWindow).toHaveBeenCalledWith("file: pets", undefined));
    });

    it("says why a files-only search found nothing", async () => {
        fileMocks.filesSearch.mockResolvedValue({ hits: [], strong: 0, meaning: true });
        fileMocks.filesSettingsGet.mockResolvedValue({ enabled: true, folders: [], code_folders: [] });
        render(<OmnibarApp />);
        fireEvent.change(screen.getByRole("searchbox", { name: "Search your memory" }), { target: { value: "file: pets" } });
        expect(await screen.findByText(/No folders chosen yet/)).toBeInTheDocument();
        fireEvent.change(screen.getByRole("searchbox", { name: "Search your memory" }), { target: { value: "code: retry" } });
        expect(await screen.findByText(/Code search is off/)).toBeInTheDocument();
    });
});
