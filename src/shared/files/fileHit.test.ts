import { describe, expect, it } from "vitest";
import type { FileHit } from "@/shared/ipc/files";
import { clock, excerptParts, fileFolder, fileName, filesNote, hitPlace, openTitle, queryWords } from "./fileHit";
import type { FilesStatus } from "@/shared/ipc/files";

function hit(partial: Partial<FileHit>): FileHit {
    return {
        path: "/Users/a/Documents/lease.pdf",
        kind: "pdf",
        score: 3,
        z: 2.5,
        cosine: 0.5,
        keyword_rank: null,
        segment: "page",
        segment_id: 1,
        loc: 3,
        loc_end: null,
        excerpt: null,
        modified: 0,
        size: 1,
        info: null,
        focus: null,
        ...partial,
    };
}

describe("file hits", () => {
    it("say where the match is", () => {
        expect(hitPlace(hit({}))).toBe("p. 3");
        expect(hitPlace(hit({ kind: "video", segment: "frame", loc: 72.4 }))).toBe("at 1:12");
        expect(hitPlace(hit({ kind: "audio", segment: "audio", loc: 3725 }))).toBe("at 1:02:05");
        expect(hitPlace(hit({ kind: "image", segment: "image", loc: null }))).toBeNull();
        expect(hitPlace(hit({ segment: "name" }))).toBeNull();
        expect(hitPlace(hit({ kind: "code", segment: "lines", loc: 40, loc_end: 80, focus: 52 }))).toBe("line 52");
        expect(hitPlace(hit({ kind: "code", segment: "lines", loc: 40, loc_end: 80 }))).toBe("line 40");
        expect(hitPlace(hit({ kind: "code", segment: "cells", loc: 3 }))).toBe("cell 3");
    });

    it("name and place the file", () => {
        expect(fileName("/Users/a/Documents/lease.pdf")).toBe("lease.pdf");
        expect(fileFolder("/Users/a/Documents/lease.pdf", "/Users/a")).toBe("~/Documents");
        expect(clock(5)).toBe("0:05");
    });

    it("split an excerpt into marked words", () => {
        expect(excerptParts("…pets need «consent» from «the» landlord")).toEqual([
            { text: "…pets need ", marked: false },
            { text: "consent", marked: true },
            { text: " from ", marked: false },
            { text: "the", marked: true },
            { text: " landlord", marked: false },
        ]);
    });

    it("take a PDF search's words from the query", () => {
        expect(queryWords("file: the clause about pets")).toEqual(["the", "clause", "about", "pets"]);
        expect(queryWords("ইলিশ মাছ")).toEqual(["ইলিশ", "মাছ"]);
    });
});

function status(partial: Partial<FilesStatus>, done = 0, pending = 0): FilesStatus {
    return {
        state: "idle",
        detail: null,
        counts: { files: done + pending, done, pending, failed: 0, vectors: 0 },
        code_counts: { files: 0, done: 0, pending: 0, failed: 0, vectors: 0 },
        code_bytes: 0,
        model_ready: true,
        helper_found: true,
        reader_found: true,
        ...partial,
    };
}

describe("filesNote", () => {
    it("says when results may be incomplete", () => {
        expect(filesNote("words", null)).toBe("Matching words…");
        expect(filesNote("words_only", status({ model_ready: false }))).toBe(
            "Matching words only: the Files model isn’t downloaded",
        );
        expect(filesNote("meaning", status({ state: "indexing" }, 42, 58))).toBe(
            "Still indexing (42%): some files aren’t in yet",
        );
        expect(filesNote("meaning", status({ state: "paused" }, 10, 1204))).toBe("1,204 files aren’t indexed yet");
        expect(filesNote("meaning", status({}, 10, 0))).toBeNull();
    });
});

describe("openTitle", () => {
    it("names the place or the editor", () => {
        expect(openTitle(hit({}))).toBe("Open at p. 3");
        expect(openTitle(hit({ kind: "audio", segment: "audio", loc: 6 }))).toBe("Open at 0:06");
        expect(openTitle(hit({ kind: "code", segment: "lines", loc: 4 }), "Cursor")).toBe("Open in Cursor");
        expect(openTitle(hit({ kind: "doc", segment: "chunk" }))).toBe("Open");
    });
});
