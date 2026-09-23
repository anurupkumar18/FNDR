import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { AskPanel } from "./AskPanel";
import { fndrAnswer, type ComposedAnswer, type MemoryCard } from "@/shared/ipc/tauri";

vi.mock("@/shared/ipc/tauri", () => ({ fndrAnswer: vi.fn() }));

const source: MemoryCard = {
    id: "demo-wed-chunking-paper",
    title: "Chunking paper results",
    summary: "512-token parents with 128-token children gave the best recall.",
    action: "",
    context: [],
    timestamp: Date.now() - 86_400_000,
    app_name: "Preview",
    window_title: "Moreno2026_ChunkingForRAG.pdf",
    score: 0.9,
    source_count: 1,
    raw_snippets: [],
};

function answer(overrides: Partial<ComposedAnswer>): ComposedAnswer {
    return {
        query: "q",
        answer: "The paper recommends 512-token parents with 128-token children.",
        evidence: { files: [], commands: [], decisions: [], errors: [], todos: [], urls: [] },
        cards: [source],
        verify_outcome: { kind: "grounded", confidence: 0.82 },
        surfacing_reasons: [],
        ...overrides,
    };
}

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("AskPanel", () => {
    it("owns modal focus, closes on Escape, and restores the invoking control", async () => {
        const onClose = vi.fn();
        const panel = (visible: boolean) => (
            <>
                <button type="button">Ask launcher</button>
                <AskPanel isVisible={visible} onClose={onClose} onOpenMemoryById={() => {}} />
            </>
        );
        const { rerender } = render(panel(false));
        const launcher = screen.getByRole("button", { name: "Ask launcher" });
        launcher.focus();

        rerender(panel(true));
        const input = screen.getByRole("textbox", { name: "Search or ask FNDR" });
        await waitFor(() => expect(input).toHaveFocus());
        fireEvent.keyDown(input, { key: "Escape" });
        expect(onClose).toHaveBeenCalledTimes(1);

        rerender(panel(false));
        await waitFor(() => expect(launcher).toHaveFocus());
    });

    it("frames searching and asking as one local-memory workflow", () => {
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);
        expect(screen.getByRole("heading", { name: "Search & Ask FNDR" })).toBeInTheDocument();
        expect(screen.getByLabelText("Search or ask FNDR")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Search & Ask" })).toBeInTheDocument();
    });

    it("shows a grounded answer with clickable cited sources", async () => {
        vi.mocked(fndrAnswer).mockResolvedValue(answer({}));
        const onOpen = vi.fn();
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={onOpen} />);
        fireEvent.change(screen.getByLabelText("Search or ask FNDR"), {
            target: { value: "What chunk size did the chunking paper recommend?" },
        });
        fireEvent.click(screen.getByRole("button", { name: "Search & Ask" }));
        expect(await screen.findByText(/512-token parents with 128-token children\./)).toBeInTheDocument();
        expect(screen.getByText("Grounded in 1 memory")).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: /Chunking paper results/ }));
        expect(onOpen).toHaveBeenCalledWith("demo-wed-chunking-paper");
    });

    it("shows an honest refusal when evidence is missing", async () => {
        vi.mocked(fndrAnswer).mockResolvedValue(
            answer({
                answer: "I don't have enough grounded evidence to answer that yet.",
                cards: [],
                verify_outcome: { kind: "not_enough_evidence", reason: "no hits" },
            })
        );
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);
        fireEvent.change(screen.getByLabelText("Search or ask FNDR"), {
            target: { value: "What's my bank balance?" },
        });
        fireEvent.click(screen.getByRole("button", { name: "Search & Ask" }));
        expect(await screen.findByText("Not in your memories")).toBeInTheDocument();
        expect(screen.getByText(/only answers from what it captured on this Mac/)).toBeInTheDocument();
    });

    it("does not call the backend for a blank question", () => {
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);
        fireEvent.click(screen.getByRole("button", { name: "Search & Ask" }));
        expect(fndrAnswer).not.toHaveBeenCalled();
    });
});
