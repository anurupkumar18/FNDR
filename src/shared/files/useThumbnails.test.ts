import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

const files = vi.hoisted(() => ({ filesThumbnails: vi.fn() }));
vi.mock("@/shared/ipc/files", () => files);

import { useThumbnails } from "./useThumbnails";

describe("useThumbnails", () => {
    it("keeps thumbnails that arrive after the rows changed, and asks for each path once", async () => {
        let finish: (found: Record<string, string>) => void = () => undefined;
        files.filesThumbnails.mockReturnValueOnce(new Promise((resolve) => (finish = resolve)));
        files.filesThumbnails.mockResolvedValue({ "/b.png": "data:b" });
        const { result, rerender } = renderHook(({ paths }) => useThumbnails(paths), { initialProps: { paths: ["/a.png"] } });
        rerender({ paths: ["/b.png", "/a.png"] });
        await act(async () => finish({ "/a.png": "data:a" }));
        await waitFor(() => expect(result.current).toEqual({ "/a.png": "data:a", "/b.png": "data:b" }));
        expect(files.filesThumbnails).toHaveBeenCalledTimes(2);
        expect(files.filesThumbnails).toHaveBeenLastCalledWith(["/b.png"], undefined);
    });
});
