import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { useSearch } from "./useSearch";
import { searchMemoryCards } from "@/shared/ipc/tauri";

vi.mock("@/shared/ipc/tauri", () => ({
    searchMemoryCards: vi.fn(),
}));

function HookHarness({ query }: { query: string }) {
    const { results, isLoading, error, activityTrace } = useSearch(query, null, null);
    const currentStep = activityTrace?.steps[activityTrace.steps.length - 1];

    return (
        <div>
            <span data-testid="loading">{String(isLoading)}</span>
            <span data-testid="error">{error ?? ""}</span>
            <span data-testid="count">{results.length}</span>
            <span data-testid="activity-label">{currentStep?.label ?? ""}</span>
            <span data-testid="activity-actor">{currentStep?.actor ?? ""}</span>
            <span data-testid="activity-detail">{currentStep?.detail ?? ""}</span>
            <span data-testid="activity-evidence">{currentStep?.evidence ?? ""}</span>
            <span data-testid="activity-steps">
                {activityTrace?.steps.map((step) => `${step.id}:${step.status}`).join("|") ?? ""}
            </span>
        </div>
    );
}

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("useSearch", () => {
    it("traces the real request boundary and verified retrieval result", async () => {
        let resolveSearch!: (value: Awaited<ReturnType<typeof searchMemoryCards>>) => void;
        vi.mocked(searchMemoryCards).mockImplementation(
            () => new Promise((resolve) => {
                resolveSearch = resolve;
            }),
        );

        render(<HookHarness query="quarterly planning" />);

        await waitFor(() => expect(searchMemoryCards).toHaveBeenCalledTimes(1));
        expect(screen.getByTestId("activity-label")).toHaveTextContent("Requesting memory search");
        expect(screen.getByTestId("activity-actor")).toHaveTextContent("FNDR search service");
        expect(screen.getByTestId("activity-steps")).toHaveTextContent("queued:completed");

        resolveSearch([{
            id: "memory-1",
            title: "Quarterly plan",
            summary: "Summary",
            action: "",
            context: [],
            timestamp: 1,
            app_name: "Notes",
            window_title: "",
            score: 0.9,
            source_count: 1,
            raw_snippets: [],
            matched_routes: ["Vector", "Keyword"],
        }]);

        await waitFor(() => {
            expect(screen.getByTestId("activity-label")).toHaveTextContent("Search completed");
        });
        expect(screen.getByTestId("activity-detail")).toHaveTextContent(
            "1 memory · Vector + Keyword",
        );
        expect(screen.getByTestId("activity-evidence")).toHaveTextContent("result-metadata");
        expect(screen.getByTestId("activity-steps")).not.toHaveTextContent(/:(running|waiting)/);
    });

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
