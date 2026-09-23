import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import type { MemoryCard } from "@/shared/ipc/tauri";
import { ExpandedMemoryCard } from "./ExpandedMemoryCard";
import { fndrGetMemorySubgraph, fndrGetRelatedMemories } from "@/shared/ipc/tauri";

vi.mock("@/shared/ipc/tauri", () => ({
    fndrBuildContextPack: vi.fn().mockResolvedValue({}),
    fndrGetMemorySubgraph: vi.fn(),
    fndrGetRelatedMemories: vi.fn(),
}));

const card: MemoryCard = {
    id: "memory-primary",
    title: "Primary memory",
    summary: "A useful memory.",
    action: "Reviewed a memory",
    context: [],
    timestamp: Date.now(),
    app_name: "FNDR",
    window_title: "Memory Vault",
    score: 1,
    source_count: 1,
    raw_snippets: [],
};

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("ExpandedMemoryCard", () => {
    it("owns modal focus and shows bounded load failures instead of hanging", async () => {
        vi.mocked(fndrGetRelatedMemories).mockRejectedValue(new Error("offline"));
        vi.mocked(fndrGetMemorySubgraph).mockRejectedValue(new Error("offline"));

        render(<ExpandedMemoryCard card={card} onClose={() => {}} />);

        const dialog = screen.getByRole("dialog", { name: "Expanded memory: Primary memory" });
        expect(dialog).toHaveAttribute("aria-modal", "true");
        await waitFor(() => {
            expect(screen.getByRole("button", { name: "Close memory details" })).toHaveFocus();
        });
        expect(await screen.findByText("Related memories unavailable.")).toBeTruthy();
        expect(await screen.findByText("Graph context unavailable.")).toBeTruthy();
        expect(screen.queryByText(/loading subgraph/i)).toBeNull();
    });

    it("renders related memories as named actions", async () => {
        vi.mocked(fndrGetRelatedMemories).mockResolvedValue([
            { ...card, id: "memory-related", title: "Related memory" },
        ]);
        vi.mocked(fndrGetMemorySubgraph).mockResolvedValue({
            seed_ids: [card.id],
            node_count: 2,
            edge_count: 1,
        });
        const onOpenRelated = vi.fn();

        render(
            <ExpandedMemoryCard
                card={card}
                onClose={() => {}}
                onOpenRelated={onOpenRelated}
            />,
        );

        const action = await screen.findByRole("button", {
            name: "Open related memory: Related memory",
        });
        action.click();
        expect(onOpenRelated).toHaveBeenCalledWith("memory-related");
    });
});
