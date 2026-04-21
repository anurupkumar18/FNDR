import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { MemoryCardsPanel } from "./MemoryCardsPanel";
import { listMemoryCards } from "../api/tauri";
import type { MemoryCard } from "../api/tauri";

vi.mock("../api/tauri", () => ({
    deleteMemory: vi.fn(),
    listMemoryCards: vi.fn(),
}));

function card(index: number): MemoryCard {
    return {
        id: `memory-${index}`,
        title: `Memory ${index}`,
        summary: `Worked through memory loading issue ${index}.`,
        action: "Reviewed memory loading",
        context: ["FNDR"],
        timestamp: Date.now() - index,
        app_name: "VS Code",
        window_title: `Memory ${index}`,
        score: 1,
        source_count: 1,
        raw_snippets: [`Worked through memory loading issue ${index}.`],
    };
}

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("MemoryCardsPanel", () => {
    it("requests the full all-app browse limit and renders returned cards", async () => {
        vi.mocked(listMemoryCards).mockResolvedValue(
            Array.from({ length: 1500 }, (_, index) => card(index))
        );

        render(
            <MemoryCardsPanel
                isVisible={true}
                onClose={() => {}}
                appNames={["VS Code"]}
            />
        );

        await waitFor(() => {
            expect(listMemoryCards).toHaveBeenCalledWith(1500, null);
        });
        expect(await screen.findByText("1500 cards")).toBeInTheDocument();
    });
});
