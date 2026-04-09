import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { Timeline } from "./Timeline";
import type { SearchResult } from "../api/tauri";

const sample: SearchResult = {
    id: "1",
    timestamp: Date.now(),
    app_name: "Safari",
    window_title: "Test window",
    session_id: "s",
    text: "full text",
    snippet: "snippet preview",
    score: 0.87,
};

describe("Timeline", () => {
    it("shows loading state", () => {
        render(
            <Timeline
                results={[]}
                isLoading
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
                onSelectResult={() => {}}
                evalUi
            />
        );
        expect(screen.getByText("Safari")).toBeInTheDocument();
        expect(screen.getByText("Test window")).toBeInTheDocument();
        expect(screen.getByText(/score 0\.870/)).toBeInTheDocument();
    });
});
