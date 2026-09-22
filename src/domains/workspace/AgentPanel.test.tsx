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
            kind: "run_read_only_command", input: { command: "git", args: ["status"] },
            status: "needs_approval", result: null,
        });
        vi.mocked(approveAgentAction).mockResolvedValue({
            id: "a1", run_id: "r1", title: "Check status", description: "Check status",
            kind: "run_read_only_command", input: { command: "git", args: ["status"] },
            status: "approved", result: null,
        });
        vi.mocked(executeAgentAction).mockResolvedValue({
            id: "a1", run_id: "r1", title: "Check status", description: "Check status",
            kind: "run_read_only_command", input: { command: "git", args: ["status"] },
            status: "succeeded",
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

    it("keeps showing the originally proposed command after the dropdown selection is changed", async () => {
        vi.mocked(proposeAgentAction).mockResolvedValue({
            id: "a2", run_id: "r2", title: "Check the compiler", description: "Check the compiler",
            kind: "run_read_only_command", input: { command: "cargo", args: ["check"] },
            status: "needs_approval", result: null,
        });

        render(<AgentPanel isVisible onClose={() => {}} />);

        fireEvent.change(screen.getByRole("combobox"), { target: { value: "act" } });
        fireEvent.change(screen.getByRole("combobox", { name: "Read-only command" }), {
            target: { value: "1" }, // cargo check
        });
        fireEvent.change(screen.getByRole("textbox", { name: "Action goal" }), {
            target: { value: "Check the compiler" },
        });
        fireEvent.click(screen.getByRole("button", { name: "Propose" }));

        await screen.findByRole("button", { name: "Approve" });
        const commandLine = screen.getByText(/Proposed command:/);
        expect(commandLine).toHaveTextContent("cargo check");

        // The command dropdown must be locked while the action is pending, and even if its
        // value is forced away programmatically, the approval card must keep showing the
        // command that was actually proposed and stored server-side, not the live dropdown.
        expect(screen.getByRole("combobox", { name: "Read-only command" })).toBeDisabled();
        fireEvent.change(screen.getByRole("combobox", { name: "Read-only command" }), {
            target: { value: "2" }, // npm run typecheck
        });

        expect(commandLine).toHaveTextContent("cargo check");
        expect(commandLine).not.toHaveTextContent("npm run typecheck");
    });
});

describe("AgentPanel propose-action visibility", () => {
    it("does not render the propose-an-action card outside act mode", () => {
        render(<AgentPanel isVisible onClose={() => {}} />);

        // Default agent mode is "ask"; the combobox is present but the action-proposal
        // form must stay hidden until the user explicitly switches to act mode.
        expect(screen.getByRole("combobox")).toHaveValue("ask");
        expect(screen.queryByText("Propose an action")).not.toBeInTheDocument();
        expect(screen.queryByRole("textbox", { name: "Action goal" })).not.toBeInTheDocument();
    });
});
