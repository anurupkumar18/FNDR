import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { OpenSourceButton } from "./OpenSourceButton";

const mocks = vi.hoisted(() => ({
    reopenMemory: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ({
    reopenMemory: mocks.reopenMemory,
}));

afterEach(() => {
    cleanup();
    mocks.reopenMemory.mockReset();
});

describe("OpenSourceButton", () => {
    it("calls reopenMemory and shows the typed outcome message", async () => {
        mocks.reopenMemory.mockResolvedValue({ kind: "missing", path: "/tmp/a.pdf" });

        render(<OpenSourceButton memoryId="memory-1" />);
        fireEvent.click(screen.getByRole("button", { name: "Open source" }));

        await waitFor(() => expect(mocks.reopenMemory).toHaveBeenCalledWith("memory-1"));
        expect(screen.getByRole("status")).toHaveTextContent("no longer exists");
    });

    it("does not trigger a parent row click handler", async () => {
        mocks.reopenMemory.mockResolvedValue({ kind: "opened" });
        const onRowClick = vi.fn();

        render(
            <div onClick={onRowClick}>
                <OpenSourceButton memoryId="memory-2" />
            </div>,
        );
        fireEvent.click(screen.getByRole("button", { name: "Open source" }));

        await waitFor(() => expect(mocks.reopenMemory).toHaveBeenCalled());
        expect(onRowClick).not.toHaveBeenCalled();
    });

    it("forwards outcomes to a parent handler when onOutcome is set", async () => {
        mocks.reopenMemory.mockResolvedValue({ kind: "app_only", app_name: "Preview" });
        const onOutcome = vi.fn();

        render(<OpenSourceButton memoryId="memory-3" onOutcome={onOutcome} />);
        fireEvent.click(screen.getByRole("button", { name: "Open source" }));

        await waitFor(() => expect(onOutcome).toHaveBeenCalledWith(expect.stringContaining("Preview"), "app_only"));
        expect(screen.queryByRole("status")).not.toBeInTheDocument();
    });
});
