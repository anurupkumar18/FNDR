import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { PrivacyProof, PrivacyProofPanel } from "./PrivacyProof";

vi.mock("@/shared/ipc/tauri", () => ({
    getPrivacyProof: vi.fn(),
}));

import { getPrivacyProof } from "@/shared/ipc/tauri";

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
            screen.getByRole("region", { name: /privacy activity for this app session/i }),
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
});

describe("PrivacyProofPanel", () => {
    afterEach(() => {
        vi.mocked(getPrivacyProof).mockReset();
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
