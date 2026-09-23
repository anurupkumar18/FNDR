import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HomeHero } from "./HomeHero";

vi.mock("@/shared/ipc/tauri", () => ({
    transcribeVoiceInput: vi.fn(),
}));

vi.mock("@/shared/motion/useReducedMotionSafe", () => ({
    useReducedMotionSafe: () => ({ reduced: true }),
}));

class MockIntersectionObserver {
    observe = vi.fn();
    disconnect = vi.fn();
}

describe("HomeHero", () => {
    beforeEach(() => {
        vi.stubGlobal("IntersectionObserver", MockIntersectionObserver);
    });

    afterEach(() => {
        cleanup();
        vi.unstubAllGlobals();
    });

    it("keeps the landing screen focused on search instead of extra CTA buttons", () => {
        render(
            <HomeHero
                userName="Anurup"
                now={new Date("2026-05-28T22:00:00")}
                greeting="Good Night, Anurup!"
                onHeroSearch={vi.fn()}
            />
        );

        expect(screen.getByRole("search")).toBeInTheDocument();
        expect(screen.getByPlaceholderText("What shall we uncover tonight?")).toBeInTheDocument();
        expect(screen.getByText("Let's dive into your memories.")).toBeInTheDocument();
        expect(screen.getByText(/search saved memories by topic, app, person, or time/i)).toBeInTheDocument();
        expect(screen.queryByText(/scroll to explore/i)).not.toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Enter the reel" })).not.toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Open work mode" })).not.toBeInTheDocument();
    });

    it("gives a useful typed-search fallback when voice capture is unavailable", () => {
        vi.stubGlobal("MediaRecorder", undefined);
        render(
            <HomeHero
                userName="Anurup"
                now={new Date("2026-05-28T22:00:00")}
                onHeroSearch={vi.fn()}
            />
        );

        fireEvent.click(screen.getByRole("button", { name: "Start voice recording" }));

        expect(screen.getByRole("status")).toHaveTextContent(
            "Microphone isn't available here. Type your search instead."
        );
        expect(screen.getByRole("textbox", { name: "Search your memories" })).toBeEnabled();
    });
});
