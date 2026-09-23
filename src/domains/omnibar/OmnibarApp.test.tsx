import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const eventMocks = vi.hoisted(() => ({
    handler: null as null | (() => void),
}));

const ipcMocks = vi.hoisted(() => ({
    copyClipboardEntry: vi.fn(),
    dismissOmnibar: vi.fn(),
    fndrAnswer: vi.fn(),
    getClipboardHistory: vi.fn(),
    omnibarOpenMemory: vi.fn(),
    pasteClipboardEntry: vi.fn(),
    searchMemoryCards: vi.fn(),
}));

vi.mock("@/shared/hooks/useTauriEvent", () => ({
    useTauriEvent: (_event: string, handler: () => void) => {
        eventMocks.handler = handler;
    },
}));
vi.mock("@/shared/ipc/tauri", () => ({
    ...ipcMocks,
    OMNIBAR_FOCUS_EVENT: "omnibar://focus",
}));

import { OmnibarApp } from "./OmnibarApp";

const clipboardEntry = {
    id: "clip-1",
    text: "A long-lived clipboard value",
    app_name: "Notes",
    window_title: "Reference",
    timestamp: Date.now(),
};

function memoryCard(id: string, title: string) {
    return {
        id,
        title,
        summary: `${title} summary`,
        action: "",
        context: [],
        timestamp: Date.now(),
        app_name: "Notes",
        window_title: "Project notes",
        score: 0.9,
        source_count: 1,
        raw_snippets: [],
    };
}

describe("OmnibarApp", () => {
    beforeEach(() => {
        eventMocks.handler = null;
        for (const mock of Object.values(ipcMocks)) mock.mockReset();
        ipcMocks.searchMemoryCards.mockResolvedValue([]);
        ipcMocks.getClipboardHistory.mockResolvedValue([]);
        ipcMocks.copyClipboardEntry.mockResolvedValue(undefined);
        ipcMocks.pasteClipboardEntry.mockResolvedValue(undefined);
        ipcMocks.dismissOmnibar.mockResolvedValue(undefined);
        ipcMocks.omnibarOpenMemory.mockResolvedValue(undefined);
    });

    afterEach(() => cleanup());

    it("identifies the foreground purpose and focuses memory search on open", () => {
        render(<OmnibarApp />);

        expect(screen.getByRole("dialog", { name: "FNDR Quick Find" })).toBeInTheDocument();
        expect(screen.getByRole("searchbox", { name: "Search your memory" })).toHaveFocus();
        expect(screen.getByRole("button", { name: "Search clipboard history" })).toBeInTheDocument();
    });

    it("keeps a memory-search failure distinct from an empty result", async () => {
        ipcMocks.searchMemoryCards.mockRejectedValue(new Error("index offline"));
        render(<OmnibarApp />);

        fireEvent.change(screen.getByRole("searchbox", { name: "Search your memory" }), {
            target: { value: "policy renewal" },
        });

        expect(await screen.findByRole("alert")).toHaveTextContent(
            "Couldn’t search your memory. Try again.",
        );
        expect(screen.queryByText("No memory matches")).not.toBeInTheDocument();
    });

    it("explains an answer failure without clearing the question", async () => {
        ipcMocks.fndrAnswer.mockRejectedValue(new Error("model unavailable"));
        render(<OmnibarApp />);

        const input = screen.getByRole("searchbox", { name: "Search your memory" });
        fireEvent.change(input, { target: { value: "What did I decide?" } });
        fireEvent.keyDown(input, { key: "Enter", metaKey: true });

        expect(await screen.findByRole("alert")).toHaveTextContent(
            "Couldn’t answer from your memory right now.",
        );
        expect(input).toHaveValue("What did I decide?");
    });

    it("states how much local evidence supports an answer", async () => {
        ipcMocks.fndrAnswer.mockResolvedValue({
            query: "What did I decide?",
            answer: "You chose the local-first option.",
            evidence: { files: [], commands: [], decisions: [], errors: [], todos: [], urls: [] },
            cards: [{
                id: "memory-1",
                title: "Architecture decision",
                summary: "Selected a local-first approach",
                action: "",
                context: [],
                timestamp: Date.now(),
                app_name: "Notes",
                window_title: "Project notes",
                score: 0.92,
                source_count: 1,
                raw_snippets: [],
            }],
            verify_outcome: { kind: "grounded", confidence: 0.92 },
            surfacing_reasons: [],
        });
        render(<OmnibarApp />);

        const input = screen.getByRole("searchbox", { name: "Search your memory" });
        fireEvent.change(input, { target: { value: "What did I decide?" } });
        fireEvent.keyDown(input, { key: "Enter", metaKey: true });

        expect(await screen.findByText("You chose the local-first option.")).toBeInTheDocument();
        expect(screen.getByText("Grounded in 1 local memory")).toBeInTheDocument();
    });

    it("returns to search when an answered question is edited", async () => {
        ipcMocks.fndrAnswer.mockResolvedValue({
            query: "First question",
            answer: "First answer",
            evidence: { files: [], commands: [], decisions: [], errors: [], todos: [], urls: [] },
            cards: [],
            verify_outcome: { kind: "not_enough_evidence", reason: "test" },
            surfacing_reasons: [],
        });
        render(<OmnibarApp />);

        const input = screen.getByRole("searchbox", { name: "Search your memory" });
        fireEvent.change(input, { target: { value: "First question" } });
        fireEvent.keyDown(input, { key: "Enter", metaKey: true });
        expect(await screen.findByText("First answer")).toBeInTheDocument();

        fireEvent.change(input, { target: { value: "Second question" } });

        expect(screen.queryByText("First answer")).not.toBeInTheDocument();
        await waitFor(() => expect(ipcMocks.searchMemoryCards).toHaveBeenCalledWith(
            "Second question",
            undefined,
            undefined,
            8,
        ));
    });

    it("does not claim a clipboard copy succeeded when the native action fails", async () => {
        ipcMocks.getClipboardHistory.mockResolvedValue([clipboardEntry]);
        ipcMocks.copyClipboardEntry.mockRejectedValue(new Error("clipboard unavailable"));
        render(<OmnibarApp />);

        fireEvent.click(screen.getByRole("button", { name: "Search clipboard history" }));
        const clip = await screen.findByRole("option", {
            name: /A long-lived clipboard value/i,
        });
        fireEvent.click(clip);

        expect(await screen.findByRole("alert")).toHaveTextContent(
            "Couldn’t copy that clip. Try again.",
        );
        expect(screen.queryByText("Copied ✓")).not.toBeInTheDocument();
        expect(ipcMocks.dismissOmnibar).not.toHaveBeenCalled();
    });

    it("opens the keyboard-selected memory and pastes the selected clipboard item", async () => {
        ipcMocks.searchMemoryCards.mockResolvedValue([
            memoryCard("memory-1", "First memory"),
            memoryCard("memory-2", "Second memory"),
        ]);
        ipcMocks.getClipboardHistory.mockResolvedValue([clipboardEntry]);
        render(<OmnibarApp />);

        const memorySearch = screen.getByRole("searchbox", { name: "Search your memory" });
        fireEvent.change(memorySearch, { target: { value: "project" } });
        await screen.findByRole("option", { name: /Second memory/i });
        fireEvent.keyDown(memorySearch, { key: "ArrowDown" });
        fireEvent.keyDown(memorySearch, { key: "Enter" });
        expect(ipcMocks.omnibarOpenMemory).toHaveBeenCalledWith("memory-2");

        eventMocks.handler?.();
        fireEvent.click(screen.getByRole("button", { name: "Search clipboard history" }));
        const clipboardSearch = await screen.findByRole("searchbox", {
            name: "Search clipboard history",
        });
        await screen.findByRole("option", { name: /A long-lived clipboard value/i });
        fireEvent.keyDown(clipboardSearch, { key: "Enter", metaKey: true });
        expect(ipcMocks.pasteClipboardEntry).toHaveBeenCalledWith(clipboardEntry.text);
    });

    it("uses Escape to leave an answer before dismissing Quick Find", async () => {
        ipcMocks.fndrAnswer.mockResolvedValue({
            query: "Question",
            answer: "Answer",
            evidence: { files: [], commands: [], decisions: [], errors: [], todos: [], urls: [] },
            cards: [],
            verify_outcome: { kind: "not_enough_evidence", reason: "test" },
            surfacing_reasons: [],
        });
        render(<OmnibarApp />);
        const dialog = screen.getByRole("dialog");
        const input = screen.getByRole("searchbox", { name: "Search your memory" });
        fireEvent.change(input, { target: { value: "Question" } });
        fireEvent.keyDown(input, { key: "Enter", metaKey: true });
        expect(await screen.findByText("Answer")).toBeInTheDocument();

        fireEvent.keyDown(dialog, { key: "Escape" });
        expect(screen.queryByText("Answer")).not.toBeInTheDocument();
        expect(ipcMocks.dismissOmnibar).not.toHaveBeenCalled();

        fireEvent.keyDown(dialog, { key: "Escape" });
        await waitFor(() => expect(ipcMocks.dismissOmnibar).toHaveBeenCalled());
    });

    it("invalidates an in-flight answer when the user switches surfaces", async () => {
        let finishAnswer: (value: unknown) => void = () => {};
        ipcMocks.fndrAnswer.mockReturnValue(new Promise((resolve) => {
            finishAnswer = resolve;
        }));
        render(<OmnibarApp />);

        const input = screen.getByRole("searchbox", { name: "Search your memory" });
        fireEvent.change(input, { target: { value: "What changed?" } });
        fireEvent.keyDown(input, { key: "Enter", metaKey: true });
        fireEvent.keyDown(screen.getByRole("dialog"), { key: "Tab" });

        finishAnswer({
            query: "What changed?",
            answer: "Stale answer",
            evidence: { files: [], commands: [], decisions: [], errors: [], todos: [], urls: [] },
            cards: [],
            verify_outcome: { kind: "not_enough_evidence", reason: "test" },
            surfacing_reasons: [],
        });
        await Promise.resolve();
        fireEvent.keyDown(screen.getByRole("dialog"), { key: "Tab" });

        await waitFor(() => expect(screen.queryByText("Stale answer")).not.toBeInTheDocument());
    });
});
