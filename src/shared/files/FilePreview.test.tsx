import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const files = vi.hoisted(() => ({
    filesPreview: vi.fn(),
    filesPassage: vi.fn(),
    filesClip: vi.fn(),
    filesReveal: vi.fn(),
}));
vi.mock("@/shared/ipc/files", () => files);

import { FilePreview } from "./FileResult";
import { fileDetails, matchReason } from "./fileHit";
import type { FileHit } from "@/shared/ipc/files";

function hit(partial: Partial<FileHit>): FileHit {
    return {
        path: "/Users/a/Recordings/podcast.m4a", kind: "audio", score: 3, z: 3, cosine: 0.5, keyword_rank: null,
        segment: "audio", segment_id: 4, loc: 25, loc_end: 55, excerpt: null, modified: 1_790_000_000, size: 2_100_000,
        info: { duration: 192 }, focus: null, ...partial,
    };
}

describe("FilePreview", () => {
    beforeEach(() => {
        for (const mock of Object.values(files)) mock.mockReset();
        files.filesPreview.mockResolvedValue("data:image/jpeg;base64,AAAA");
        files.filesPassage.mockResolvedValue(null);
        files.filesClip.mockResolvedValue("data:audio/mp4;base64,AAAA");
        files.filesReveal.mockResolvedValue(undefined);
    });

    afterEach(() => cleanup());

    it("plays a recording from the moment that matched, and says why it is here", async () => {
        const { container } = render(<FilePreview hit={hit({})} onClose={() => undefined} />);
        expect(screen.getByText("0:25 – 0:55")).toBeInTheDocument();
        expect(screen.getByText("Matches what’s heard at 0:25")).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Play from 0:25" }));
        await waitFor(() => expect(files.filesClip).toHaveBeenCalledWith("/Users/a/Recordings/podcast.m4a", "audio", 25));
        await waitFor(() => expect(container.querySelector("audio")?.getAttribute("src")).toBe("data:audio/mp4;base64,AAAA"));
        fireEvent.click(screen.getByRole("button", { name: "/Users/a/Recordings" }));
        await waitFor(() => expect(files.filesReveal).toHaveBeenCalledWith("/Users/a/Recordings/podcast.m4a"));
    });

    it("shows code with line numbers and the line it opens at", async () => {
        files.filesPassage.mockResolvedValue("import time\n\ndef retry_with_backoff(call):\n    pass");
        const code = hit({
            path: "/Users/a/code/retry.py", kind: "code", segment: "lines", loc: 10, loc_end: 13, focus: 12,
            info: { lines: 40 }, size: 900,
        });
        render(<FilePreview hit={code} query="code: retry backoff" onClose={() => undefined} />);
        const lines = await screen.findByLabelText("The lines that matched");
        expect(lines).toHaveTextContent("10import time");
        const focused = lines.querySelector("[data-focus]");
        expect(focused).toHaveTextContent("12def retry_with_backoff(call):");
        expect([...(focused?.querySelectorAll("mark") ?? [])].map((mark) => mark.textContent)).toEqual(["retry", "backoff"]);
        expect(screen.getByText(/Python · 40 lines · .* · 900 bytes/)).toBeInTheDocument();
    });
});

describe("matchReason and fileDetails", () => {
    it("words the reason as DigUp does", () => {
        expect(matchReason(hit({ kind: "video", segment: "frame", loc: 6 }))).toBe("Matches the frame at 0:06");
        expect(matchReason(hit({ kind: "pdf", segment: "page", loc: 3 }))).toBe("Matches the text on page 3");
        expect(matchReason(hit({ kind: "screenshot", segment: "image", loc: null }))).toBe("Matches what’s on screen");
        expect(matchReason(hit({ z: null, keyword_rank: 2 }))).toBe("Has your words");
        expect(matchReason(hit({ z: 0.4, keyword_rank: null }))).toBe("Matches its name");
    });

    it("lists what the file is", () => {
        expect(fileDetails(hit({}))).toMatch(/^Recording · .* · 3:12 · 2\.1 MB$/);
        expect(fileDetails(hit({ kind: "pdf", segment: "page", info: { pages: 463, read_to: 120 }, size: 23_000_000 }))).toMatch(
            /^PDF · .* · 463 pages, 120 read so far · 23 MB$/,
        );
        expect(fileDetails(hit({ kind: "image", segment: "image", info: { width: 1440, height: 900 }, size: 0 }))).toMatch(
            /^Picture · .* · 1440 × 900$/,
        );
    });
});
