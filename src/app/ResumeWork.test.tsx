import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ResumeThread } from "@/shared/ipc/tauri";
import { ResumeWork } from "./ResumeWork";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function thread(title = "Parser"): ResumeThread {
    return {
        title,
        last_state: "Fixed weekday aliases",
        age_minutes: 12,
        next_steps: [],
        suggested_next_steps: [{ title: "Add the regression test", source_memory_id: "earlier", confidence: 0.7 }],
        evidence: ["earlier", "latest"],
        pack: { items: [{ memory_id: "earlier", text: "Add the regression test", ts_ms: 1 }], dropped_for_budget: 0, estimated_tokens: 6 },
    };
}

beforeEach(() => vi.mocked(invoke).mockReset());
afterEach(cleanup);

describe("Resume Work on Home", () => {
    it("loads recent work through the existing IPC and opens exact cited memories", async () => {
        vi.mocked(invoke).mockResolvedValue([thread()]);
        const onOpenMemory = vi.fn();
        const onOpenVault = vi.fn();
        render(<ResumeWork onOpenMemory={onOpenMemory} onOpenVault={onOpenVault} />);

        expect(screen.getByRole("status")).toHaveTextContent("Loading recent work");
        expect(await screen.findByText("Fixed weekday aliases")).toBeInTheDocument();
        expect(invoke).toHaveBeenCalledWith("resume_work", { hours: 24, budgetTokens: 800 });
        expect(screen.getByText("12 min ago")).toBeInTheDocument();
        expect(screen.getByText("Suggested next step")).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "View latest source for Parser" }));
        expect(onOpenMemory).toHaveBeenLastCalledWith("latest");
        fireEvent.click(screen.getByRole("button", { name: "View source for suggested step: Add the regression test" }));
        expect(onOpenMemory).toHaveBeenLastCalledWith("earlier");
        fireEvent.click(screen.getByRole("button", { name: "Open Vault" }));
        expect(onOpenVault).toHaveBeenCalledOnce();
        expect(invoke).toHaveBeenCalledOnce();
    });

    it("shows only three threads without presenting uncited suggestions", async () => {
        const first = thread("First");
        first.suggested_next_steps[0].source_memory_id = "missing";
        vi.mocked(invoke).mockResolvedValue([first, thread("Second"), thread("Third"), thread("Fourth")]);
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);

        await screen.findByRole("heading", { name: "First" });
        expect(screen.getAllByRole("listitem")).toHaveLength(3);
        expect(screen.queryByRole("heading", { name: "Fourth" })).not.toBeInTheDocument();
        expect(screen.getAllByText("Suggested next step")).toHaveLength(2);
    });

    it("teaches the next action when no recent memories are available", async () => {
        vi.mocked(invoke).mockResolvedValue([]);
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        expect(await screen.findByText(/No recent work yet/)).toHaveTextContent("find older memories");
        expect(screen.queryByRole("list")).not.toBeInTheDocument();
    });

    it("offers retry after an error without exposing backend details", async () => {
        vi.mocked(invoke).mockRejectedValueOnce(new Error("private filesystem path"));
        vi.mocked(invoke).mockResolvedValueOnce([thread()]);
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        expect(await screen.findByRole("alert")).toHaveTextContent("Recent work couldn’t be loaded");
        expect(screen.queryByText(/private filesystem/)).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Retry" }));
        expect(await screen.findByText("Fixed weekday aliases")).toBeInTheDocument();
        expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    });

    it("refreshes from current rows and removes old citations while loading", async () => {
        vi.mocked(invoke).mockResolvedValueOnce([thread()]);
        let completeRefresh!: (rows: ResumeThread[]) => void;
        vi.mocked(invoke).mockReturnValueOnce(new Promise<ResumeThread[]>((resolve) => { completeRefresh = resolve; }));
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        await screen.findByText("Fixed weekday aliases");
        fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
        expect(screen.queryByRole("button", { name: "View latest source for Parser" })).not.toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Refresh" })).toBeDisabled();
        await act(async () => completeRefresh([]));
        expect(await screen.findByText(/No recent work yet/)).toBeInTheDocument();
    });

    it("reloads when Home reopens and ignores an older unmounted request", async () => {
        let completeOld!: (rows: ResumeThread[]) => void;
        vi.mocked(invoke).mockReturnValueOnce(new Promise<ResumeThread[]>((resolve) => { completeOld = resolve; }));
        const first = render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        first.unmount();
        vi.mocked(invoke).mockResolvedValueOnce([thread("Current")]);
        render(<ResumeWork onOpenMemory={vi.fn()} onOpenVault={vi.fn()} />);
        await screen.findByRole("heading", { name: "Current" });
        await act(async () => completeOld([thread("Stale")]));
        expect(screen.queryByRole("heading", { name: "Stale" })).not.toBeInTheDocument();
        await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
    });
});
