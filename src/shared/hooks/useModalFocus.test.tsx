import { useRef } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useModalFocus } from "./useModalFocus";

afterEach(cleanup);

describe("useModalFocus", () => {
    it("lets a modal layered above the panel own Escape", async () => {
        const onClose = vi.fn();

        function Harness() {
            const dialogRef = useRef<HTMLDivElement>(null);
            const closeRef = useRef<HTMLButtonElement>(null);
            useModalFocus(true, dialogRef, closeRef, onClose);
            return (
                <div ref={dialogRef} role="dialog" aria-modal="true" aria-label="Parent panel">
                    <button ref={closeRef} type="button">Close parent</button>
                    <div role="dialog" aria-modal="true" aria-label="Layered modal">
                        <button type="button">Close layered modal</button>
                    </div>
                </div>
            );
        }

        render(<Harness />);
        await waitFor(() => expect(screen.getByRole("button", { name: "Close parent" })).toHaveFocus());

        const layeredButton = screen.getByRole("button", { name: "Close layered modal" });
        layeredButton.focus();
        fireEvent.keyDown(layeredButton, { key: "Escape" });
        expect(onClose).not.toHaveBeenCalled();

        const parentButton = screen.getByRole("button", { name: "Close parent" });
        parentButton.focus();
        fireEvent.keyDown(parentButton, { key: "Escape" });
        expect(onClose).toHaveBeenCalledTimes(1);
    });
});
