import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { SearchBar } from "./SearchBar";

afterEach(() => {
    cleanup();
});

describe("SearchBar", () => {
    it("renders input and forwards changes", () => {
        const onChange = vi.fn();
        render(
            <SearchBar
                value=""
                onChange={onChange}
                timeFilter={null}
                onTimeFilterChange={() => {}}
                appFilter={null}
                onAppFilterChange={() => {}}
                appNames={["Safari"]}
                resultCount={0}
                searchResults={[]}
            />
        );
        const input = screen.getByPlaceholderText(/what do you remember/i);
        fireEvent.change(input, { target: { value: "oauth" } });
        expect(onChange).toHaveBeenCalledWith("oauth");
    });

    it("shows disabled hint when disabled", () => {
        render(
            <SearchBar
                value=""
                onChange={() => {}}
                timeFilter={null}
                onTimeFilterChange={() => {}}
                appFilter={null}
                onAppFilterChange={() => {}}
                appNames={[]}
                resultCount={0}
                searchResults={[]}
                disabled
                disabledHint="Waiting for backend"
            />
        );
        expect(screen.getByText(/waiting for backend/i)).toBeInTheDocument();
        expect(screen.getByRole("textbox")).toBeDisabled();
    });
});
