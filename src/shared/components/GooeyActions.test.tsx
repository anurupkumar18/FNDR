import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GooeyActions } from "./GooeyActions";

const motion = vi.hoisted(() => ({ reduced: false }));

vi.mock("@/shared/motion/useReducedMotionSafe", () => ({
    useReducedMotionSafe: () => ({ reduced: motion.reduced }),
}));

function renderPair(visible: boolean, onSecondary = vi.fn(), onPrimary = vi.fn()) {
    return render(
        <GooeyActions
            primary={<button type="button" onClick={onPrimary}>Speak</button>}
            secondary={<button type="button" aria-label="Submit" onClick={onSecondary}>Go</button>}
            secondaryVisible={visible}
            slotPx={52}
        />,
    );
}

describe("GooeyActions", () => {
    afterEach(() => {
        cleanup();
        motion.reduced = false;
    });

    it("keeps both buttons live on one liquid surface while the secondary is visible", () => {
        const onSecondary = vi.fn();
        const onPrimary = vi.fn();
        const { container } = renderPair(true, onSecondary, onPrimary);

        expect(container.querySelector('[data-gooey="on"]')).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Submit" }));
        fireEvent.click(screen.getByRole("button", { name: "Speak" }));
        expect(onSecondary).toHaveBeenCalledOnce();
        expect(onPrimary).toHaveBeenCalledOnce();
    });

    it("parks the secondary out of the accessibility tree and focus order until it is needed", () => {
        const { container } = renderPair(false);

        expect(screen.queryByRole("button", { name: "Submit" })).not.toBeInTheDocument();
        expect(container.querySelector("[inert][aria-hidden='true']")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Speak" })).toBeEnabled();
    });

    it("renders no goo under reduced motion and simply shows or hides the secondary", () => {
        motion.reduced = true;
        const { container, rerender } = renderPair(false);

        expect(container.querySelector('[data-gooey="off"]')).toBeInTheDocument();
        expect(container.querySelector("svg")).not.toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Submit" })).not.toBeInTheDocument();

        rerender(
            <GooeyActions
                primary={<button type="button">Speak</button>}
                secondary={<button type="button" aria-label="Submit">Go</button>}
                secondaryVisible
                slotPx={52}
            />,
        );
        expect(screen.getByRole("button", { name: "Submit" })).toBeInTheDocument();
    });
});
