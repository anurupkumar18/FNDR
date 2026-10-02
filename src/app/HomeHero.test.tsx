import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HomeHero } from "./HomeHero";

const voiceMocks = vi.hoisted(() => ({
    start: vi.fn(),
    stop: vi.fn(),
    cancel: vi.fn(),
    retry: vi.fn(),
    options: null as null | {
        surface: string;
        mode: string;
        onPartial?: (text: string) => void;
        onFinal?: (text: string) => void;
    },
    state: { kind: "idle" } as
        | { kind: "idle" }
        | { kind: "listening"; level: number }
        | { kind: "unavailable"; reason: "private_context"; message: string },
    level: 0,
    isActive: false,
}));

vi.mock("@/shared/voice/useVoice", () => ({
    useVoice: vi.fn((options) => {
        voiceMocks.options = options;
        return {
            state: voiceMocks.state,
            level: voiceMocks.level,
            sessionId: voiceMocks.isActive ? "home-session" : null,
            surface: "home_search",
            mode: "toggle",
            isActive: voiceMocks.isActive,
            start: voiceMocks.start,
            stop: voiceMocks.stop,
            cancel: voiceMocks.cancel,
            retry: voiceMocks.retry,
        };
    }),
}));

vi.mock("@/shared/motion/useReducedMotionSafe", () => ({
    useReducedMotionSafe: () => ({ reduced: true }),
}));

class MockIntersectionObserver {
    observe = vi.fn();
    disconnect = vi.fn();
}

function renderHero(onHeroSearch = vi.fn()) {
    return {
        onHeroSearch,
        ...render(
            <HomeHero
                userName="Anurup"
                now={new Date("2026-05-28T22:00:00")}
                greeting="Good Night, Anurup!"
                onHeroSearch={onHeroSearch}
            />,
        ),
    };
}

describe("HomeHero", () => {
    beforeEach(() => {
        vi.stubGlobal("IntersectionObserver", MockIntersectionObserver);
        voiceMocks.start.mockReset();
        voiceMocks.stop.mockReset();
        voiceMocks.cancel.mockReset();
        voiceMocks.retry.mockReset();
        voiceMocks.options = null;
        voiceMocks.state = { kind: "idle" };
        voiceMocks.level = 0;
        voiceMocks.isActive = false;
    });

    afterEach(() => {
        cleanup();
        vi.unstubAllGlobals();
    });

    it("keeps the landing screen focused on search instead of extra CTA buttons", () => {
        renderHero();

        expect(screen.getByRole("search")).toBeInTheDocument();
        expect(screen.getByPlaceholderText("What shall we uncover tonight?")).toBeInTheDocument();
        expect(screen.getByText("Let's dive into your memories.")).toBeInTheDocument();
        expect(screen.getByText(/search saved memories by topic, app, person, or time/i)).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Enter the reel" })).not.toBeInTheDocument();
    });

    it("uses the shared home-search toggle voice session", () => {
        const { rerender } = renderHero();
        expect(voiceMocks.options).toMatchObject({ surface: "home_search", mode: "toggle" });

        fireEvent.click(screen.getByRole("button", { name: "Start voice input" }));
        expect(voiceMocks.start).toHaveBeenCalledOnce();

        voiceMocks.state = { kind: "listening", level: 0.4 };
        voiceMocks.level = 0.4;
        voiceMocks.isActive = true;
        rerender(
            <HomeHero
                userName="Anurup"
                now={new Date("2026-05-28T22:00:00")}
                onHeroSearch={vi.fn()}
            />,
        );
        fireEvent.click(screen.getByRole("button", { name: "Stop voice input" }));
        expect(voiceMocks.stop).toHaveBeenCalledOnce();
    });

    it("drafts partial speech without submitting a search", () => {
        const { onHeroSearch } = renderHero();

        act(() => voiceMocks.options?.onPartial?.("show my"));

        expect(screen.getByRole("textbox", { name: "Search your memories" })).toHaveValue("show my");
        expect(onHeroSearch).not.toHaveBeenCalled();
    });

    it("keeps final speech reviewable until Enter confirms it", () => {
        const { onHeroSearch } = renderHero();

        act(() => voiceMocks.options?.onFinal?.("show my meetings"));

        const input = screen.getByRole("textbox", { name: "Search your memories" });
        expect(input).toHaveValue("show my meetings");
        expect(onHeroSearch).not.toHaveBeenCalled();

        fireEvent.keyDown(input, { key: "Enter" });
        expect(onHeroSearch).toHaveBeenCalledWith("show my meetings");
    });

    it("keeps typed search available when shared voice is unavailable", () => {
        voiceMocks.state = {
            kind: "unavailable",
            reason: "private_context",
            message: "Voice isn't available in Private Mode. Type your search instead.",
        };
        renderHero();

        expect(screen.getByRole("alert")).toHaveTextContent("Voice isn't available in Private Mode");
        expect(screen.getByRole("textbox", { name: "Search your memories" })).toBeEnabled();
    });
});
