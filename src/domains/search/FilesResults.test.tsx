import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const fileMocks = vi.hoisted(() => ({
    filesSearch: vi.fn(),
    filesPreview: vi.fn(),
    filesOpen: vi.fn(),
    filesOpenCode: vi.fn(),
    filesReveal: vi.fn(),
    filesCopy: vi.fn(),
    filesThumbnails: vi.fn(),
    filesQuickLook: vi.fn(),
}));
vi.mock("@/shared/ipc/files", () => fileMocks);

import { FilesResults } from "./FilesResults";

function hit(path: string, kind: string, segment: string, loc: number | null = null) {
    return {
        path, kind, segment, loc, score: 3, z: 3, cosine: 0.5, keyword_rank: null, segment_id: path.length,
        loc_end: null, excerpt: null, modified: 0, size: 1, info: null, focus: null,
    };
}

describe("FilesResults", () => {
    beforeEach(() => {
        for (const mock of Object.values(fileMocks)) mock.mockReset();
        fileMocks.filesSearch.mockResolvedValue({
            hits: [hit("/p/dog_on_beach.jpg", "image", "image"), hit("/d/lease.pdf", "pdf", "page", 3)],
            strong: 2,
            meaning: true,
        });
        fileMocks.filesPreview.mockResolvedValue("data:image/jpeg;base64,AAAA");
        fileMocks.filesOpen.mockResolvedValue("default");
        fileMocks.filesReveal.mockResolvedValue(undefined);
        fileMocks.filesCopy.mockResolvedValue(undefined);
        fileMocks.filesThumbnails.mockResolvedValue({ "/p/dog_on_beach.jpg": "data:image/png;base64,AAAA" });
        fileMocks.filesQuickLook.mockImplementation((path?: string) => Promise.resolve(Boolean(path)));
    });

    afterEach(() => cleanup());

    it("shows pictures as a grid and other files as a list, and acts on them", async () => {
        render(<FilesResults query="dog lease" />);
        expect(await screen.findByRole("img", { name: "dog_on_beach.jpg" })).toBeInTheDocument();
        const pdf = screen.getByRole("option", { name: /lease\.pdf/ });
        expect(pdf).toHaveTextContent("p. 3");

        fireEvent.click(pdf);
        await waitFor(() => expect(fileMocks.filesOpen).toHaveBeenCalledWith("/d/lease.pdf", 3, undefined, ["dog", "lease"]));
        fireEvent.click(screen.getByRole("button", { name: "Show in Finder" }));
        await waitFor(() => expect(fileMocks.filesReveal).toHaveBeenCalled());
    });

    it("filters by kind", async () => {
        render(<FilesResults query="lease" />);
        await screen.findByRole("option", { name: /lease\.pdf/ });
        expect(screen.getByRole("tab", { name: "PDFs 1" })).toBeInTheDocument();
        expect(screen.queryByRole("tab", { name: /Videos/ })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole("tab", { name: "PDFs 1" }));
        await waitFor(() => expect(fileMocks.filesSearch).toHaveBeenLastCalledWith("lease", ["pdf"], 40));
        fireEvent.keyDown(screen.getByRole("region", { name: "Files" }), { key: "8", metaKey: true });
        await waitFor(() => expect(fileMocks.filesSearch).toHaveBeenLastCalledWith("code: lease", undefined, 40));
    });

    it("adds nothing to a search that found no files", async () => {
        fileMocks.filesSearch.mockResolvedValue({ hits: [], strong: 0, meaning: true });
        const { container } = render(<FilesResults query="zebra" />);
        await waitFor(() => expect(fileMocks.filesSearch).toHaveBeenCalled());
        expect(container).toBeEmptyDOMElement();
    });

    it("moves with the arrows and acts with the keys", async () => {
        render(<FilesResults query="dog lease" />);
        const region = await screen.findByRole("region", { name: "Files" });
        await screen.findByRole("option", { name: /lease\.pdf/ });
        const tile = screen.getByRole("option", { name: "dog_on_beach.jpg" });
        expect(tile).toHaveAttribute("aria-selected", "true");
        fireEvent.keyDown(region, { key: "ArrowDown" });
        await waitFor(() => expect(screen.getByRole("option", { name: /lease\.pdf/ })).toHaveAttribute("aria-selected", "true"));
        fireEvent.keyDown(region, { key: " " });
        await waitFor(() => expect(fileMocks.filesQuickLook).toHaveBeenCalledWith("/d/lease.pdf"));
        fireEvent.keyDown(region, { key: "Enter", metaKey: true });
        await waitFor(() => expect(fileMocks.filesReveal).toHaveBeenCalledWith("/d/lease.pdf"));
        fireEvent.keyDown(region, { key: "c", metaKey: true });
        await waitFor(() => expect(fileMocks.filesCopy).toHaveBeenCalledWith("/d/lease.pdf"));
    });

    it("takes the filter Quick Find handed over", async () => {
        render(<FilesResults query="lease" opened={{ query: "lease", kinds: ["pdf"] }} />);
        await waitFor(() => expect(fileMocks.filesSearch).toHaveBeenCalledWith("lease", ["pdf"], 40));
    });
});
