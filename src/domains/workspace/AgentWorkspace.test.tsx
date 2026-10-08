import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

const ipc = vi.hoisted(() => ({
    getHermesBridgeStatus: vi.fn(),
    installHermesBridge: vi.fn(),
    saveHermesSetup: vi.fn(),
    listAgentChats: vi.fn(),
    getAgentChat: vi.fn(),
    deleteAgentChat: vi.fn(),
    sendHermesMessage: vi.fn(),
    cancelHermesMessage: vi.fn(),
    listMemoryCards: vi.fn(),
    reopenMemory: vi.fn(),
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
    ipc.cancelHermesMessage.mockResolvedValue(undefined);
});

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("AgentWorkspace", () => {
    it("still opens setup when Hermes status cannot be read, and retries", async () => {
        ipc.getHermesBridgeStatus.mockRejectedValueOnce(new Error("vault is locked"));
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);

        expect(await screen.findByRole("heading", { name: "Hermes isn't reachable yet" })).toBeInTheDocument();
        expect(screen.getByRole("alert")).toHaveTextContent("vault is locked");
        ipc.getHermesBridgeStatus.mockResolvedValue(hermes(false));
        fireEvent.click(screen.getByRole("button", { name: "Retry" }));
        expect(await screen.findByRole("heading", { name: "Choose a model" })).toBeInTheDocument();
    });

    it("asks for a model before offering the chat", async () => {
        ipc.getHermesBridgeStatus.mockResolvedValue(hermes(false));
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);

        expect(await screen.findByRole("heading", { name: "Choose a model" })).toBeInTheDocument();
        expect(screen.queryByLabelText("Message Hermes")).not.toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Set up a model" })).toBeInTheDocument();
    });

    it("says what a cloud provider is sent and keeps related memories off unless chosen", async () => {
        ipc.getHermesBridgeStatus.mockResolvedValue(hermes(false));
        ipc.saveHermesSetup.mockResolvedValue(hermes(true));
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);

        const note = await screen.findByRole("note", { name: "What is sent" });
        expect(note).toHaveTextContent("Sent to chatgpt.com: your messages and the memories you attach.");
        const related = within(note).getByRole("checkbox");
        expect(related).not.toBeChecked();

        fireEvent.click(screen.getByRole("button", { name: "Ollama" }));
        expect(screen.getByRole("note", { name: "What is sent" })).toHaveTextContent("Runs on this Mac");
        expect(within(screen.getByRole("note", { name: "What is sent" })).queryByRole("checkbox")).toBeNull();
    });

    it("shows the memories FNDR added to a message", async () => {
        ipc.getHermesBridgeStatus.mockResolvedValue({ ...hermes(true), related_memories: true });
        ipc.sendHermesMessage.mockResolvedValue({
            response_id: "r1",
            conversation_id: "c",
            content: "Here is the plan.",
            auto_memories: [{ id: "m9", title: "Quarterly review deck", appName: "Keynote", timestamp: 1 }],
        });
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);
        expect(await screen.findByText(/related ones FNDR finds/)).toBeInTheDocument();
        const input = await screen.findByLabelText("Message Hermes");
        await waitFor(() => expect(input).toBeEnabled());
        fireEvent.change(input, { target: { value: "What did I decide?" } });
        fireEvent.keyDown(input, { key: "Enter" });

        const added = await screen.findByLabelText("Memories FNDR added");
        expect(added).toHaveTextContent("Quarterly review deck");
    });

    it("opens the memory an answer cites, counting FNDR's own before the attached ones", async () => {
        ipc.reopenMemory.mockResolvedValue(true);
        ipc.getHermesBridgeStatus.mockResolvedValue({ ...hermes(true), related_memories: true });
        ipc.sendHermesMessage.mockResolvedValue({
            response_id: "r1",
            conversation_id: "c",
            content: "The deck says Q3 [1], and the notes agree [2]. Nothing backs [7].",
            auto_memories: [{ id: "m9", title: "Quarterly review deck", appName: "Keynote", timestamp: 1 }],
        });
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);
        const input = await screen.findByLabelText("Message Hermes");
        await waitFor(() => expect(input).toBeEnabled());
        fireEvent.click(screen.getByRole("button", { name: "Memories" }));
        fireEvent.click(await screen.findByRole("option", { name: /Chunking paper notes/ }));
        fireEvent.change(input, { target: { value: "What did I decide?" } });
        fireEvent.keyDown(input, { key: "Enter" });

        fireEvent.click(await screen.findByRole("button", { name: "Open memory 2: Chunking paper notes" }));
        expect(ipc.reopenMemory).toHaveBeenCalledWith("mem-1");
        fireEvent.click(screen.getByRole("button", { name: "Open memory 1: Quarterly review deck" }));
        expect(ipc.reopenMemory).toHaveBeenCalledWith("m9");
        expect(screen.queryByRole("button", { name: /Open memory 7/ })).not.toBeInTheDocument();
    });

    it("names the one missing step in the header chip", async () => {
        ipc.getHermesBridgeStatus.mockResolvedValue({ ...hermes(false), codex_logged_in: true });
        const { unmount } = render(<AgentWorkspace isVisible onClose={vi.fn()} />);
        expect(await screen.findByRole("button", { name: "Choose a model" })).toBeInTheDocument();
        unmount();

        ipc.getHermesBridgeStatus.mockResolvedValue({ ...hermes(true), codex_logged_in: false });
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);
        expect(await screen.findByRole("button", { name: "Reconnect ChatGPT" })).toBeInTheDocument();
    });

    it("hands the message back when sending fails", async () => {
        ipc.sendHermesMessage.mockRejectedValue(new Error("Hermes API request failed."));
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);
        const input = await screen.findByLabelText("Message Hermes");
        await waitFor(() => expect(input).toBeEnabled());

        fireEvent.change(input, { target: { value: "Summarize what I read" } });
        fireEvent.keyDown(input, { key: "Enter" });

        expect(await screen.findByRole("alert")).toHaveTextContent("Hermes API request failed.");
        expect(input).toHaveValue("Summarize what I read");
        const sent = screen.getByText("Summarize what I read", { selector: ".aw-bubble" }).closest(".aw-message");
        expect(within(sent as HTMLElement).getByText("Not sent")).toBeInTheDocument();
    });

    it("stops waiting for a reply, hands the message back, and ignores a late answer", async () => {
        let answer: (reply: unknown) => void = () => undefined;
        ipc.sendHermesMessage.mockReturnValue(new Promise((resolve) => (answer = resolve)));
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);
        const input = await screen.findByLabelText("Message Hermes");
        await waitFor(() => expect(input).toBeEnabled());

        fireEvent.change(input, { target: { value: "Draft the email" } });
        fireEvent.keyDown(input, { key: "Enter" });
        fireEvent.click(await screen.findByRole("button", { name: "Stop" }));

        expect(ipc.cancelHermesMessage).toHaveBeenCalledWith(expect.stringMatching(/^fndr-/));
        expect(screen.getByRole("button", { name: "Send" })).toBeInTheDocument();
        expect(input).toHaveValue("Draft the email");
        expect(document.querySelector(".aw-bubble")).toBeNull();

        await act(async () => answer({ response_id: "r", conversation_id: "c", content: "Late reply." }));
        expect(screen.queryByText("Late reply.")).not.toBeInTheDocument();
    });

    it("traces the actual Hermes installation request without exposing backend errors", async () => {
        let finishInstall!: () => void;
        ipc.getHermesBridgeStatus.mockResolvedValue({
            ...hermes(false),
            installed: false,
            bundled_repo_available: false,
        });
        ipc.installHermesBridge.mockReturnValue(new Promise<void>((resolve) => {
            finishInstall = resolve;
        }));
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);

        fireEvent.click(await screen.findByRole("button", { name: "Install Hermes" }));

        const trace = await screen.findByLabelText("Agent setup activity");
        expect(within(trace).getByRole("status")).toHaveTextContent("Requesting Hermes installation");
        expect(within(trace).getByRole("status")).toHaveTextContent("Running");
        expect(trace).not.toHaveTextContent("curl");
        finishInstall();
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

    it("shows the real agent request boundary and model without copying message content into the trace", async () => {
        let finishRequest!: (value: { response_id: string; conversation_id: string; content: string }) => void;
        ipc.sendHermesMessage.mockReturnValueOnce(new Promise((resolve) => {
            finishRequest = resolve;
        }));

        render(<AgentWorkspace isVisible onClose={vi.fn()} />);
        const input = await screen.findByLabelText("Message Hermes");
        await waitFor(() => expect(input).toBeEnabled());

        fireEvent.change(input, { target: { value: "Private planning prompt" } });
        fireEvent.click(screen.getByRole("button", { name: "Send" }));

        const trace = await screen.findByLabelText("Agent request activity");
        expect(within(trace).getByRole("status")).toHaveTextContent("Waiting for ChatGPT · gpt-6-sol");
        expect(within(trace).getByRole("status")).toHaveTextContent("Hermes bridge");
        expect(trace).not.toHaveTextContent("Private planning prompt");
        expect(trace.parentElement?.closest("[aria-live]")).toBeNull();

        finishRequest({ response_id: "r-live", conversation_id: "c-live", content: "A verified response." });

        await waitFor(() => {
            expect(within(trace).getByRole("status")).toHaveTextContent("Response received from ChatGPT · gpt-6-sol");
        });
        expect(within(trace).getByRole("status")).toHaveTextContent("Completed");
    });

    it("does not append an old response or trace after starting a new chat", async () => {
        let finishRequest!: (value: { response_id: string; conversation_id: string; content: string }) => void;
        ipc.sendHermesMessage.mockReturnValueOnce(new Promise((resolve) => {
            finishRequest = resolve;
        }));

        render(<AgentWorkspace isVisible onClose={vi.fn()} />);
        const input = await screen.findByLabelText("Message Hermes");
        await waitFor(() => expect(input).toBeEnabled());

        fireEvent.change(input, { target: { value: "Request for the old chat" } });
        fireEvent.click(screen.getByRole("button", { name: "Send" }));
        expect(await screen.findByLabelText("Agent request activity")).toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "New chat" }));
        expect(screen.queryByLabelText("Agent request activity")).toBeNull();
        expect(screen.getByLabelText("Message Hermes")).toBeEnabled();

        finishRequest({
            response_id: "stale-response",
            conversation_id: "old-chat",
            content: "This reply belongs to the old chat.",
        });

        await waitFor(() => expect(ipc.sendHermesMessage).toHaveBeenCalledTimes(1));
        expect(screen.queryByText("This reply belongs to the old chat.")).toBeNull();
        expect(screen.queryByLabelText("Agent request activity")).toBeNull();
    });

    it("does not let an older chat load overwrite a newer selection", async () => {
        const oldChat = {
            id: "fndr-old",
            title: "Old plan",
            createdAt: 1,
            updatedAt: 2,
            messages: [{ role: "assistant", content: "Old chat response", at: 2, memories: [] }],
        };
        const newerChat = {
            id: "fndr-newer",
            title: "Newer plan",
            createdAt: 3,
            updatedAt: 4,
            messages: [{ role: "assistant", content: "Newer chat response", at: 4, memories: [] }],
        };
        const pending = new Map<string, (chat: typeof oldChat) => void>();
        ipc.listAgentChats.mockResolvedValue([
            { id: oldChat.id, title: oldChat.title, updatedAt: oldChat.updatedAt, messageCount: 1 },
            { id: newerChat.id, title: newerChat.title, updatedAt: newerChat.updatedAt, messageCount: 1 },
        ]);
        ipc.getAgentChat.mockImplementation((id: string) => new Promise<typeof oldChat>((resolve) => {
            pending.set(id, resolve);
        }));
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);

        fireEvent.click(await screen.findByRole("button", { name: /^Old plan/ }));
        fireEvent.click(screen.getByRole("button", { name: /^Newer plan/ }));
        await waitFor(() => expect(pending.size).toBe(2));

        await act(async () => {
            pending.get(newerChat.id)?.(newerChat);
        });
        expect(await screen.findByText("Newer chat response")).toBeInTheDocument();

        await act(async () => {
            pending.get(oldChat.id)?.(oldChat);
        });
        expect(screen.getByText("Newer chat response")).toBeInTheDocument();
        expect(screen.queryByText("Old chat response")).not.toBeInTheDocument();
    });

    it("does not let a pending chat load overwrite a new chat", async () => {
        const oldChat = {
            id: "fndr-old",
            title: "Old plan",
            createdAt: 1,
            updatedAt: 2,
            messages: [{ role: "assistant", content: "Old chat response", at: 2, memories: [] }],
        };
        let finishLoad!: (chat: typeof oldChat) => void;
        ipc.listAgentChats.mockResolvedValue([
            { id: oldChat.id, title: oldChat.title, updatedAt: oldChat.updatedAt, messageCount: 1 },
        ]);
        ipc.getAgentChat.mockReturnValue(new Promise<typeof oldChat>((resolve) => {
            finishLoad = resolve;
        }));
        render(<AgentWorkspace isVisible onClose={vi.fn()} />);

        fireEvent.click(await screen.findByRole("button", { name: /^Old plan/ }));
        fireEvent.click(screen.getByRole("button", { name: "New chat" }));
        await act(async () => {
            finishLoad(oldChat);
        });

        expect(screen.queryByText("Old chat response")).not.toBeInTheDocument();
        expect(screen.getByText("What should we work on?")).toBeInTheDocument();
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
