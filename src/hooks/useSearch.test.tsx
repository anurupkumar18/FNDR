import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { useSearch } from "./useSearch";
import { searchMemoryCards } from "../api/tauri";

vi.mock("../api/tauri", () => ({
    searchMemoryCards: vi.fn(),
}));

function HookHarness({ query }: { query: string }) {
    const { results, isLoading, error } = useSearch(query, null, null);

    return (
        <div>
            <span data-testid="loading">{String(isLoading)}</span>
            <span data-testid="error">{error ?? ""}</span>
            <span data-testid="count">{results.length}</span>
        </div>
    );
}

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("useSearch", () => {
    it("does not issue a second backend search after a timeout error", async () => {
        vi.mocked(searchMemoryCards).mockRejectedValueOnce(new Error("Search timed out"));

        render(<HookHarness query="quarterly planning" />);

        await waitFor(() => {
            expect(searchMemoryCards).toHaveBeenCalledTimes(1);
        });

        await waitFor(() => {
            expect(screen.getByTestId("error")).toHaveTextContent(
                "Search timed out. Try a shorter query or remove filters."
            );
        });

        expect(searchMemoryCards).toHaveBeenCalledTimes(1);
        expect(screen.getByTestId("count")).toHaveTextContent("0");
    });
});
