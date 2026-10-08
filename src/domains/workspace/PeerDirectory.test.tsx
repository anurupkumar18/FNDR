import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const ipc = vi.hoisted(() => ({
    listConfiguredPeers: vi.fn(),
    addConfiguredPeer: vi.fn(),
    removeConfiguredPeer: vi.fn(),
}));
vi.mock("@/shared/ipc/tauri", () => ipc);

import { PeerDirectory } from "./PeerDirectory";

beforeEach(() => {
    ipc.listConfiguredPeers.mockResolvedValue([]);
    ipc.addConfiguredPeer.mockResolvedValue({
        id: "peer-1", card_url: "https://peer.example/.well-known/agent-card.json",
        name: "Research peer", endpoint: "https://peer.example/a2a",
        requires_bearer: false, verified_at_ms: 1,
    });
    ipc.removeConfiguredPeer.mockResolvedValue(true);
});
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("PeerDirectory", () => {
    it("verifies a person-entered Card before showing the saved peer, then removes it", async () => {
        render(<PeerDirectory onBack={vi.fn()} />);
        expect(await screen.findByText("No peers saved.")).toBeInTheDocument();
        fireEvent.change(screen.getByRole("textbox", { name: "Agent Card URL" }), {
            target: { value: "https://peer.example/.well-known/agent-card.json" },
        });
        fireEvent.click(screen.getByRole("button", { name: "Verify and save" }));
        await waitFor(() => expect(ipc.addConfiguredPeer).toHaveBeenCalledWith("https://peer.example/.well-known/agent-card.json"));
        expect(await screen.findByText("Research peer")).toBeInTheDocument();
        expect(screen.getByText("https://peer.example/a2a")).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Remove Research peer" }));
        await waitFor(() => expect(ipc.removeConfiguredPeer).toHaveBeenCalledWith("peer-1"));
        expect(await screen.findByText("No peers saved.")).toBeInTheDocument();
    });

    it("shows validation errors without saving a peer", async () => {
        ipc.addConfiguredPeer.mockRejectedValue(new Error("Agent Card redirect is not allowed"));
        render(<PeerDirectory onBack={vi.fn()} />);
        fireEvent.change(screen.getByRole("textbox", { name: "Agent Card URL" }), {
            target: { value: "https://peer.example/card" },
        });
        fireEvent.click(screen.getByRole("button", { name: "Verify and save" }));
        expect(await screen.findByRole("alert")).toHaveTextContent("Agent Card redirect is not allowed");
        expect(screen.getByText("No peers saved.")).toBeInTheDocument();
    });

    it("shows a load failure without leaving a loading message", async () => {
        ipc.listConfiguredPeers.mockRejectedValue(new Error("Vault unavailable"));
        render(<PeerDirectory onBack={vi.fn()} />);
        expect(await screen.findByRole("alert")).toHaveTextContent("Vault unavailable");
        expect(screen.queryByText("Loading peers…")).not.toBeInTheDocument();
    });

    it("keeps a stale peer visible when removal did not happen", async () => {
        ipc.listConfiguredPeers.mockResolvedValue([await ipc.addConfiguredPeer()]);
        ipc.removeConfiguredPeer.mockResolvedValue(false);
        render(<PeerDirectory onBack={vi.fn()} />);
        fireEvent.click(await screen.findByRole("button", { name: "Remove Research peer" }));
        expect(await screen.findByRole("alert")).toHaveTextContent("already removed");
        expect(screen.getByText("Research peer")).toBeInTheDocument();
    });
});
