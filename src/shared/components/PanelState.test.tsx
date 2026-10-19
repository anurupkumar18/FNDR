import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { PanelState } from "./PanelState";

describe("PanelState", () => {
    it("announces loading without offering an action that cannot run yet", () => {
        render(
            <PanelState
                kind="loading"
                title="Loading memories"
                description="Checking this Mac for saved memories."
            />,
        );

        expect(screen.getByRole("status")).toHaveTextContent("Loading memories");
        expect(screen.queryByRole("button")).toBeNull();
    });

    it("gives an empty panel its next action", () => {
        const onAction = vi.fn();
        render(
            <PanelState
                kind="empty"
                title="No memories yet"
                description="New memories will appear after capture runs."
                action={{ label: "Open Screen Guide", onAction }}
            />,
        );

        fireEvent.click(screen.getByRole("button", { name: "Open Screen Guide" }));
        expect(onAction).toHaveBeenCalledOnce();
    });

    it("makes a recoverable error an alert with retry", () => {
        const onRetry = vi.fn();
        render(
            <PanelState
                kind="error"
                title="Memories could not load"
                description="Try again in a moment."
                action={{ label: "Try again", onAction: onRetry }}
            />,
        );

        expect(screen.getByRole("alert")).toHaveTextContent("Memories could not load");
        fireEvent.click(screen.getByRole("button", { name: "Try again" }));
        expect(onRetry).toHaveBeenCalledOnce();
    });
});
