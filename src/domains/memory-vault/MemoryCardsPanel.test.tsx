import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryCardsPanel } from "./MemoryCardsPanel";
import { listMemoryCards, listNeedsSignalMemoryCards } from "@/shared/ipc/tauri";
import type { MemoryCard } from "@/shared/ipc/tauri";

vi.mock("@/shared/ipc/tauri", () => ({
    deleteMemory: vi.fn(),
    listMemoryCards: vi.fn(),
    listNeedsSignalMemoryCards: vi.fn().mockResolvedValue([]),
    getFullGraph: vi.fn().mockResolvedValue({
        nodes: [],
        edges: [],
        louvain: {},
        cluster_0_name: "",
    }),
    getGraphForProject: vi.fn().mockResolvedValue({
        nodes: [],
        edges: [],
        louvain: {},
        cluster_0_name: "",
    }),
    getContextRuntimeStatus: vi.fn().mockResolvedValue({
        status: "idle",
        mcp_running: false,
        active_project: null,
        current_context_pack: null,
        recent_pack_count: 0,
        activity_event_count: 0,
        decision_count: 0,
        failed_writes: 0,
        last_error: null,
        latest_pack_summary: null,
        latest_pack_tokens_used: 0,
    }),
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
                feature="vault"
            />
        );

        await waitFor(() => {
            expect(listMemoryCards).toHaveBeenCalledWith(1500, null);
        });
        expect(await screen.findByText("1500 cards")).toBeInTheDocument();
    });

    it("keeps low-signal captures out of the normal list and reveals only their safe review metadata", async () => {
        vi.mocked(listMemoryCards).mockResolvedValue([card(1)]);
        vi.mocked(listNeedsSignalMemoryCards).mockResolvedValue([
            {
                card: { ...card(2), app_name: "ChatGPT", title: "ChatGPT_178970.png" },
                reason_code: "filename_only",
                reason: "Only a filename or app label was available.",
            },
        ]);

        render(
            <MemoryCardsPanel
                isVisible={true}
                onClose={() => {}}
                appNames={["VS Code", "ChatGPT"]}
                feature="vault"
            />,
        );

        expect(await screen.findByText("Memory 1")).toBeInTheDocument();
        expect(screen.queryByText("ChatGPT_178970.png")).toBeNull();

        fireEvent.click(await screen.findByRole("button", { name: "Needs more signal (1)" }));

        expect(await screen.findByText("Only a filename or app label was available.")).toBeInTheDocument();
        expect(
            within(screen.getByLabelText("Needs more signal review queue")).getByText("ChatGPT"),
        ).toBeInTheDocument();
        expect(screen.queryByText("ChatGPT_178970.png")).toBeNull();
    });
});
