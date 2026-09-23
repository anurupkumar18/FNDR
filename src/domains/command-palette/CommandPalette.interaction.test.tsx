import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CommandPalette } from "./CommandPalette";

vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));
vi.mock("@/shared/ipc/tauri", () => ({
    deleteMemory: vi.fn(),
    pauseCapture: vi.fn(),
    resumeCapture: vi.fn(),
}));

const baseContext = {
    query: "",
    onOpenPanel: vi.fn(),
    onSearch: vi.fn(),
    onSearchApp: vi.fn(),
    onClearSearch: vi.fn(),
    onDeleteMemory: vi.fn(),
    onGoHome: vi.fn(),
};

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("CommandPalette interaction contract", () => {
    it("is a labelled modal and only offers the capture action that matches current state", () => {
        const { rerender } = render(
            <CommandPalette
                isOpen
                onClose={vi.fn()}
                selectedMemory={null}
                demoOnly
                context={{ ...baseContext, isCapturePaused: false }}
            />,
        );

        expect(screen.getByRole("dialog", { name: /fndr command palette/i })).toBeInTheDocument();
        expect(screen.getByRole("textbox", { name: /search fndr commands/i })).toBeInTheDocument();
        expect(screen.getByRole("button", { name: /pause capture/i })).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: /resume capture/i })).toBeNull();
        expect(screen.getByRole("button", { name: /privacy activity/i })).toBeInTheDocument();
        expect(
            screen.getAllByRole("button").map((button) =>
                button.querySelector(".cp-item-label")?.textContent,
            ),
        ).toEqual([
            "Home",
            "Memory Vault",
            "Search & Ask FNDR",
            "Daily Summary",
            "Stats",
            "To-dos",
            "FNDR Wrapped",
            "Screen Guide",
            "Engine diagnostics",
            "Privacy Activity",
            "Pause capture",
        ]);

        rerender(
            <CommandPalette
                isOpen
                onClose={vi.fn()}
                selectedMemory={null}
                demoOnly
                context={{ ...baseContext, isCapturePaused: true }}
            />,
        );

        expect(screen.getByRole("button", { name: /resume capture/i })).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: /pause capture/i })).toBeNull();
    });

    it("moves focus into the palette and restores the launcher after Escape", async () => {
        function Harness() {
            const [open, setOpen] = useState(false);
            return (
                <>
                    <button type="button" onClick={() => setOpen(true)}>Open commands</button>
                    <CommandPalette
                        isOpen={open}
                        onClose={() => setOpen(false)}
                        selectedMemory={null}
                        demoOnly
                        context={{ ...baseContext, isCapturePaused: false }}
                    />
                </>
            );
        }

        render(<Harness />);
        const launcher = screen.getByRole("button", { name: /open commands/i });
        launcher.focus();
        fireEvent.click(launcher);

        const input = screen.getByRole("textbox", { name: /search fndr commands/i });
        await waitFor(() => expect(input).toHaveFocus());
        fireEvent.keyDown(input, { key: "Escape" });

        await waitFor(() => expect(launcher).toHaveFocus());
        expect(screen.queryByRole("dialog", { name: /fndr command palette/i })).toBeNull();
    });
});
