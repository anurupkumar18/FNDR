import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("./EngineMetricsCard", () => ({
    EngineMetricsCard: () => <section>metrics content</section>,
}));

import { EngineMetricsPanel } from "./EngineMetricsPanel";

describe("EngineMetricsPanel", () => {
    afterEach(() => cleanup());

    it("presents metrics as a focused developer diagnostic and closes with Escape", async () => {
        const onClose = vi.fn();
        render(<EngineMetricsPanel isVisible onClose={onClose} />);

        expect(
            screen.getByRole("dialog", { name: "Engine diagnostics" }),
        ).toBeInTheDocument();
        expect(screen.getByText(/developer diagnostic/i)).toBeInTheDocument();
        const close = screen.getByRole("button", { name: "Close engine diagnostics" });
        await waitFor(() => expect(close).toHaveFocus());

        fireEvent.keyDown(close, { key: "Escape" });
        expect(onClose).toHaveBeenCalledTimes(1);
    });

    it("does not render while hidden", () => {
        render(<EngineMetricsPanel isVisible={false} onClose={() => {}} />);

        expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
});
