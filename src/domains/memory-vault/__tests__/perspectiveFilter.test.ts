import { describe, expect, it } from "vitest";
import type { MemoryCard } from "@/shared/ipc/tauri";
import { matchesPerspective } from "../perspectiveFilter";

function makeCard(overrides: Partial<MemoryCard> = {}): MemoryCard {
    return {
        id: "card-1",
        title: "Retrieval notes",
        summary: "",
        action: "",
        context: [],
        timestamp: 0,
        app_name: "Google Chrome",
        window_title: "",
        score: 1,
        source_count: 1,
        raw_snippets: [],
        ...overrides,
    };
}

describe("matchesPerspective", () => {
    it("keeps a web page whose activity label is not a web label", () => {
        const card = makeCard({ activity_type: "debugging", url: "https://docs.rs/tokio" });
        expect(matchesPerspective(card, "web")).toBe(true);
    });

    it("counts debugging and testing as coding", () => {
        expect(matchesPerspective(makeCard({ activity_type: "debugging" }), "coding")).toBe(true);
        expect(matchesPerspective(makeCard({ activity_type: "testing_workflow" }), "coding")).toBe(true);
    });

    it("falls back to text signals when the label says nothing about the perspective", () => {
        const spec = makeCard({ activity_type: "planning", window_title: "Onboarding spec draft" });
        expect(matchesPerspective(spec, "docs")).toBe(true);
        expect(matchesPerspective(spec, "meetings")).toBe(false);

        const inbox = makeCard({ activity_type: "researching", summary: "Replied to the email thread" });
        expect(matchesPerspective(inbox, "communication")).toBe(true);
    });

    it("excludes a card with no label, no url and no matching text", () => {
        expect(matchesPerspective(makeCard(), "web")).toBe(false);
        expect(matchesPerspective(makeCard(), "coding")).toBe(false);
    });
});
