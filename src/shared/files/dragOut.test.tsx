import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

const files = vi.hoisted(() => ({ filesDrag: vi.fn(() => Promise.resolve()), filesPreview: vi.fn(), filesPassage: vi.fn(), filesClip: vi.fn(), filesReveal: vi.fn() }));
vi.mock("@/shared/ipc/files", () => files);

import { FileResult } from "./FileResult";
import type { FileHit } from "@/shared/ipc/files";

const hit: FileHit = {
    path: "/Users/a/Pictures/dog.png", kind: "image", score: 3, z: 3, cosine: 0.5, keyword_rank: null, segment: "image",
    segment_id: 1, loc: null, loc_end: null, excerpt: null, modified: 0, size: 1, info: null, focus: null,
};

describe("dragging a row out", () => {
    afterEach(() => {
        cleanup();
        files.filesDrag.mockClear();
    });

    it("drags the file once the pointer moves, and does not also open it", () => {
        const onOpen = vi.fn();
        render(
            <FileResult hit={hit} id="r" selected={false} thumbnail="data:image/png;base64,AAAA" onSelect={() => undefined} onOpen={onOpen} />,
        );
        const row = screen.getByRole("option");
        fireEvent.mouseDown(row, { button: 0, clientX: 10, clientY: 10 });
        fireEvent.mouseMove(row, { buttons: 1, clientX: 11, clientY: 11 });
        expect(files.filesDrag).not.toHaveBeenCalled();
        fireEvent.mouseMove(row, { buttons: 1, clientX: 30, clientY: 10 });
        expect(files.filesDrag).toHaveBeenCalledWith("/Users/a/Pictures/dog.png", "data:image/png;base64,AAAA");
        fireEvent.click(row);
        expect(onOpen).not.toHaveBeenCalled();
        fireEvent.click(row);
        expect(onOpen).toHaveBeenCalledTimes(1);
    });
});
