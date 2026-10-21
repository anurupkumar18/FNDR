import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { OutputsPanel } from "./OutputsPanel";
import * as ipc from "@/shared/ipc/tauri";

vi.mock("@/shared/ipc/tauri", () => ({
    generateDailyOutput: vi.fn(),
    renderDailyOutput: vi.fn(),
    saveDailyOutput: vi.fn(),
    outputDestinationStatus: vi.fn(),
    setOutputDestination: vi.fn(),
    proposeDailyOutputSend: vi.fn(),
    resolveDailyOutputSend: vi.fn(),
    sendDailyOutputAllowed: vi.fn(),
    trustOutputDestination: vi.fn(),
    listOutputAllowedDestinations: vi.fn(),
    forgetOutputAllowedDestination: vi.fn(),
}));

const doc: ipc.DailyOutputDoc = {
    id: "d1",
    kind: "standup",
    title: "Standup, Oct 15",
    start_ms: 1,
    end_ms: 2,
    generated_at_ms: 3,
    model_polished: false,
    sections: [{
        heading: "Yesterday",
        lines: [{
            text: "Reviewed the Atlas export plan",
            project: null,
            citations: [{ kind: "memory", id: "memory-1", memory_id: "memory-1" }],
        }],
    }],
};

const card: ipc.DailyOutputApprovalCard = {
    approval_id: "approval-1",
    tool: "outputs.post_slack",
    action: "Post to Slack",
    destination: "hooks.slack.com",
    title: doc.title,
    body: "# Standup\n- Reviewed the Atlas export plan [1]",
    bytes: 60,
    expires_at_ms: Date.now() + 120_000,
    trimmed: false,
};

beforeEach(() => {
    vi.mocked(ipc.generateDailyOutput).mockResolvedValue(doc);
    vi.mocked(ipc.renderDailyOutput).mockResolvedValue(card.body);
    vi.mocked(ipc.outputDestinationStatus).mockResolvedValue({
        slack_webhook: true,
        linear_api_key: false,
        linear_team_id: false,
    });
    vi.mocked(ipc.proposeDailyOutputSend).mockResolvedValue(card);
    vi.mocked(ipc.sendDailyOutputAllowed).mockResolvedValue(null);
    vi.mocked(ipc.listOutputAllowedDestinations).mockResolvedValue([]);
    vi.mocked(ipc.trustOutputDestination).mockResolvedValue("trusted-1");
    vi.mocked(ipc.resolveDailyOutputSend).mockResolvedValue({
        tool: card.tool,
        destination: card.destination,
        detail: "Posted to Slack.",
        trust_id: "trust-1",
    });
});

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("OutputsPanel", () => {
    it("generates a cited standup and opens its source memory", async () => {
        const openMemory = vi.fn();
        render(<OutputsPanel isVisible onClose={vi.fn()} onOpenMemoryById={openMemory} />);
        fireEvent.click(screen.getByRole("button", { name: "Generate" }));
        await screen.findByText("Reviewed the Atlas export plan");
        expect(ipc.generateDailyOutput).toHaveBeenCalledWith("standup", expect.any(Number), expect.any(Number), false);
        fireEvent.click(screen.getByRole("button", { name: "Open memory memory-1" }));
        expect(openMemory).toHaveBeenCalledWith("memory-1");
    });

    it("keeps citations in saved output", async () => {
        vi.mocked(ipc.saveDailyOutput).mockResolvedValue("/tmp/standup.md");
        render(<OutputsPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        fireEvent.click(screen.getByRole("button", { name: "Generate" }));
        await screen.findByText("Reviewed the Atlas export plan");
        expect(screen.queryByLabelText("Include sources in copy and save")).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Save file" }));
        await waitFor(() => expect(ipc.saveDailyOutput).toHaveBeenCalledWith("d1", "markdown"));
    });

    it("shows the exact destination and body before posting", async () => {
        render(<OutputsPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        fireEvent.click(screen.getByRole("button", { name: "Generate" }));
        await screen.findByText("Reviewed the Atlas export plan");
        fireEvent.click(screen.getByRole("button", { name: "Post to Slack" }));
        expect(await screen.findByText("hooks.slack.com")).toBeInTheDocument();
        expect(screen.getByRole("alertdialog").querySelector("pre")?.textContent).toBe(card.body);
        expect(ipc.resolveDailyOutputSend).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole("button", { name: "Approve send" }));
        await waitFor(() => expect(ipc.resolveDailyOutputSend).toHaveBeenCalledWith("approval-1", true));
    });

    it("declines a send when the card is cancelled", async () => {
        render(<OutputsPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        fireEvent.click(screen.getByRole("button", { name: "Generate" }));
        await screen.findByText("Reviewed the Atlas export plan");
        fireEvent.click(screen.getByRole("button", { name: "Post to Slack" }));
        await screen.findByText("hooks.slack.com");
        fireEvent.click(screen.getByRole("button", { name: "Cancel send" }));
        await waitFor(() => expect(ipc.resolveDailyOutputSend).toHaveBeenCalledWith("approval-1", false));
    });

    it("offers an exact destination allow list only after a successful approved send", async () => {
        render(<OutputsPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        fireEvent.click(screen.getByRole("button", { name: "Generate" }));
        await screen.findByText("Reviewed the Atlas export plan");
        expect(screen.queryByRole("button", { name: "Always allow this destination" })).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Post to Slack" }));
        await screen.findByText("hooks.slack.com");
        fireEvent.click(screen.getByRole("button", { name: "Approve send" }));
        fireEvent.click(await screen.findByRole("button", { name: "Always allow this destination" }));
        await waitFor(() => expect(ipc.trustOutputDestination).toHaveBeenCalledWith("trust-1"));
        vi.mocked(ipc.sendDailyOutputAllowed).mockResolvedValueOnce({
            tool: card.tool,
            destination: card.destination,
            detail: "Posted to Slack.",
            trust_id: "trust-2",
        });
        fireEvent.click(screen.getByRole("button", { name: "Post to Slack" }));
        await waitFor(() => expect(ipc.sendDailyOutputAllowed).toHaveBeenCalledTimes(2));
        expect(ipc.proposeDailyOutputSend).toHaveBeenCalledTimes(1);
    });
});
