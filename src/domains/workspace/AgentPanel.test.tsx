import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));
vi.mock("@/shared/hooks/usePolling", () => ({ usePolling: vi.fn() }));
vi.mock("@/shared/ipc/tauri", () => ({
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
