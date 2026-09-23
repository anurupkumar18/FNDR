import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryCardsPanel } from "./MemoryCardsPanel";
import {
    findVisuallySimilarMemories,
    getMemoryDebugInspector,
    listMemoryCards,
    listNeedsSignalMemoryCards,
} from "@/shared/ipc/tauri";
import type { MemoryCard, SearchResult } from "@/shared/ipc/tauri";

vi.mock("@/shared/ipc/tauri", () => ({
    deleteMemory: vi.fn(),
    findVisuallySimilarMemories: vi.fn().mockResolvedValue([]),
    fndrBuildContextPack: vi.fn().mockResolvedValue({}),
    fndrGetMemorySubgraph: vi.fn().mockResolvedValue({
        seed_ids: [],
        node_count: 0,
        edge_count: 0,
    }),
    fndrGetRelatedMemories: vi.fn().mockResolvedValue([]),
    getMemoryDebugInspector: vi.fn().mockResolvedValue({}),
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
    it("owns modal focus, closes on Escape, and restores the invoking control", async () => {
        vi.mocked(listMemoryCards).mockResolvedValue([]);
        const onClose = vi.fn();
        const panel = (visible: boolean) => (
            <>
                <button type="button">Vault launcher</button>
                <MemoryCardsPanel
                    isVisible={visible}
                    onClose={onClose}
                    appNames={[]}
                    feature="vault"
                />
            </>
        );
        const { rerender } = render(panel(false));
        const launcher = screen.getByRole("button", { name: "Vault launcher" });
        launcher.focus();

        rerender(panel(true));
        const closeButton = screen.getByRole("button", { name: "Close Memory Vault" });
        await waitFor(() => expect(closeButton).toHaveFocus());
        fireEvent.keyDown(screen.getByRole("dialog", { name: "Memory Vault" }), { key: "Escape" });
        expect(onClose).toHaveBeenCalledTimes(1);

        rerender(panel(false));
        await waitFor(() => expect(launcher).toHaveFocus());
    });

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
        expect(await screen.findByText("1,500 memories")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Close Memory Vault" })).toBeInTheDocument();
        expect(screen.getByRole("combobox", { name: "App" })).toBeInTheDocument();
        expect(screen.getByRole("combobox", { name: "When" })).toBeInTheDocument();
        expect(screen.getByRole("combobox", { name: "Activity" })).toBeInTheDocument();
        expect(screen.getByText("Showing 300 of 1,500")).toBeInTheDocument();
        expect(screen.queryByText("Memory 300")).toBeNull();

        fireEvent.click(screen.getByRole("button", { name: "Show 300 more memories" }));

        expect(await screen.findByText("Memory 300")).toBeInTheDocument();
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

        fireEvent.click(await screen.findByRole("button", { name: "Excluded captures (1)" }));

        expect(await screen.findByText("Only a filename or app label was available.")).toBeInTheDocument();
        expect(
            within(screen.getByLabelText("Excluded captures queue")).getByText("ChatGPT"),
        ).toBeInTheDocument();
        expect(screen.queryByText("ChatGPT_178970.png")).toBeNull();
    });

    it("opens a human-readable memory without automatically exposing developer diagnostics", async () => {
        vi.mocked(listMemoryCards).mockResolvedValue([card(1)]);

        render(
            <MemoryCardsPanel
                isVisible={true}
                onClose={() => {}}
                appNames={["VS Code"]}
                feature="vault"
            />,
        );

        fireEvent.click(await screen.findByRole("button", { name: "Open memory: Memory 1" }));

        expect(
            await screen.findByRole("dialog", { name: "Expanded memory: Memory 1" }),
        ).toBeInTheDocument();
        expect(getMemoryDebugInspector).not.toHaveBeenCalled();
        expect(screen.queryByText(/"memory_id"/)).not.toBeInTheDocument();
    });

    it("shows visual-similarity progress and lets the user open a returned memory", async () => {
        let resolveSimilar: (results: SearchResult[]) => void = () => {};
        vi.mocked(findVisuallySimilarMemories).mockImplementation(
            () => new Promise((resolve) => {
                resolveSimilar = resolve;
            }),
        );
        vi.mocked(listMemoryCards).mockResolvedValue([card(1), card(2)]);

        render(
            <MemoryCardsPanel
                isVisible={true}
                onClose={() => {}}
                appNames={["VS Code"]}
                feature="vault"
            />,
        );

        fireEvent.click(await screen.findByRole("button", { name: "Open memory: Memory 1" }));
        fireEvent.click(screen.getByRole("button", { name: "Find similar screens" }));

        expect(screen.getByRole("button", { name: "Finding similar screens" })).toBeDisabled();

        resolveSimilar([
            {
                id: "memory-2",
                timestamp: Date.now(),
                app_name: "VS Code",
                window_title: "Memory 2",
                session_id: "session-2",
                text: "A related screen",
                snippet: "A related screen",
                score: 0.91,
            },
        ]);

        const result = await screen.findByRole("button", {
            name: "Open visually similar memory: Memory 2",
        });
        fireEvent.click(result);
        expect(
            await screen.findByRole("dialog", { name: "Expanded memory: Memory 2" }),
        ).toBeInTheDocument();
    });
});
