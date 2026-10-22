import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SmartFillGlow } from "./SmartFillGlow";
import { SMART_FILL_STATE_EVENT, smartFillLabel, type SmartFillStateEvent } from "./smartFillState";

let handler: ((event: { payload: SmartFillStateEvent }) => void) | null = null;

vi.mock("@tauri-apps/api/event", () => ({
    listen: vi.fn(async (name: string, callback: (event: { payload: SmartFillStateEvent }) => void) => {
        if (name === SMART_FILL_STATE_EVENT) handler = callback;
        return () => undefined;
    }),
}));

const send = async (payload: SmartFillStateEvent) => {
    await act(async () => {
        handler?.({ payload });
    });
};

describe("SmartFillGlow", () => {
    afterEach(cleanup);

    beforeEach(() => {
        handler = null;
    });

    it("draws nothing until the backend sends a phase", async () => {
        const { container } = render(<SmartFillGlow />);
        await act(async () => undefined);
        expect(container).toBeEmptyDOMElement();
    });

    it("follows the phases a request goes through", async () => {
        const { container } = render(<SmartFillGlow />);
        await act(async () => undefined);
        await send({ phase: "thinking", message: null });
        expect(screen.getByRole("status")).toHaveTextContent("Reading the screen");
        expect(container.querySelector(".sf-halo")).not.toBeNull();
        await send({ phase: "writing", message: null });
        expect(screen.getByRole("status")).toHaveTextContent("Writing");
        await send({ phase: "done", message: null });
        expect(screen.getByRole("status")).toHaveTextContent("Done");
    });

    it("shows a notice as a sentence without the caret", async () => {
        const { container } = render(<SmartFillGlow />);
        await act(async () => undefined);
        await send({ phase: "error", message: "Smart Fill never writes into a password field." });
        expect(screen.getByRole("status")).toHaveTextContent("never writes into a password field");
        expect(container.querySelector(".sf-caret")).toBeNull();
    });

    it("shows form progress and a summary when the backend sends one", async () => {
        render(<SmartFillGlow />);
        await act(async () => undefined);
        await send({ phase: "writing", message: "Filling 3 of 20" });
        expect(screen.getByRole("status")).toHaveTextContent("Filling 3 of 20");
        await send({ phase: "done", message: "Filled 18 of 34 fields" });
        expect(screen.getByRole("status")).toHaveTextContent("Filled 18 of 34 fields");
    });

    it("labels a notice without text with a safe default", () => {
        expect(smartFillLabel("notice", null)).toBe("Smart Fill could not write here.");
    });
});
