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
    reduced: true,
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
    useReducedMotionSafe: () => ({ reduced: voiceMocks.reduced }),
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
        voiceMocks.reduced = true;
    });

    afterEach(() => {
        cleanup();
        vi.unstubAllGlobals();
    });

    it("keeps the landing screen focused on search instead of extra CTA buttons", () => {
        renderHero();

        expect(screen.getByRole("search")).toBeInTheDocument();
        expect(screen.getByPlaceholderText("Search memories")).toBeInTheDocument();
        expect(screen.getByText("Let's dive into your memories.")).toBeInTheDocument();
        expect(screen.getByText(/search saved memories by topic, app, person, or time/i)).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Enter the reel" })).not.toBeInTheDocument();
    });

    it("shows one submit action only when a query can be sent", () => {
        const { onHeroSearch } = renderHero();
        expect(screen.queryByRole("button", { name: "Submit search" })).not.toBeInTheDocument();

        fireEvent.change(screen.getByRole("textbox", { name: "Search your memories" }), { target: { value: "beta notes" } });
        fireEvent.click(screen.getByRole("button", { name: "Submit search" }));
        expect(onHeroSearch).toHaveBeenCalledWith("beta notes");
    });

    it("melts Submit out of Speak on one liquid surface when motion is allowed", () => {
        voiceMocks.reduced = false;
        const { onHeroSearch, container } = renderHero();
        const input = screen.getByRole("textbox", { name: "Search your memories" });
        expect(container.querySelector('[data-gooey="on"]')).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Submit search" })).not.toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Start voice input" })).toBeEnabled();

        fireEvent.change(input, { target: { value: "beta notes" } });
        const submit = screen.getByRole("button", { name: "Submit search" });
        expect(submit).toBeEnabled();
        fireEvent.click(submit);
        expect(onHeroSearch).toHaveBeenCalledWith("beta notes");

        fireEvent.click(screen.getByRole("button", { name: "Start voice input" }));
        expect(voiceMocks.start).toHaveBeenCalledOnce();
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

    it.each([
        ["2026-10-09T16:30:00", "Good Evening, Anurup! Let's get productive.", "Good Evening, Anurup!"],
        ["2026-10-09T16:30:00", "Good Afternoon, Anurup! Let's get productive.", "Good Evening, Anurup!"],
        ["2026-10-09T13:00:00", "Good Afternoon, Anurup! Ready?", "Good Afternoon, Anurup!"],
        ["2026-10-09T09:00:00", "", "Good Morning, Anurup!"],
        ["2026-10-09T02:00:00", "", "Good Night, Anurup!"],
    ])("at %s the greeting matches the current time of day while search stays plainly labeled", (at, greeting, title) => {
        render(<HomeHero userName="Anurup" now={new Date(at)} greeting={greeting} onHeroSearch={vi.fn()} />);
        expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(title);
        expect(screen.getByPlaceholderText("Search memories")).toBeInTheDocument();
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
