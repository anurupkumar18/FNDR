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
            />
        );
        expect(screen.getByText(/searching memories/i)).toBeInTheDocument();
    });

    it("renders result app name", () => {
        render(
            <Timeline
                results={[sample]}
                isLoading={false}
                query="q"
            />
        );
        expect(screen.getByText("Safari")).toBeInTheDocument();
        expect(screen.getByText("Test window")).toBeInTheDocument();
    });
});
