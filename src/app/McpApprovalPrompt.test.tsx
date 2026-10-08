import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { McpApprovalPrompt as McpApprovalPromptData } from "@/shared/ipc/tauri";
import { McpApprovalPrompt } from "./McpApprovalPrompt";

const mocks = vi.hoisted(() => ({
    eventHandler: undefined as ((payload: unknown) => void) | undefined,
    resolve: vi.fn(),
}));

vi.mock("@/shared/hooks/useTauriEvent", () => ({
    useTauriEvent: (_event: string, handler: (payload: unknown) => void) => {
        mocks.eventHandler = handler;
    },
}));

vi.mock("@/shared/ipc/tauri", async () => {
    const actual = await vi.importActual<typeof import("@/shared/ipc/tauri")>("@/shared/ipc/tauri");
    return { ...actual, resolveMcpApproval: mocks.resolve };
});

const prompt: McpApprovalPromptData = {
    request_id: "mcp-approval-1",
    tool: "fndr.open_target",
    arguments: { memory_id: "memory-42" },
    expires_at_ms: Date.now() + 120_000,
};

describe("McpApprovalPrompt", () => {
    beforeEach(() => {
        mocks.eventHandler = undefined;
        mocks.resolve.mockReset().mockResolvedValue(true);
    });

    it("shows the structured MCP call and sends explicit approval", async () => {
        render(<McpApprovalPrompt />);
        mocks.eventHandler?.(prompt);

        expect(await screen.findByRole("alertdialog", { name: "MCP action approval" })).toBeInTheDocument();
        expect(screen.getByText("fndr.open_target")).toBeInTheDocument();
        expect(screen.getByText(/memory-42/)).toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Approve and run" }));
        expect(mocks.resolve).toHaveBeenCalledWith("mcp-approval-1", true);
    });

    it("sends an explicit decline without approving the request", async () => {
        render(<McpApprovalPrompt />);
        mocks.eventHandler?.(prompt);

        fireEvent.click(await screen.findByRole("button", { name: "Decline" }));
        expect(mocks.resolve).toHaveBeenCalledWith("mcp-approval-1", false);
    });
});
