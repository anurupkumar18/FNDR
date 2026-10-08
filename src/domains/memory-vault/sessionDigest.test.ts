import { describe, expect, it } from "vitest";
import type { MemoryCard } from "@/shared/ipc/tauri";
import { sessionDigest, sessionDigestLabel } from "./sessionDigest";

const MINUTE = 60_000;

function card(id: string, minutesAgo: number, summary: string, extra: Partial<MemoryCard> = {}): MemoryCard {
    return {
        id,
        title: "Lab 5 report",
        summary,
        action: "",
        context: [],
        timestamp: 1_800_000_000_000 - minutesAgo * MINUTE,
        app_name: "Pages",
        window_title: "Lab 5 report",
        score: 0,
        source_count: 1,
        raw_snippets: [],
        ...extra,
    } as MemoryCard;
}

const lineOf = (c: MemoryCard) => c.summary;

describe("sessionDigest", () => {
    it("is null for a row with one moment", () => {
        expect(sessionDigest({ lead: card("a", 0, "Wrote the conclusion."), similar: [] }, lineOf)).toBeNull();
    });

    it("counts moments, minutes and distinct files across the session", () => {
        const digest = sessionDigest(
            {
                lead: card("a", 0, "Wrote the conclusion.", {
                    files_touched: ["report.pages"],
                }),
                similar: [
                    card("b", 20, "Wrote the conclusion.", {
                        files_touched: ["Report.pages", "timing.csv"],
                    }),
                    card("c", 42, "Wrote the conclusion."),
                ],
            },
            lineOf,
        )!;
        expect(digest.moments).toBe(3);
        expect(digest.minutes).toBe(42);
        expect(digest.files).toBe(2);
        expect(digest.earlier).toBeUndefined();
        expect(sessionDigestLabel(digest)).toBe("3 moments over 42 min, 2 files");
    });

    it("surfaces the most detailed earlier sentence that adds something", () => {
        const digest = sessionDigest(
            {
                lead: card("a", 0, "Wrote the conclusion."),
                similar: [
                    card("b", 10, "Pasted the timing table."),
                    card("c", 25, "Measured the parallel loop with four threads and recorded a speedup of 3.1."),
                ],
            },
            lineOf,
        )!;
        expect(digest.earlier).toBe("Measured the parallel loop with four threads and recorded a speedup of 3.1.");
    });

    it("never surfaces a line written from the title alone", () => {
        const digest = sessionDigest(
            {
                lead: card("a", 0, "Wrote the conclusion."),
                similar: [
                    card("b", 10, "Viewed Lab 5 report draft for parallel loops at 03:14 PM."),
                    card("c", 25, "Captured recent activity at 02:59 PM."),
                ],
            },
            lineOf,
        )!;
        expect(digest.earlier).toBeUndefined();
    });

    it("leaves out the minutes of a session shorter than one", () => {
        const digest = sessionDigest(
            {
                lead: card("a", 0, "Wrote the conclusion."),
                similar: [card("b", 0.2, "Wrote the conclusion.")],
            },
            lineOf,
        )!;
        expect(sessionDigestLabel(digest)).toBe("2 moments");
    });
});
