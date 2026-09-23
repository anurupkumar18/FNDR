import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { PanelErrorBoundary } from "./PanelErrorBoundary";

afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
});

describe("PanelErrorBoundary", () => {
    it("offers retry and a safe way home without exposing raw runtime details", () => {
        vi.spyOn(console, "error").mockImplementation(() => undefined);
        const onClose = vi.fn();
        let shouldCrash = true;
        function TestPanel() {
            if (shouldCrash) throw new Error("sensitive internal path");
            return <p>Recovered panel</p>;
        }

        render(
            <PanelErrorBoundary panelName="Daily Summary" onClose={onClose}>
                <TestPanel />
            </PanelErrorBoundary>,
        );

        expect(screen.getByRole("alert")).toHaveTextContent(/daily summary couldn't open/i);
        expect(screen.queryByText(/sensitive internal path/i)).toBeNull();

        shouldCrash = false;
        fireEvent.click(screen.getByRole("button", { name: /try again/i }));
        expect(screen.getByText("Recovered panel")).toBeInTheDocument();

        shouldCrash = true;
        cleanup();
        render(
            <PanelErrorBoundary panelName="Stats" onClose={onClose}>
                <TestPanel />
            </PanelErrorBoundary>,
        );
        fireEvent.click(screen.getByRole("button", { name: /return home/i }));
        expect(onClose).toHaveBeenCalledTimes(1);
    });
});
