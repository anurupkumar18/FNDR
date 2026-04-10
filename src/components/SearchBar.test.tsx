import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { SearchBar } from "./SearchBar";

afterEach(() => {
    cleanup();
});

describe("SearchBar", () => {
    const defaultProps = {
        value: "",
        onChange: vi.fn(),
        timeFilter: null,
        onTimeFilterChange: () => {},
        appFilter: null,
        onAppFilterChange: () => {},
        onSetMeetingPanelOpen: () => {},
        onSetGraphPanelOpen: () => {},
        appNames: ["Safari"],
        resultCount: 0,
        searchResults: [],
    };

    it("renders input and forwards changes", () => {
        const onChange = vi.fn();
        render(<SearchBar {...defaultProps} onChange={onChange} />);
        const input = screen.getByPlaceholderText(/search your memories/i);
        fireEvent.change(input, { target: { value: "oauth" } });
        expect(onChange).toHaveBeenCalledWith("oauth");
    });

    it("renders mic button", () => {
        render(<SearchBar {...defaultProps} />);
        const micBtn = screen.getByRole("button", { name: /voice recording/i });
        expect(micBtn).toBeDefined();
    });
});
