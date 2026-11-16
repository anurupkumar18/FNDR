import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { AskPanel } from "./AskPanel";
import { fndrAnswer, type ComposedAnswer, type MemoryCard } from "@/shared/ipc/tauri";

vi.mock("@/shared/ipc/tauri", () => ({ fndrAnswer: vi.fn(), reopenMemory: vi.fn() }));

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
    it("keeps both sides of successive exchanges and clears the composer", async () => {
        vi.mocked(fndrAnswer)
            .mockResolvedValueOnce(answer({ answer: "First grounded reply." }))
            .mockResolvedValueOnce(answer({ answer: "Second grounded reply." }));
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);
        const input = screen.getByLabelText("Search or ask FNDR");
        for (const [question, reply] of [["First question", "First grounded reply."], ["Second question", "Second grounded reply."]]) {
            fireEvent.change(input, { target: { value: question } });
            fireEvent.keyDown(input, { key: "Enter" });
            expect(await screen.findByText(reply)).toBeInTheDocument();
            expect(input).toHaveValue("");
        }
        expect(screen.getByText("First question")).toBeInTheDocument();
        expect(screen.getByText("Second question")).toBeInTheDocument();
        expect(screen.getByText("First grounded reply.")).toBeInTheDocument();
        expect(screen.getByText("Each message searches local memories independently.")).toBeInTheDocument();
    });

    it("starts a new chat without accepting a late reply from the previous one", async () => {
        let finish!: (value: ComposedAnswer) => void;
        vi.mocked(fndrAnswer).mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);
        fireEvent.change(screen.getByLabelText("Search or ask FNDR"), { target: { value: "Old question" } });
        fireEvent.click(screen.getByRole("button", { name: "Search & Ask" }));
        fireEvent.click(screen.getByRole("button", { name: "New chat" }));
        await act(async () => { finish(answer({ answer: "Late reply" })); });
        expect(screen.queryByText("Old question")).not.toBeInTheDocument();
        expect(screen.queryByText("Late reply")).not.toBeInTheDocument();
        expect(screen.getByLabelText("Search or ask FNDR")).toHaveFocus();
    });

    it("retries a failed turn in place without losing earlier exchanges", async () => {
        vi.mocked(fndrAnswer)
            .mockResolvedValueOnce(answer({ answer: "Keep this reply." }))
            .mockRejectedValueOnce(new Error("private backend error"))
            .mockResolvedValueOnce(answer({ answer: "Retried reply." }));
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);
        const input = screen.getByLabelText("Search or ask FNDR");
        fireEvent.change(input, { target: { value: "First question" } });
        fireEvent.keyDown(input, { key: "Enter" });
        await screen.findByText("Keep this reply.");
        fireEvent.change(input, { target: { value: "Failed question" } });
        fireEvent.keyDown(input, { key: "Enter" });
        fireEvent.click(await screen.findByRole("button", { name: "Try again" }));
        await screen.findByText("Retried reply.");
        expect(screen.getAllByText("Failed question")).toHaveLength(1);
        expect(screen.getByText("Keep this reply.")).toBeInTheDocument();
        expect(screen.queryByText("private backend error")).not.toBeInTheDocument();
    });

    it("does not submit Shift+Enter or an IME composition Enter", () => {
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);
        const input = screen.getByLabelText("Search or ask FNDR");
        fireEvent.change(input, { target: { value: "Draft question" } });
        fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
        fireEvent.keyDown(input, { key: "Enter", isComposing: true });
        expect(fndrAnswer).not.toHaveBeenCalled();
    });

    it("retains the thread when the panel is hidden and reopened", async () => {
        vi.mocked(fndrAnswer).mockResolvedValue(answer({ answer: "Reply to keep." }));
        const panel = (visible: boolean) => <AskPanel isVisible={visible} onClose={() => {}} onOpenMemoryById={() => {}} />;
        const { rerender } = render(panel(true));
        fireEvent.change(screen.getByLabelText("Search or ask FNDR"), { target: { value: "Question to keep" } });
        fireEvent.keyDown(screen.getByLabelText("Search or ask FNDR"), { key: "Enter" });
        await screen.findByText("Reply to keep.");
        rerender(panel(false));
        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
        rerender(panel(true));
        expect(screen.getByText("Question to keep")).toBeInTheDocument();
        expect(screen.getByText("Reply to keep.")).toBeInTheDocument();
    });

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

    it("shows Open source beside a cited memory that has a reopen target", async () => {
        vi.mocked(fndrAnswer).mockResolvedValue(
            answer({ cards: [{ ...source, reopen_target: "https://example.com/paper" }] }),
        );
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);
        fireEvent.change(screen.getByLabelText("Search or ask FNDR"), {
            target: { value: "chunk size" },
        });
        fireEvent.click(screen.getByRole("button", { name: "Search & Ask" }));
        await screen.findByText(/512-token parents/);
        expect(screen.getByRole("button", { name: "Open source" })).toBeInTheDocument();
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

    it("shows an evidence-backed activity trace while answering and after completion", async () => {
        let resolveAnswer!: (value: ComposedAnswer) => void;
        vi.mocked(fndrAnswer).mockImplementation(
            () => new Promise<ComposedAnswer>((resolve) => {
                resolveAnswer = resolve;
            }),
        );
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);

        fireEvent.change(screen.getByLabelText("Search or ask FNDR"), {
            target: { value: "What did I decide?" },
        });
        fireEvent.click(screen.getByRole("button", { name: "Search & Ask" }));

        expect(screen.getByLabelText("Ask FNDR activity")).toHaveTextContent(
            "Requesting an answer from local memory",
        );
        expect(screen.getByLabelText("Ask FNDR activity")).toHaveTextContent(
            "FNDR answer service",
        );
        expect(screen.getAllByRole("status")).toHaveLength(1);

        resolveAnswer(answer({
            cards: [{ ...source, matched_routes: ["Vector", "Keyword"] }],
        }));

        expect(await screen.findByText("Grounded answer returned")).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Show Ask FNDR activity details" }));
        expect(screen.getByText(/1 memory · Vector \+ Keyword/)).toBeInTheDocument();
        expect(screen.getAllByText(/Request boundary/).length).toBeGreaterThan(0);
        expect(screen.getAllByText(/Verified result/).length).toBeGreaterThan(0);
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
