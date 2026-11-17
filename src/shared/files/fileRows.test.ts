import { describe, expect, it } from "vitest";
import type { FileHit, FileKind } from "@/shared/ipc/files";
import { fileRows } from "./fileRows";

function hit(path: string, kind: FileKind): FileHit {
    return {
        path, kind, score: 1, z: null, cosine: null, keyword_rank: null, segment: "chunk", segment_id: 1,
        loc: null, loc_end: null, excerpt: null, modified: 0, size: 0, info: null, focus: null,
    };
}

describe("fileRows", () => {
    it("groups strong matches by kind, best group first, with headings only when there are several", () => {
        const rows = fileRows([hit("/a.pdf", "pdf"), hit("/b.png", "image"), hit("/c.pdf", "pdf")], 3, false);
        expect(rows.map((row) => (row.type === "hit" ? row.hit.path : row.type))).toEqual(["/a.pdf", "/c.pdf", "/b.png"]);
        expect(rows.map((row) => row.heading)).toEqual(["PDF", undefined, "Picture"]);
        expect(fileRows([hit("/a.pdf", "pdf")], 1, false)[0].heading).toBeUndefined();
    });

    it("shows six of a kind, then a Show all row; code gets eighteen", () => {
        const pdfs = Array.from({ length: 8 }, (_, index) => hit(`/${index}.pdf`, "pdf"));
        const rows = fileRows(pdfs, 8, false);
        expect(rows).toHaveLength(7);
        expect(rows[6]).toEqual({ type: "more", kind: "pdf", count: 8 });
        const code = Array.from({ length: 20 }, (_, index) => hit(`/${index}.ts`, "code"));
        expect(fileRows(code, 20, false).filter((row) => row.type === "hit")).toHaveLength(18);
    });

    it("folds the weaker matches behind one row until it is opened", () => {
        const hits = [hit("/a.md", "doc"), hit("/b.md", "doc"), hit("/c.md", "doc")];
        expect(fileRows(hits, 1, false).map((row) => row.type)).toEqual(["hit", "weaker"]);
        const open = fileRows(hits, 1, true);
        expect(open.map((row) => row.type)).toEqual(["hit", "weaker", "hit", "hit"]);
        expect(open[1]).toEqual({ type: "weaker", count: 2, open: true });
        expect(open[2]).toMatchObject({ weaker: true });
    });
});
