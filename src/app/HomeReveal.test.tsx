import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import { HomeReveal } from "./HomeReveal";

afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
});

function stubObserver() {
    const watchers: Array<(entries: Array<{ isIntersecting: boolean }>) => void> = [];
    vi.stubGlobal(
        "IntersectionObserver",
        class {
            constructor(callback: (entries: Array<{ isIntersecting: boolean }>) => void) {
                watchers.push(callback);
            }
            observe() {}
            disconnect() {}
        },
    );
    vi.stubGlobal("matchMedia", () => ({ matches: false }));
    return watchers;
}

describe("HomeReveal", () => {
    it("waits blurred below the fold and sharpens once it scrolls into view", () => {
        const watchers = stubObserver();
        render(<HomeReveal><p>Due soon</p></HomeReveal>);
        const wrapper = screen.getByText("Due soon").parentElement as HTMLElement;
        expect(wrapper).not.toHaveClass("is-revealed");

        act(() => watchers[0]([{ isIntersecting: false }]));
        expect(wrapper).not.toHaveClass("is-revealed");
        act(() => watchers[0]([{ isIntersecting: true }]));
        expect(wrapper).toHaveClass("is-revealed");
    });

    it("is simply there with reduced motion", () => {
        stubObserver();
        vi.stubGlobal("matchMedia", () => ({ matches: true }));
        render(<HomeReveal><p>Due soon</p></HomeReveal>);
        expect(screen.getByText("Due soon").parentElement).toHaveClass("is-revealed");
    });

    it("is simply there where scroll observation does not exist", () => {
        render(<HomeReveal><p>Due soon</p></HomeReveal>);
        expect(screen.getByText("Due soon").parentElement).toHaveClass("is-revealed");
    });
});
