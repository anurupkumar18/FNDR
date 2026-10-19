import { describe, expect, it } from "vitest";
import type { FileHit } from "@/shared/ipc/files";
import { clock, excerptParts, fileFolder, fileName, hitPlace, queryWords } from "./fileHit";

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
