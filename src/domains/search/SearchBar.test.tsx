import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { SearchBar } from "./SearchBar";

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    vi.unstubAllGlobals();
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
    onSetMemoryCardsPanelOpen: () => {},
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

    it("keeps typed search available when microphone capture is unavailable", () => {
        vi.stubGlobal("MediaRecorder", undefined);
        render(<SearchBar {...defaultProps} />);

        fireEvent.click(screen.getByRole("button", { name: "Start voice recording" }));

        expect(screen.getByRole("status")).toHaveTextContent(
            "Microphone isn't available here. Type your search instead."
        );
        expect(screen.getByRole("textbox", { name: "Search memories" })).toBeEnabled();
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

    it("leaves Cmd+K to the command palette and only clears a focused search", () => {
        const onChange = vi.fn();
        const onSubmit = vi.fn();
        render(<SearchBar {...defaultProps} value="oauth" onChange={onChange} onSubmit={onSubmit} />);
        const input = screen.getByRole("textbox", { name: /search memories/i });

        fireEvent.keyDown(window, { key: "k", metaKey: true });
        expect(input).not.toHaveFocus();
        expect(onChange).not.toHaveBeenCalled();

        fireEvent.keyDown(window, { key: "Escape" });
        expect(onChange).not.toHaveBeenCalled();

        input.focus();
        fireEvent.keyDown(window, { key: "Escape" });
        expect(onChange).toHaveBeenCalledWith("");
        expect(onSubmit).toHaveBeenCalledWith("");
    });

    it("labels both filters and announces a grammatically correct result count", () => {
        render(
            <SearchBar
                {...defaultProps}
                submittedValue="oauth"
                resultCount={1}
                timeFilter="24h"
                appFilter="Safari"
            />
        );

        expect(screen.getByRole("combobox", { name: "Time range" })).toHaveValue("24h");
        expect(screen.getByRole("combobox", { name: "App" })).toHaveValue("Safari");
        expect(screen.getByRole("status", { name: "Search result count" })).toHaveTextContent(
            "1 result"
        );
    });

    it("announces that edited text has not been searched yet", () => {
        render(
            <SearchBar
                {...defaultProps}
                value="oauth refresh"
                submittedValue="oauth"
            />
        );

        expect(screen.getByText("Press Enter to search")).toHaveAttribute("role", "status");
    });
});
