import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { PrivacyProof } from "./PrivacyProof";

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

    it("states the egress count plainly", () => {
        render(<PrivacyProof proof={proof} />);
        expect(screen.getByText(/0 network requests/i)).toBeInTheDocument();
    });

    it("lists the hosts when there were requests", () => {
        render(<PrivacyProof proof={{ ...proof, egress_requests: 2, egress_hosts: ["huggingface.co"] }} />);
        expect(screen.getByText(/2 network requests/i)).toBeInTheDocument();
        expect(screen.getByText(/huggingface\.co/)).toBeInTheDocument();
    });
});
