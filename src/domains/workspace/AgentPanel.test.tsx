import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));
vi.mock("@/shared/hooks/usePolling", () => ({ usePolling: vi.fn() }));
vi.mock("@/shared/ipc/tauri", () => ({
    approveAgentAction: vi.fn(),
    executeAgentAction: vi.fn(),
    fndrSubscribe: vi.fn().mockResolvedValue(true),
    fndrUnsubscribe: vi.fn().mockResolvedValue(true),
    getAgentStatus: vi.fn(),
    getContextRuntimeStatus: vi.fn(),
    getHermesBridgeStatus: vi.fn(),
    getAgentAuditRun: vi.fn(),
    listAgentAuditRuns: vi.fn(),
    listRecentContextPacks: vi.fn(),
    explainAgentRetrieval: vi.fn(),
    installHermesBridge: vi.fn(),
    onContextDelta: vi.fn().mockResolvedValue(() => {}),
    proposeAgentAction: vi.fn(),
    proposeEvalFromRun: vi.fn(),
    proposeSkillFromRun: vi.fn(),
    quickSetupOllama: vi.fn(),
    rateAgentResult: vi.fn(),
    runAgentRequest: vi.fn(),
    saveHermesSetup: vi.fn(),
    sendDirectChat: vi.fn(),
    sendHermesMessage: vi.fn(),
    startHermesGateway: vi.fn(),
    stopAgent: vi.fn(),
    stopHermesGateway: vi.fn(),
    syncHermesBridgeContext: vi.fn(),
}));

import { approveAgentAction, executeAgentAction, proposeAgentAction } from "@/shared/ipc/tauri";
import { AgentPanel } from "./AgentPanel";

afterEach(cleanup);

describe("AgentPanel context mode", () => {
    it("keeps the alpha surface local and hides provider configuration", () => {
        render(<AgentPanel isVisible onClose={vi.fn()} mode="context" />);

        expect(screen.getByText("Context")).toBeInTheDocument();
        expect(screen.getByText("Build local context")).toBeInTheDocument();
        expect(screen.getByRole("textbox", { name: "Context question" })).toBeInTheDocument();
        expect(screen.queryByText("Configure provider")).not.toBeInTheDocument();
        expect(screen.queryByText("FNDR Agent")).not.toBeInTheDocument();
        expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    });
});

describe("AgentPanel act mode actions", () => {
    it("proposes, approves, and executes a read-only command action in act mode", async () => {
        vi.mocked(proposeAgentAction).mockResolvedValue({
            id: "a1", run_id: "r1", title: "Check status", description: "Check status",
            kind: "run_read_only_command", status: "needs_approval", result: null,
        });
        vi.mocked(approveAgentAction).mockResolvedValue({
            id: "a1", run_id: "r1", title: "Check status", description: "Check status",
            kind: "run_read_only_command", status: "approved", result: null,
        });
        vi.mocked(executeAgentAction).mockResolvedValue({
            id: "a1", run_id: "r1", title: "Check status", description: "Check status",
            kind: "run_read_only_command", status: "succeeded",
            result: { success: true, output: "On branch main", error: null, duration_ms: 42 },
        });

        render(<AgentPanel isVisible onClose={() => {}} />);

        // Only the agent-mode selector exists until act mode is selected.
        fireEvent.change(screen.getByRole("combobox"), { target: { value: "act" } });

        fireEvent.change(screen.getByRole("textbox", { name: "Action goal" }), {
            target: { value: "Check status" },
        });
        fireEvent.click(screen.getByRole("button", { name: "Propose" }));

        const approveButton = await screen.findByRole("button", { name: "Approve" });
        expect(screen.getByRole("button", { name: "Run" })).toBeDisabled();

        fireEvent.click(approveButton);

        await waitFor(() => expect(screen.getByRole("button", { name: "Run" })).toBeEnabled());

        fireEvent.click(screen.getByRole("button", { name: "Run" }));

        expect(await screen.findByText("On branch main")).toBeInTheDocument();
        expect(proposeAgentAction).toHaveBeenCalledWith(
            expect.any(String),
            "run_read_only_command",
            "medium",
            "Check status",
            { command: "git", args: ["status"] },
        );
        expect(approveAgentAction).toHaveBeenCalledWith("a1");
        expect(executeAgentAction).toHaveBeenCalledWith("a1");
    });
});
