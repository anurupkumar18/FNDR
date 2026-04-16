import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SearchBar } from "./SearchBar";

afterEach(() => {
    cleanup();
});

const defaultProps = {
    value: "",
    submittedValue: "",
    onChange: vi.fn(),
    onSubmit: vi.fn(),
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

describe("SearchBar", () => {
    it("renders input and forwards changes", () => {
        const onChange = vi.fn();
        render(<SearchBar {...defaultProps} onChange={onChange} />);
        const input = screen.getByRole("textbox", { name: /search memories/i });
        fireEvent.change(input, { target: { value: "oauth" } });
        expect(onChange).toHaveBeenCalledWith("oauth");
    });

    it("submits only when Enter is pressed", () => {
        const onSubmit = vi.fn();
        render(<SearchBar {...defaultProps} value="oauth flow" onSubmit={onSubmit} />);
        const input = screen.getByRole("textbox", { name: /search memories/i });
        fireEvent.keyDown(input, { key: "Enter", code: "Enter" });
        expect(onSubmit).toHaveBeenCalledTimes(1);
    });

    it("renders the voice button", () => {
        render(<SearchBar {...defaultProps} />);
        expect(screen.getByRole("button", { name: /voice recording/i })).toBeInTheDocument();
    });

    it("shows the disabled hint and disables the input", () => {
        render(
            <SearchBar
                {...defaultProps}
                disabled={true}
                disabledHint="Waiting for backend"
            />
        );

        expect(screen.getByText(/waiting for backend/i)).toBeInTheDocument();
        expect(screen.getByRole("textbox")).toBeDisabled();
    });
});
