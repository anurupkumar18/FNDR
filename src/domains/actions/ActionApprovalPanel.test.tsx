import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { ActionApprovalPanel } from "./ActionApprovalPanel";

const mocks = vi.hoisted(() => ({
    list: vi.fn(),
    resolve: vi.fn(),
    getSettings: vi.fn(),
    setSettings: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ({
    listActionApprovals: mocks.list,
    resolveActionApproval: mocks.resolve,
    getActionSettings: mocks.getSettings,
    setActionSettings: mocks.setSettings,
}));

const pending = {
    id: "synthetic-approval",
    caller: "mcp",
    surface: "mcp",
    tool: "fndr.open_target",
    title: "Reopen a memory's source",
    target: "memory-42",
    args: [{ name: "memory_id", value: "memory-42" }],
    reason: "Reopen a memory's source waits for a tap.",
    undo: { kind: "unavailable", reason: "Close the opened window yourself." },
    status: "pending",
    detail: null,
    created_at_ms: 1,
    expires_at_ms: 120_001,
};

beforeEach(() => {
    mocks.list.mockReset().mockResolvedValue([pending]);
    mocks.resolve.mockReset().mockResolvedValue({ ...pending, status: "approved" });
    mocks.getSettings.mockReset().mockResolvedValue({
        actions_kill_switch: false,
        confirm_unnamed_apps: false,
    });
    mocks.setSettings.mockReset().mockImplementation(async (settings) => settings);
});

afterEach(() => cleanup());

describe("ActionApprovalPanel", () => {
    it("shows the structured tool, target, arguments and undo before accepting a tap", async () => {
        render(<ActionApprovalPanel />);
        const card = await screen.findByRole("alertdialog", { name: "Approve action" });
        expect(card).toHaveTextContent("fndr.open_target");
        expect(card).toHaveTextContent("memory-42");
        expect(card).toHaveTextContent("memory_id");
        expect(card).toHaveTextContent("Close the opened window yourself.");
        fireEvent.click(screen.getByRole("button", { name: "Approve action" }));
        await waitFor(() => expect(mocks.resolve).toHaveBeenCalledWith("synthetic-approval", true));
    });

    it("persists the optional unnamed-app confirmation from settings", async () => {
        render(<ActionApprovalPanel embedded />);
        const toggle = await screen.findByRole("checkbox", {
            name: "Confirm before opening apps I did not name",
        });
        fireEvent.click(toggle);
        await waitFor(() => expect(mocks.setSettings).toHaveBeenCalledWith({
            actions_kill_switch: false,
            confirm_unnamed_apps: true,
        }));
    });
});
