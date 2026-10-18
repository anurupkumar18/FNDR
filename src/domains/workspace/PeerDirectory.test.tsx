import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const ipc = vi.hoisted(() => ({
    listConfiguredPeers: vi.fn(),
    addConfiguredPeer: vi.fn(),
    removeConfiguredPeer: vi.fn(),
    previewPeerDelegation: vi.fn(),
    sendPeerDelegation: vi.fn(),
    listPeerRuns: vi.fn(),
    refreshPeerTask: vi.fn(),
    cancelPeerTask: vi.fn(),
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
    ipc.listPeerRuns.mockResolvedValue([]);
    ipc.refreshPeerTask.mockResolvedValue({
        run: { local_id: "local-1", peer_id: "peer-1", host: "peer.example", created_at_ms: 1,
            status: "acknowledged", remote_task_id: "remote-1", remote_state: "TASK_STATE_COMPLETED" },
        output_text: "Finished report",
    });
    ipc.cancelPeerTask.mockResolvedValue({
        run: { local_id: "local-1", peer_id: "peer-1", host: "peer.example", created_at_ms: 1,
            status: "acknowledged", remote_task_id: "remote-1", remote_state: "TASK_STATE_WORKING" },
        output_text: null,
    });
    ipc.previewPeerDelegation.mockResolvedValue({
        peer_id: "peer-1", destination: "https://peer.example/a2a",
        message_text: "Task:\nReview the plan\n\nOutput goal:\nBrief report",
        attachments: [],
    });
    ipc.sendPeerDelegation.mockResolvedValue({
        run: {
            local_id: "local-1", message_id: "msg-1", peer_id: "peer-1", host: "peer.example",
            attachment_ids: [], payload_bytes: 200, created_at_ms: 1, status: "acknowledged",
            remote_task_id: "remote-1", remote_state: "TASK_STATE_SUBMITTED",
        },
        output_text: null,
    });
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
        expect(await screen.findByRole("button", { name: "Remove Research peer" })).toBeInTheDocument();
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

    it("reports when peer send history cannot be loaded", async () => {
        ipc.listPeerRuns.mockRejectedValue(new Error("History unavailable"));
        render(<PeerDirectory onBack={vi.fn()} />);
        expect(await screen.findByRole("alert")).toHaveTextContent("Could not load peer send history: History unavailable");
    });

    it("keeps a stale peer visible when removal did not happen", async () => {
        ipc.listConfiguredPeers.mockResolvedValue([await ipc.addConfiguredPeer()]);
        ipc.removeConfiguredPeer.mockResolvedValue(false);
        render(<PeerDirectory onBack={vi.fn()} />);
        fireEvent.click(await screen.findByRole("button", { name: "Remove Research peer" }));
        expect(await screen.findByRole("alert")).toHaveTextContent("already removed");
        expect(screen.getByRole("button", { name: "Remove Research peer" })).toBeInTheDocument();
    });

    it("previews a bounded task for a saved peer without sending until asked", async () => {
        ipc.listConfiguredPeers.mockResolvedValue([await ipc.addConfiguredPeer()]);
        render(<PeerDirectory onBack={vi.fn()} selectedMemories={[{ id: "memory-1", title: "Plan", appName: "Editor", timestamp: 1 }]} />);
        fireEvent.change(await screen.findByRole("textbox", { name: "Task for peer" }), { target: { value: "Review the plan" } });
        fireEvent.change(screen.getByRole("textbox", { name: "Output goal" }), { target: { value: "Brief report" } });
        fireEvent.click(screen.getByRole("button", { name: "Preview task" }));
        await waitFor(() => expect(ipc.previewPeerDelegation).toHaveBeenCalledWith("peer-1", "Review the plan", "Brief report", ["memory-1"]));
        await waitFor(() => expect(screen.getByLabelText("Task preview").querySelector("pre")).toHaveTextContent("Task: Review the plan Output goal: Brief report"));
        expect(screen.getByRole("button", { name: "Send task" })).toBeInTheDocument();
        expect(ipc.sendPeerDelegation).not.toHaveBeenCalled();
    });

    it("sends only the reviewed draft and shows the peer task ID", async () => {
        ipc.listConfiguredPeers.mockResolvedValue([await ipc.addConfiguredPeer()]);
        render(<PeerDirectory onBack={vi.fn()} />);
        fireEvent.change(await screen.findByRole("textbox", { name: "Task for peer" }), { target: { value: "Review the plan" } });
        fireEvent.change(screen.getByRole("textbox", { name: "Output goal" }), { target: { value: "Brief report" } });
        expect(screen.queryByRole("button", { name: "Send task" })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Preview task" }));
        fireEvent.click(await screen.findByRole("button", { name: "Send task" }));
        await waitFor(() => expect(ipc.sendPeerDelegation).toHaveBeenCalledWith(
            "peer-1", "Review the plan", "Brief report", [],
            "https://peer.example/a2a", "Task:\nReview the plan\n\nOutput goal:\nBrief report",
        ));
        expect(await screen.findByText(/remote-1/)).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Send task" })).not.toBeInTheDocument();
    });

    it("keeps a confirmed send visible when its history refresh fails", async () => {
        ipc.listConfiguredPeers.mockResolvedValue([await ipc.addConfiguredPeer()]);
        ipc.listPeerRuns.mockResolvedValueOnce([]).mockRejectedValueOnce(new Error("History unavailable"));
        render(<PeerDirectory onBack={vi.fn()} />);
        fireEvent.change(await screen.findByRole("textbox", { name: "Task for peer" }), { target: { value: "Review the plan" } });
        fireEvent.change(screen.getByRole("textbox", { name: "Output goal" }), { target: { value: "Brief report" } });
        fireEvent.click(screen.getByRole("button", { name: "Preview task" }));
        fireEvent.click(await screen.findByRole("button", { name: "Send task" }));
        expect(await screen.findByRole("alert")).toHaveTextContent("Could not load peer send history: History unavailable");
        expect(screen.getByText(/Peer task remote-1/)).toBeInTheDocument();
    });

    it("keeps the delivery warning when Send and history refresh both fail", async () => {
        ipc.listConfiguredPeers.mockResolvedValue([await ipc.addConfiguredPeer()]);
        ipc.sendPeerDelegation.mockRejectedValue(new Error("Peer task request failed; delivery is uncertain"));
        ipc.listPeerRuns.mockResolvedValueOnce([]).mockRejectedValueOnce(new Error("History unavailable"));
        render(<PeerDirectory onBack={vi.fn()} />);
        fireEvent.change(await screen.findByRole("textbox", { name: "Task for peer" }), { target: { value: "Review the plan" } });
        fireEvent.change(screen.getByRole("textbox", { name: "Output goal" }), { target: { value: "Brief report" } });
        fireEvent.click(screen.getByRole("button", { name: "Preview task" }));
        fireEvent.click(await screen.findByRole("button", { name: "Send task" }));
        expect(await screen.findByRole("alert")).toHaveTextContent("Could not load peer send history: History unavailable");
        expect(screen.getByRole("alert")).toHaveTextContent("delivery is uncertain");
    });

    it("explains a direct reply that has no displayable text", async () => {
        ipc.listConfiguredPeers.mockResolvedValue([await ipc.addConfiguredPeer()]);
        ipc.sendPeerDelegation.mockResolvedValue({
            run: {
                local_id: "local-1", message_id: "msg-1", peer_id: "peer-1", host: "peer.example",
                attachment_ids: [], payload_bytes: 200, created_at_ms: 1, status: "direct_reply",
                remote_task_id: null, remote_state: "DIRECT_MESSAGE_UNSUPPORTED",
            },
            output_text: null,
        });
        render(<PeerDirectory onBack={vi.fn()} />);
        fireEvent.change(await screen.findByRole("textbox", { name: "Task for peer" }), { target: { value: "Review the plan" } });
        fireEvent.change(screen.getByRole("textbox", { name: "Output goal" }), { target: { value: "Brief report" } });
        fireEvent.click(screen.getByRole("button", { name: "Preview task" }));
        fireEvent.click(await screen.findByRole("button", { name: "Send task" }));
        expect(await screen.findByText(/FNDR cannot display its reply/)).toBeInTheDocument();
        expect(screen.queryByLabelText("Untrusted peer output")).not.toBeInTheDocument();
    });

    it("checks a known peer task and shows its output only for review", async () => {
        ipc.listPeerRuns.mockResolvedValue([{
            local_id: "local-1", peer_id: "peer-1", host: "peer.example", created_at_ms: 1,
            status: "acknowledged", remote_task_id: "remote-1", remote_state: "TASK_STATE_WORKING",
        }]);
        render(<PeerDirectory onBack={vi.fn()} />);
        fireEvent.click(await screen.findByRole("button", { name: "Check status" }));
        await waitFor(() => expect(ipc.refreshPeerTask).toHaveBeenCalledWith("local-1"));
        expect(await screen.findByLabelText("Peer task result")).toHaveTextContent("Finished report");
    });

    it("does not call a cancellation confirmed while the peer still reports working", async () => {
        ipc.listPeerRuns.mockResolvedValue([{
            local_id: "local-1", peer_id: "peer-1", host: "peer.example", created_at_ms: 1,
            status: "acknowledged", remote_task_id: "remote-1", remote_state: "TASK_STATE_WORKING",
        }]);
        render(<PeerDirectory onBack={vi.fn()} />);
        fireEvent.click(await screen.findByRole("button", { name: "Request cancel" }));
        await waitFor(() => expect(ipc.cancelPeerTask).toHaveBeenCalledWith("local-1"));
        expect(await screen.findByText(/has not confirmed cancellation/)).toBeInTheDocument();
    });
});
