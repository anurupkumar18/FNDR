import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { Timeline } from "./Timeline";
import type { MemoryCard } from "../api/tauri";

const sample: MemoryCard = {
    id: "1",
    timestamp: Date.now(),
    title: "Test window",
    summary: "Reviewed test content in Safari while validating timeline rendering behavior.",
    action: "Reviewed test content",
    context: ["Safari", "Test context"],
    app_name: "Safari",
    window_title: "Test window",
    source_count: 1,
    raw_snippets: ["snippet preview"],
    score: 0.87,
};

describe("Timeline", () => {
    it("shows loading state", () => {
        render(
            <Timeline
                results={[]}
                isLoading={true}
                query="q"
                selectedResultId={null}
                onSelectResult={() => {}}
            />
        );

        expect(screen.getByText(/searching memories/i)).toBeInTheDocument();
    });

    it("renders result meta including score in eval UI", () => {
        render(
            <Timeline
                results={[sample]}
                isLoading={false}
                query="q"
                selectedResultId={sample.id}
                onSelectResult={vi.fn()}
                evalUi={true}
            />
        );

        expect(screen.getAllByText("Safari").length).toBeGreaterThan(0);
        expect(
            screen.getByText(/reviewed test content in safari while validating timeline rendering behavior\./i)
        ).toBeInTheDocument();
        expect(screen.getByText(/score 0\.870/i)).toBeInTheDocument();
    });
});
