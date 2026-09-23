import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Timeline } from "./Timeline";
import type { MemoryCard } from "@/shared/ipc/tauri";

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

afterEach(cleanup);

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

        expect(screen.getByRole("status")).toHaveAttribute("aria-busy", "true");
        expect(screen.getByText(/searching saved memories/i)).toBeInTheDocument();
    });

    it("does not imply capture is active before the user searches", () => {
        render(
            <Timeline
                results={[]}
                isLoading={false}
                query=""
                selectedResultId={null}
                onSelectResult={vi.fn()}
            />
        );

        expect(screen.getByText(/search by topic, app, person, or time/i)).toBeInTheDocument();
        expect(screen.queryByText(/being captured/i)).not.toBeInTheDocument();
    });

    it("explains what failed to match without claiming that no memories exist", () => {
        render(
            <Timeline
                results={[]}
                isLoading={false}
                query="quarterly planning"
                selectedResultId={null}
                onSelectResult={vi.fn()}
            />
        );

        expect(screen.getByText(/no saved memories match/i)).toBeInTheDocument();
        expect(screen.getByText(/quarterly planning/i)).toBeInTheDocument();
        expect(screen.getByText(/time range or app filter/i)).toBeInTheDocument();
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
        expect(screen.getByRole("button", { name: "Selected memory: Test window" })).toHaveAttribute(
            "aria-pressed",
            "true"
        );
    });

    it("does not repeat a summary that says the same thing as its title", () => {
        const duplicate = {
            ...sample,
            title: "Alpha demo script – Google Docs",
            summary: "Alpha demo script – Google Docs",
        };
        render(
            <Timeline
                results={[duplicate]}
                isLoading={false}
                query="demo"
                selectedResultId={null}
                onSelectResult={vi.fn()}
            />
        );

        expect(screen.getAllByText(/Alpha demo script – Google Docs/)).toHaveLength(1);
        expect(screen.queryByRole("button", { name: "Delete this memory" })).not.toBeInTheDocument();
    });

    it("uses a real, named button to select a result instead of nesting controls in a button", () => {
        const onSelectResult = vi.fn();
        render(
            <Timeline
                results={[sample]}
                isLoading={false}
                query="test"
                selectedResultId={null}
                onSelectResult={onSelectResult}
            />
        );

        const article = screen.getByRole("article");
        expect(article).not.toHaveAttribute("role", "button");
        const selectButton = screen.getByRole("button", { name: "Select memory: Test window" });
        expect(selectButton).toHaveAttribute("aria-pressed", "false");
        fireEvent.click(selectButton);
        expect(onSelectResult).toHaveBeenCalledWith(expect.objectContaining({ id: sample.id }));
    });

    it("uses truthful fallbacks when source and time metadata are missing", () => {
        render(
            <Timeline
                results={[{
                    ...sample,
                    id: "sparse",
                    app_name: "",
                    timestamp: Number.NaN,
                    title: "",
                    window_title: "",
                    summary: "",
                    display_summary: "",
                    context: [],
                    raw_snippets: [],
                }]}
                isLoading={false}
                query="memory"
                selectedResultId={null}
                onSelectResult={vi.fn()}
            />
        );

        expect(screen.getByText("Unknown app")).toBeInTheDocument();
        expect(screen.getByText("Time unavailable")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Select memory: Captured memory" })).toBeInTheDocument();
    });

    it("reveals the next result page from the Load More control", () => {
        const results = Array.from({ length: 31 }, (_, index) => ({
            ...sample,
            id: `memory-${index}`,
            timestamp: sample.timestamp - index * 60_000,
            title: `Memory ${index}`,
            summary: `Unique${index} detail${index} context${index}.`,
        }));

        const { container } = render(
            <Timeline
                results={results}
                isLoading={false}
                query="search"
                selectedResultId={null}
                onSelectResult={vi.fn()}
            />
        );

        expect(container.querySelectorAll(".result-card")).toHaveLength(30);
        fireEvent.click(screen.getByRole("button", { name: "Load 1 more" }));
        expect(container.querySelectorAll(".result-card")).toHaveLength(31);
        expect(screen.getByRole("button", { name: "All 31 results shown" })).toHaveAttribute(
            "aria-disabled",
            "true"
        );
    });
});
