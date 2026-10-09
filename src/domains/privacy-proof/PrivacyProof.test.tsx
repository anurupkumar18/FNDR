import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { PrivacyProof, PrivacyProofPanel } from "./PrivacyProof";

vi.mock("@/shared/ipc/tauri", () => ({
    getPrivacyProof: vi.fn(),
    listPeerRuns: vi.fn(),
}));

import { getPrivacyProof, listPeerRuns } from "@/shared/ipc/tauri";

const proof = {
    evaluated: 120,
    stored: 80,
    skipped_by_reason: { blocklist: 6, perceptual_dup: 30, noise: 0 },
    egress_requests: 0,
    egress_hosts: [] as string[],
};

afterEach(cleanup);

describe("PrivacyProof", () => {
    it("shows skip counts by reason and hides reasons with zero", () => {
        render(<PrivacyProof proof={proof} />);
        expect(screen.getByText("Blocked app or site")).toBeInTheDocument();
        expect(screen.getByText("Repeated image")).toBeInTheDocument();
        expect(screen.queryByText(/perceptual dup/i)).not.toBeInTheDocument();
        expect(screen.queryByText(/^noise/i)).not.toBeInTheDocument();
    });

    it("labels process-lifetime counters as current-session activity", () => {
        render(<PrivacyProof proof={proof} />);
        expect(
            screen.getByRole("region", { name: /privacy activity details/i }),
        ).toBeInTheDocument();
        expect(screen.getByText("Frames evaluated this app session")).toBeInTheDocument();
        expect(screen.getByText("Frames stored this app session")).toBeInTheDocument();
        expect(screen.getByText(/these counts reset when FNDR closes/i)).toBeInTheDocument();
    });

    it("bounds recorded network activity without presenting it as a complete audit", () => {
        render(<PrivacyProof proof={proof} />);
        expect(screen.getByText(/0 FNDR network requests recorded this app session/i)).toBeInTheDocument();
        expect(screen.getByText(/can include model downloads and enabled integrations/i)).toBeInTheDocument();
        expect(screen.getByText(/not a complete network audit/i)).toBeInTheDocument();
    });

    it("lists the hosts when there were requests", () => {
        render(<PrivacyProof proof={{ ...proof, egress_requests: 2, egress_hosts: ["huggingface.co"] }} />);
        expect(screen.getByText(/2 FNDR network requests recorded this app session/i)).toBeInTheDocument();
        expect(screen.getByText(/huggingface\.co/)).toBeInTheDocument();
    });

    it("lists cloud model requests by feature, host and size, without content", () => {
        render(
            <PrivacyProof
                proof={{
                    ...proof,
                    model_requests: [
                        { atMs: Date.now(), feature: "notch_do_plan", host: "chatgpt.com", bytesSent: 1300 },
                        { atMs: Date.now(), feature: "hermes_chat", host: "chatgpt.com", bytesSent: 420 },
                        {
                            atMs: Date.now(),
                            feature: "screen_guide_answer",
                            host: "chatgpt.com",
                            bytesSent: 900,
                            included: ["screen_text", "screenshot"],
                        },
                    ],
                }}
            />,
        );
        const list = screen.getByRole("list", { name: "Cloud model requests" });
        expect(list).toHaveTextContent("Notch Do planned a request · chatgpt.com");
        expect(list).toHaveTextContent("1.3 KB");
        expect(list).toHaveTextContent("Hermes chat message");
        expect(list).toHaveTextContent("420 B");
        expect(list).toHaveTextContent(/Screen Guide asked ChatGPT · chatgpt\.com · .* · with on-screen text and a screenshot/);
    });
});

describe("Notch Do runs in PrivacyProof", () => {
    it("lists recent runs by what became of their actions", () => {
        render(
            <PrivacyProof
                proof={{
                    ...proof,
                    operator_runs: [
                        { runId: "r2", startedAt: new Date().toISOString(), done: 3, asked: 1, refused: 2, failed: 0 },
                        { runId: "r1", startedAt: new Date().toISOString(), done: 1, asked: 0, refused: 0, failed: 0 },
                    ],
                }}
            />,
        );
        const list = screen.getByRole("list", { name: "Notch Do runs" });
        expect(list).toHaveTextContent("3 done · 1 asked first · 2 refused");
        expect(list).toHaveTextContent("1 done");
        expect(list).not.toHaveTextContent("failed");
    });
});

describe("PrivacyProofPanel", () => {
    afterEach(() => {
        vi.mocked(getPrivacyProof).mockReset();
        vi.mocked(listPeerRuns).mockReset();
    });

    it("shows persisted peer egress metadata without task or memory text", async () => {
        vi.mocked(getPrivacyProof).mockResolvedValue(proof);
        vi.mocked(listPeerRuns).mockResolvedValue([{
            local_id: "local-1", peer_id: "peer-1", host: "peer.example",
            created_at_ms: Date.now(), payload_bytes: 321, status: "acknowledged",
            remote_task_id: "task-1", remote_state: "TASK_STATE_WORKING",
        }]);
        render(<PrivacyProofPanel isVisible onClose={() => {}} />);
        const runs = await screen.findByRole("list", { name: "Peer sends" });
        expect(runs).toHaveTextContent("peer.example");
        expect(runs).toHaveTextContent("321 B");
        expect(runs).toHaveTextContent("working");
        expect(runs).not.toHaveTextContent("PRIVATE_TASK_TEXT");
        expect(runs).not.toHaveTextContent("memory-1");
    });

    it("fetches and renders the privacy proof when opened", async () => {
        vi.mocked(getPrivacyProof).mockResolvedValue({
            evaluated: 10,
            stored: 4,
            skipped_by_reason: { blocklist: 2 },
            egress_requests: 1,
            egress_hosts: ["huggingface.co"],
        });

        render(<PrivacyProofPanel isVisible onClose={() => {}} />);

        expect(await screen.findByRole("heading", { name: "Privacy activity" })).toBeInTheDocument();
        expect(screen.getByText("Frames evaluated this app session")).toBeInTheDocument();
        expect(screen.getByText("10")).toBeInTheDocument();
        expect(screen.getByText(/1 FNDR network request recorded this app session/i)).toBeInTheDocument();
        await waitFor(() => expect(getPrivacyProof).toHaveBeenCalled());

        const trace = screen.getByRole("region", { name: "Privacy activity refresh" });
        expect(within(trace).getByText("Privacy activity refreshed")).toBeInTheDocument();
        fireEvent.click(within(trace).getByRole("button", { name: "Show Privacy activity refresh details" }));
        expect(within(trace).getByText(/10 evaluated · 4 stored · 2 not stored · 1 recorded request/i)).toBeInTheDocument();
        expect(within(trace).queryByText("Running")).not.toBeInTheDocument();
        expect(within(trace).queryByText(/huggingface\.co/i)).not.toBeInTheDocument();
    });

    it("shows the real request while privacy counters are loading", async () => {
        vi.mocked(getPrivacyProof).mockReturnValue(new Promise(() => {}));

        render(<PrivacyProofPanel isVisible onClose={() => {}} />);

        const trace = await screen.findByRole("region", { name: "Privacy activity refresh" });
        expect(within(trace).getByText("Requesting privacy activity")).toBeInTheDocument();
        expect(within(trace).getByText("Running")).toBeInTheDocument();
    });

    it("renders a bounded refresh failure without exposing the native error", async () => {
        vi.mocked(getPrivacyProof).mockRejectedValue(
            new Error("/Users/person/private.db via private.example"),
        );

        render(<PrivacyProofPanel isVisible onClose={() => {}} />);

        expect(await screen.findByRole("alert")).toHaveTextContent(
            "Privacy activity could not be refreshed",
        );
        const trace = screen.getByRole("region", { name: "Privacy activity refresh" });
        expect(within(trace).getByText("Privacy activity refresh failed")).toBeInTheDocument();
        expect(within(trace).getByText("Failed")).toBeInTheDocument();
        expect(screen.queryByText(/private\.db|private\.example/)).not.toBeInTheDocument();
    });

    it("owns modal focus, closes on Escape, traps focus, and restores the invoking control", async () => {
        vi.mocked(getPrivacyProof).mockResolvedValue(proof);
        const onClose = vi.fn();
        const panel = (visible: boolean) => (
            <>
                <button type="button">Privacy activity launcher</button>
                <PrivacyProofPanel isVisible={visible} onClose={onClose} />
            </>
        );
        const { rerender } = render(panel(false));
        const launcher = screen.getByRole("button", { name: "Privacy activity launcher" });
        launcher.focus();

        rerender(panel(true));
        const dialog = screen.getByRole("dialog", { name: "Privacy activity" });
        const closeButton = screen.getByRole("button", { name: "Close privacy activity" });
        expect(dialog).toHaveAttribute("aria-modal", "true");
        await waitFor(() => expect(closeButton).toHaveFocus());

        fireEvent.keyDown(closeButton, { key: "Tab" });
        expect(closeButton).toHaveFocus();
        fireEvent.keyDown(closeButton, { key: "Escape" });
        expect(onClose).toHaveBeenCalledTimes(1);

        rerender(panel(false));
        await waitFor(() => expect(launcher).toHaveFocus());
    });

    it("renders nothing when not visible", () => {
        render(<PrivacyProofPanel isVisible={false} onClose={() => {}} />);
        expect(screen.queryByText(/Privacy activity/i)).not.toBeInTheDocument();
        expect(getPrivacyProof).not.toHaveBeenCalled();
    });
});
