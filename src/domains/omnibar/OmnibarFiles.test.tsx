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
}));

vi.mock("@/shared/hooks/useTauriEvent", () => ({ useTauriEvent: () => undefined }));
vi.mock("@/shared/ipc/tauri", () => ({ ...ipcMocks, OMNIBAR_FOCUS_EVENT: "omnibar://focus" }));
vi.mock("@/shared/ipc/files", () => fileMocks);

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
});
