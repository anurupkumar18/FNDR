import { afterEach, describe, expect, it } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { parseHomeSections, setHomeSection, useHomeSections } from "./homeSections";

afterEach(() => localStorage.clear());

describe("home sections", () => {
    it("shows every section until the person turns one off", () => {
        expect(parseHomeSections(null)).toEqual({ routine: true, deadlines: true, threads: true, sets: true });
        expect(parseHomeSections("not json")).toEqual({ routine: true, deadlines: true, threads: true, sets: true });
        expect(parseHomeSections('{"sets":false}')).toEqual({ routine: true, deadlines: true, threads: true, sets: false });
    });

    it("reads a choice live and keeps it across a reload", () => {
        const { result } = renderHook(() => useHomeSections());
        expect(result.current.deadlines).toBe(true);

        act(() => setHomeSection("deadlines", false));
        expect(result.current.deadlines).toBe(false);
        expect(JSON.parse(localStorage.getItem("fndr-home-sections") ?? "{}")).toMatchObject({ deadlines: false });

        const again = renderHook(() => useHomeSections());
        expect(again.result.current).toMatchObject({ deadlines: false, threads: true });
    });
});
