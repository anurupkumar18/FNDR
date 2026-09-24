import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

const ipc = vi.hoisted(() => ({
    getHermesBridgeStatus: vi.fn(),
    installHermesBridge: vi.fn(),
    saveHermesSetup: vi.fn(),
    listAgentChats: vi.fn(),
    getAgentChat: vi.fn(),
    deleteAgentChat: vi.fn(),
    sendHermesMessage: vi.fn(),
    listMemoryCards: vi.fn(),
    searchMemoryCards: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ipc);
vi.mock("./CodexAccountCard", () => ({ CodexAccountCard: () => <div>ChatGPT account</div> }));

import { AgentWorkspace } from "./AgentWorkspace";

const hermes = (configured: boolean) => ({
    installed: true,
    configured,
    bundled_repo_available: false,
    api_server_ready: configured,
    provider_kind: configured ? "codex" : null,
    model_name: configured ? "gpt-6-sol" : null,
    base_url: null,
    codex_logged_in: configured,
    ollama_installed: false,
    ollama_reachable: false,
    ollama_models: [],
    ollama_base_url: "http://127.0.0.1:11434/v1",
    install_command: "curl … | bash",
});

const card = {
    id: "mem-1",
    title: "Chunking paper notes",
    window_title: "paper.pdf",
    app_name: "Preview",
    timestamp: Date.now() - 3_600_000,
};

beforeEach(() => {
    ipc.getHermesBridgeStatus.mockResolvedValue(hermes(true));
    ipc.listAgentChats.mockResolvedValue([]);
    ipc.listMemoryCards.mockResolvedValue([card]);
    ipc.searchMemoryCards.mockResolvedValue([card]);
    ipc.sendHermesMessage.mockResolvedValue({ response_id: "r1", conversation_id: "c", content: "Here is the plan." });
    ipc.deleteAgentChat.mockResolvedValue(undefined);
});

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("AgentWorkspace", () => {
    it("asks for a model before offering the chat", async () => {
        ipc.getHermesBridgeStatus.mockResolvedValue(hermes(false));
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);

        expect(await screen.findByRole("heading", { name: "Choose a model" })).toBeInTheDocument();
        expect(screen.queryByLabelText("Message Hermes")).not.toBeInTheDocument();
        expect(screen.getByRole("button", { name: /not set up/i })).toBeInTheDocument();
    });

    it("sends a message with the memories the user attached", async () => {
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);
        const input = await screen.findByLabelText("Message Hermes");
        await waitFor(() => expect(input).toBeEnabled());

        fireEvent.click(screen.getByRole("button", { name: "Memories" }));
        fireEvent.click(await screen.findByRole("option", { name: /Chunking paper notes/ }));
        fireEvent.click(screen.getByRole("button", { name: "Memories · 1" }));

        fireEvent.change(input, { target: { value: "Summarize what I read" } });
        fireEvent.keyDown(input, { key: "Enter" });

        await waitFor(() =>
            expect(ipc.sendHermesMessage).toHaveBeenCalledWith(expect.stringMatching(/^fndr-/), "Summarize what I read", ["mem-1"]),
        );
        expect(await screen.findByText("Here is the plan.")).toBeInTheDocument();
        const sent = screen.getByText("Summarize what I read").closest(".aw-message") as HTMLElement;
        expect(within(sent).getByText("Chunking paper notes")).toBeInTheDocument();
        await waitFor(() => expect(ipc.listAgentChats).toHaveBeenCalledTimes(2));
    });

    it("reopens a past chat and keeps its conversation id", async () => {
        ipc.listAgentChats.mockResolvedValue([{ id: "fndr-old", title: "Plan the demo", updatedAt: Date.now(), messageCount: 2 }]);
        ipc.getAgentChat.mockResolvedValue({
            id: "fndr-old",
            title: "Plan the demo",
            createdAt: 1,
            updatedAt: 2,
            messages: [
                { role: "user", content: "Plan the demo", at: 1, memories: [] },
                { role: "assistant", content: "Three steps.", at: 2, memories: [] },
            ],
        });
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);

        fireEvent.click(await screen.findByRole("button", { name: /^Plan the demo/ }));
        expect(await screen.findByText("Three steps.")).toBeInTheDocument();

        const input = screen.getByLabelText("Message Hermes");
        fireEvent.change(input, { target: { value: "Add a Q&A slot" } });
        fireEvent.click(screen.getByRole("button", { name: "Send" }));
        await waitFor(() => expect(ipc.sendHermesMessage).toHaveBeenCalledWith("fndr-old", "Add a Q&A slot", []));
    });

    it("deletes a chat from the history", async () => {
        ipc.listAgentChats
            .mockResolvedValueOnce([{ id: "fndr-old", title: "Old chat", updatedAt: Date.now(), messageCount: 2 }])
            .mockResolvedValue([]);
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);

        fireEvent.click(await screen.findByRole("button", { name: 'Delete chat "Old chat"' }));
        await waitFor(() => expect(ipc.deleteAgentChat).toHaveBeenCalledWith("fndr-old"));
        await waitFor(() => expect(screen.queryByText("Old chat")).not.toBeInTheDocument());
    });
});
