import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { PrivacyProof, PrivacyProofPanel } from "./PrivacyProof";

vi.mock("@/shared/ipc/tauri", () => ({
    getPrivacyProof: vi.fn(),
    listPeerRuns: vi.fn(),
    deleteMemoriesInRange: vi.fn(),
    exportMemoriesArchive: vi.fn(),
    importMemoriesArchive: vi.fn(),
    getEncryptionStatement: vi.fn(),
    getPrivacyExclusions: vi.fn(),
}));

import { deleteMemoriesInRange, exportMemoriesArchive, importMemoriesArchive, getEncryptionStatement, getPrivacyExclusions, getPrivacyProof, listPeerRuns } from "@/shared/ipc/tauri";

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

    it("starts an unknown reason with a capital and leaves the rest as written", () => {
        render(<PrivacyProof proof={{ ...proof, skipped_by_reason: { new_reason_kind: 2 } }} />);
        expect(screen.getByText("New reason kind")).toBeInTheDocument();
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
                        { atMs: Date.now(), feature: "daily_output_send", host: "hooks.slack.com", bytesSent: 310 },
                        {
                            atMs: Date.now(),
                            feature: "screen_guide_answer",
                            host: "chatgpt.com",
                            bytesSent: 900,
                            included: ["screen_text", "screenshot"],
                        },
                        {
                            atMs: Date.now(),
                            feature: "hand_off",
                            host: "chatgpt.com",
                            bytesSent: 52_000,
                            included: ["memory_summaries", "files"],
                        },
                        {
                            atMs: Date.now(),
                            feature: "hand_off_work",
                            host: "chatgpt.com",
                            bytesSent: 10_700,
                            included: ["command_output", "images"],
                        },
                    ],
                }}
            />,
        );
        const list = screen.getByRole("list", { name: "Cloud model requests" });
        expect(list).toHaveTextContent("Notch Do planned a request · chatgpt.com");
        expect(list).toHaveTextContent("1.3 KB");
        expect(list).toHaveTextContent("Hermes chat message");
        expect(list).toHaveTextContent("Daily output sent · hooks.slack.com");
        expect(list).toHaveTextContent("420 B");
        expect(list).toHaveTextContent(/Screen Guide asked ChatGPT · chatgpt\.com · .* · with on-screen text and a screenshot/);
        expect(list).toHaveTextContent(/Hand-off started a job · chatgpt\.com · .* · with memory summaries and attached files/);
        expect(list).toHaveTextContent(/Hand-off's worker read back its work · chatgpt\.com · .* · with command output and images it made/);
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
    beforeEach(() => {
        vi.mocked(getEncryptionStatement).mockResolvedValue({
            filevault: "unknown",
            encrypted: [],
            not_encrypted: [],
        });
        vi.mocked(getPrivacyExclusions).mockResolvedValue([]);
    });
    afterEach(() => {
        vi.mocked(getPrivacyProof).mockReset();
        vi.mocked(listPeerRuns).mockReset();
        vi.mocked(deleteMemoriesInRange).mockReset();
        vi.mocked(exportMemoriesArchive).mockReset();
        vi.mocked(importMemoriesArchive).mockReset();
        vi.mocked(getEncryptionStatement).mockReset();
        vi.mocked(getPrivacyExclusions).mockReset();
    });

    it("lists the built-in exclusions from the backend", async () => {
        vi.mocked(getPrivacyProof).mockResolvedValue(proof);
        vi.mocked(listPeerRuns).mockResolvedValue([]);
        vi.mocked(getPrivacyExclusions).mockResolvedValue([
            { id: "banking", label: "Banking, brokerage and payments", examples: "banks and payment sites" },
            { id: "password_managers", label: "Password managers and Keychain", examples: "1Password and Bitwarden" },
            { id: "private_windows", label: "Private and incognito browser windows", examples: "Safari and Chrome" },
        ]);
        render(<PrivacyProofPanel isVisible onClose={() => {}} />);

        const section = await screen.findByRole("region", { name: "Default capture exclusions" });
        expect(section).toHaveTextContent("Banking, brokerage and payments");
        expect(section).toHaveTextContent("Password managers and Keychain");
        expect(section).toHaveTextContent("Private and incognito browser windows");
    });

    it("shows FileVault status and the limited encryption scope", async () => {
        vi.mocked(getPrivacyProof).mockResolvedValue(proof);
        vi.mocked(listPeerRuns).mockResolvedValue([]);
        vi.mocked(getEncryptionStatement).mockResolvedValue({
            filevault: "on",
            encrypted: ["The few screenshots FNDR saves are sealed with a Keychain key.", "Archives saved from Privacy activity are sealed."],
            not_encrypted: ["The memory database is not encrypted by FNDR."],
        });
        render(<PrivacyProofPanel isVisible onClose={() => {}} />);

        expect(await screen.findByText("FileVault is on")).toBeInTheDocument();
        expect(screen.getByText(/few screenshots FNDR saves/)).toBeInTheDocument();
        expect(screen.getByText(/memory database is not encrypted by FNDR/)).toBeInTheDocument();
    });

    it("saves an encrypted archive and restores it through the file picker", async () => {
        vi.mocked(getPrivacyProof).mockResolvedValue(proof);
        vi.mocked(listPeerRuns).mockResolvedValue([]);
        vi.mocked(exportMemoriesArchive).mockResolvedValue({
            exported: 2,
            jsonl_path: "/synthetic/memories.jsonl.fndrenc",
            markdown_path: "/synthetic/memories.md.fndrenc",
        });
        vi.mocked(importMemoriesArchive).mockResolvedValue(2);
        render(<PrivacyProofPanel isVisible onClose={() => {}} />);

        fireEvent.click(screen.getByRole("button", { name: "Export encrypted memories" }));
        await screen.findByText("2 memories exported as encrypted JSONL and Markdown.");
        expect(exportMemoriesArchive).toHaveBeenCalledOnce();
        fireEvent.click(screen.getByRole("button", { name: "Import encrypted memories" }));
        await screen.findByText("2 memories restored.");
        expect(importMemoriesArchive).toHaveBeenCalledOnce();
    });

    it("previews a time range before enabling deletion", async () => {
        vi.mocked(getPrivacyProof).mockResolvedValue(proof);
        vi.mocked(listPeerRuns).mockResolvedValue([]);
        vi.mocked(deleteMemoriesInRange)
            .mockResolvedValueOnce({ matched: 2, deleted: 0 })
            .mockResolvedValueOnce({ matched: 2, deleted: 2 });
        render(<PrivacyProofPanel isVisible onClose={() => {}} />);

        expect(screen.getByRole("button", { name: "Delete matching memories" })).toBeDisabled();
        fireEvent.change(screen.getByLabelText("From"), { target: { value: "2026-10-14T08:00" } });
        fireEvent.change(screen.getByLabelText("Until"), { target: { value: "2026-10-15T08:00" } });
        fireEvent.click(screen.getByRole("button", { name: "Preview deletion" }));
        await screen.findByText("2 memories match this range.");
        expect(deleteMemoriesInRange).toHaveBeenCalledWith(
            new Date("2026-10-14T08:00").getTime(),
            new Date("2026-10-15T08:00").getTime(),
            true,
        );

        fireEvent.click(screen.getByRole("button", { name: "Delete 2 memories" }));
        await screen.findByText("2 memories deleted.");
        expect(deleteMemoriesInRange).toHaveBeenLastCalledWith(
            new Date("2026-10-14T08:00").getTime(),
            new Date("2026-10-15T08:00").getTime(),
            false,
        );
    });

    it("shows persisted peer egress metadata without task or memory text", async () => {
        vi.mocked(getPrivacyProof).mockResolvedValue(proof);
        vi.mocked(listPeerRuns).mockResolvedValue([{
            local_id: "local-1", peer_id: "peer-1", host: "peer.example",
            created_at_ms: Date.now(), payload_bytes: 321, status: "acknowledged",
            remote_task_id: "task-1", remote_state: "TASK_STATE_WORKING",
        }, {
            local_id: "local-2", peer_id: "peer-2", host: "other.example",
            created_at_ms: Date.now(), payload_bytes: 200, status: "direct_reply",
            remote_task_id: null, remote_state: "DIRECT_MESSAGE_UNSUPPORTED",
        }]);
        render(<PrivacyProofPanel isVisible onClose={() => {}} />);
        const runs = await screen.findByRole("list", { name: "Peer sends" });
        expect(runs).toHaveTextContent("peer.example");
        expect(runs).toHaveTextContent("321 B");
        expect(runs).toHaveTextContent("working");
        expect(runs).toHaveTextContent("reply without displayable text");
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
