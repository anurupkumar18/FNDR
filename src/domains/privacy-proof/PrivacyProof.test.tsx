import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
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
        expect(screen.getByText(/blocklist/i)).toBeInTheDocument();
        expect(screen.getByText(/perceptual dup/i)).toBeInTheDocument();
        expect(screen.queryByText(/^noise/i)).not.toBeInTheDocument();
    });

    it("states the direct egress count without overclaiming total network use", () => {
        render(<PrivacyProof proof={proof} />);
        expect(screen.getByText(/0 direct network requests from FNDR/i)).toBeInTheDocument();
        expect(screen.getByText(/does not include model downloads/i)).toBeInTheDocument();
    });

    it("lists the hosts when there were requests", () => {
        render(<PrivacyProof proof={{ ...proof, egress_requests: 2, egress_hosts: ["huggingface.co"] }} />);
        expect(screen.getByText(/2 direct network requests from FNDR/i)).toBeInTheDocument();
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

        expect(await screen.findByText(/10 frames evaluated/i)).toBeInTheDocument();
        await waitFor(() => expect(getPrivacyProof).toHaveBeenCalled());
    });

    it("renders nothing when not visible", () => {
        render(<PrivacyProofPanel isVisible={false} onClose={() => {}} />);
        expect(screen.queryByText(/Privacy proof/i)).not.toBeInTheDocument();
        expect(getPrivacyProof).not.toHaveBeenCalled();
    });
});
