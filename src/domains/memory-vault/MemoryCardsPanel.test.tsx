import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryCardsPanel } from "./MemoryCardsPanel";
import {
    findVisuallySimilarMemories,
    getFullGraph,
    getMemoryDebugInspector,
    listMemoryCards,
    listNeedsSignalMemoryCards,
    reopenMemory,
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
    fndrGetMemorySourceStatements: vi.fn().mockResolvedValue([]),
    fndrGetRelatedMemories: vi.fn().mockResolvedValue([]),
    getMemoryDebugInspector: vi.fn().mockResolvedValue({}),
    listMemoryCards: vi.fn(),
    listNeedsSignalMemoryCards: vi.fn().mockResolvedValue([]),
    reopenMemory: vi.fn().mockResolvedValue(true),
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
        timestamp: Date.now() - index * 31 * 60 * 1000,
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
    vi.useRealTimers();
});

/** Local 3 pm on Sep 22 2026, so Today and Yesterday never straddle midnight mid-test. */
const VAULT_NOW = new Date(2026, 8, 22, 15, 0).getTime();
const HOUR = 60 * 60 * 1000;

function renderVault() {
    return render(
        <MemoryCardsPanel isVisible onClose={() => {}} appNames={["VS Code"]} feature="vault" />,
    );
}

describe("MemoryCardsPanel", () => {
    it("shows an evidence-backed trace when the vault index finishes loading", async () => {
        vi.mocked(listMemoryCards).mockResolvedValue([card(1), card(2)]);

        render(
            <MemoryCardsPanel
                isVisible
                onClose={vi.fn()}
                appNames={["VS Code"]}
                feature="vault"
            />,
        );

        const trace = await screen.findByLabelText("Memory Vault loading activity");
        expect(within(trace).getByText("Loaded saved-memory index")).toBeInTheDocument();
        fireEvent.click(within(trace).getByRole("button", { name: "Show Memory Vault loading activity details" }));
        expect(within(trace).getByText("2 memories returned")).toBeInTheDocument();
        expect(within(trace).getByText(/verified result/i)).toBeInTheDocument();
    });

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

        fireEvent.click(await screen.findByRole("button", { name: "Vault options" }));
        fireEvent.click(screen.getByRole("menuitem", { name: "Excluded captures (1)" }));

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
        const trace = screen.getByRole("region", { name: "Visual similarity activity" });
        expect(trace).toHaveTextContent("Compared local visual embeddings");
        fireEvent.click(within(trace).getByRole("button", {
            name: "Show Visual similarity activity details",
        }));
        expect(trace).toHaveTextContent("CLIP image index");
        expect(trace).toHaveTextContent("1 match returned");
        fireEvent.click(result);
        expect(
            await screen.findByRole("dialog", { name: "Expanded memory: Memory 2" }),
        ).toBeInTheDocument();
    });

    it("does not offer screen similarity for an agent note", async () => {
        vi.mocked(listMemoryCards).mockResolvedValue([{ ...card(1), source_type: "agent", added_by: "Claude Code" }]);
        render(<MemoryCardsPanel isVisible={true} onClose={() => {}} appNames={["Agent note"]} feature="vault" />);
        fireEvent.click(await screen.findByRole("button", { name: "Open memory: Memory 1" }));
        expect(await screen.findByRole("dialog", { name: "Expanded memory: Memory 1" })).toBeTruthy();
        expect(screen.queryByRole("button", { name: "Find similar screens" })).toBeNull();
        expect(findVisuallySimilarMemories).not.toHaveBeenCalled();
    });

    it("groups memories by day, then by project or app thread, with a source icon per row", async () => {
        vi.useFakeTimers({ now: VAULT_NOW, toFake: ["Date"] });
        vi.mocked(listMemoryCards).mockResolvedValue([
            { ...card(1), title: "Read the essay rubric", project: "HIST 2100 essay", app_name: "Google Chrome", url: "https://canvas.example.edu/a/7", timestamp: VAULT_NOW - HOUR },
            { ...card(2), title: "Read chapter 4", project: "HIST 2100 essay", app_name: "Preview", reopen_target: "file:///tmp/fixtures/reader.pdf", timestamp: VAULT_NOW - 2 * HOUR },
            { ...card(3), title: "Ran the test suite", app_name: "Terminal", timestamp: VAULT_NOW - 24 * HOUR },
        ]);

        renderVault();

        const today = await screen.findByRole("region", { name: "Today" });
        expect(within(today).getByRole("heading", { name: /HIST 2100 essay/ })).toBeInTheDocument();
        expect(within(today).getByRole("img", { name: "Web page" })).toBeInTheDocument();
        expect(within(today).getByRole("img", { name: "Document" })).toBeInTheDocument();
        const yesterday = screen.getByRole("region", { name: "Yesterday" });
        expect(within(yesterday).getByRole("heading", { name: /Terminal/ })).toBeInTheDocument();
        expect(within(yesterday).getByRole("button", { name: "Open memory: Ran the test suite" })).toBeInTheDocument();
        expect(within(yesterday).getByRole("img", { name: "Screen capture" })).toBeInTheDocument();
    });

    it("folds near-duplicates behind an N similar toggle that expands them in place", async () => {
        vi.useFakeTimers({ now: VAULT_NOW, toFake: ["Date"] });
        vi.mocked(listMemoryCards).mockResolvedValue(
            [1, 2, 3].map((index) => ({
                ...card(index),
                title: "Drafted the demo script",
                app_name: "Notes",
                project: "Beta demo",
                timestamp: VAULT_NOW - index * 10 * 60 * 1000,
            })),
        );

        renderVault();

        const toggle = await screen.findByRole("button", { name: /^2 more moments/ });
        expect(screen.getAllByRole("button", { name: "Open memory: Drafted the demo script" })).toHaveLength(1);
        expect(toggle).toHaveAttribute("aria-expanded", "false");

        fireEvent.click(toggle);

        expect(toggle).toHaveAttribute("aria-expanded", "true");
        expect(screen.getAllByRole("button", { name: "Open memory: Drafted the demo script" })).toHaveLength(3);
    });

    it("prints the folded session time range on its row", async () => {
        vi.useFakeTimers({ now: VAULT_NOW, toFake: ["Date"] });
        vi.mocked(listMemoryCards).mockResolvedValue([
            { ...card(1), title: "First moment", timestamp: VAULT_NOW - 60 * 60 * 1000 },
            { ...card(2), title: "Second moment", timestamp: VAULT_NOW - 35 * 60 * 1000 },
            { ...card(3), title: "Latest moment", timestamp: VAULT_NOW - 10 * 60 * 1000 },
        ]);

        renderVault();

        expect(await screen.findByLabelText("Session time range")).toHaveTextContent(/to/);
    });

    it("reopens a row's source in one click and a folded duplicate's source in two", async () => {
        vi.useFakeTimers({ now: VAULT_NOW, toFake: ["Date"] });
        vi.mocked(listMemoryCards).mockResolvedValue([
            { ...card(1), title: "Read the rubric", app_name: "Google Chrome", reopen_target: "https://canvas.example.edu/a/7", timestamp: VAULT_NOW - HOUR },
            { ...card(2), title: "Read the rubric", app_name: "Google Chrome", reopen_target: "https://canvas.example.edu/a/8", timestamp: VAULT_NOW - 2 * HOUR },
        ]);

        renderVault();

        fireEvent.click(await screen.findByRole("button", { name: "Open source: Read the rubric" }));
        expect(reopenMemory).toHaveBeenLastCalledWith("memory-1");

        fireEvent.click(screen.getByRole("button", { name: /^1 more moment/ }));
        fireEvent.click(screen.getAllByRole("button", { name: "Open source: Read the rubric" })[1]);
        expect(reopenMemory).toHaveBeenLastCalledWith("memory-2");
    });

    it("keeps the graph strip behind a Connections toggle", async () => {
        vi.mocked(listMemoryCards).mockResolvedValue([card(1)]);

        renderVault();

        fireEvent.click(await screen.findByRole("button", { name: "Vault options" }));
        const toggle = screen.getByRole("menuitem", { name: "Connections" });
        expect(toggle).toHaveAttribute("aria-pressed", "false");
        expect(screen.queryByRole("region", { name: "Connections" })).toBeNull();
        expect(getFullGraph).not.toHaveBeenCalled();

        fireEvent.click(toggle);

        const strip = screen.getByRole("region", { name: "Connections" });
        await waitFor(() => expect(getFullGraph).toHaveBeenCalledTimes(1));
        expect(await within(strip).findByText("No connections to show yet.")).toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Vault options" }));
        const activeToggle = screen.getByRole("menuitem", { name: "Connections" });
        expect(activeToggle).toHaveAttribute("aria-pressed", "true");
        fireEvent.click(activeToggle);
        expect(screen.queryByRole("region", { name: "Connections" })).toBeNull();
    });

    it("puts Connections and Excluded captures in the same overflow menu", async () => {
        vi.mocked(listMemoryCards).mockResolvedValue([card(1)]);
        vi.mocked(listNeedsSignalMemoryCards).mockResolvedValue([
            { card: card(2), reason_code: "low_signal", reason: "Not enough context." },
        ]);

        renderVault();

        expect(screen.queryByRole("menu")).toBeNull();
        fireEvent.click(await screen.findByRole("button", { name: "Vault options" }));
        const menu = screen.getByRole("menu", { name: "Memory Vault options" });
        expect(within(menu).getByRole("menuitem", { name: "Connections" })).toBeInTheDocument();
        expect(within(menu).getByRole("menuitem", { name: "Excluded captures (1)" })).toBeInTheDocument();
    });
});
