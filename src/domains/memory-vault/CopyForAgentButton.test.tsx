import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { fndrBuildContextPack, type MemoryCard } from "@/shared/ipc/tauri";
import { CopyForAgentButton } from "./CopyForAgentButton";

vi.mock("@/shared/ipc/tauri", () => ({
    fndrBuildContextPack: vi.fn(),
}));

const card: MemoryCard = {
    id: "memory-selected",
    title: "Selected design review",
    summary: "Reviewed the hierarchy.",
    insight_why_mattered: "The next action became obvious.",
    action: "Reviewed design",
    context: [],
    timestamp: new Date("2026-09-22T22:00:00Z").getTime(),
    app_name: "Figma",
    window_title: "FNDR Home",
    project: "FNDR",
    score: 1,
    source_count: 1,
    raw_snippets: [],
};

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("CopyForAgentButton", () => {
    it("anchors the copied payload to the selected memory before related context", async () => {
        vi.mocked(fndrBuildContextPack).mockResolvedValue({
            summary: "Two related local memories were selected.",
            relevant_files: [{ path: "src/app/HomeHero.tsx" }],
        });
        const writeText = vi.fn().mockResolvedValue(undefined);
        Object.defineProperty(navigator, "clipboard", {
            configurable: true,
            value: { writeText },
        });

        render(<CopyForAgentButton card={card} />);
        fireEvent.click(
            screen.getByRole("button", { name: "Copy this memory and related context" }),
        );

        await waitFor(() => {
            expect(fndrBuildContextPack).toHaveBeenCalledWith({
                query: "Selected design review",
                project: "FNDR",
            });
            expect(writeText).toHaveBeenCalledTimes(1);
        });
        const copied = vi.mocked(writeText).mock.calls[0][0] as string;
        expect(copied).toContain("# FNDR memory: Selected design review");
        expect(copied).toContain("Memory ID: memory-selected");
        expect(copied).toContain("## Why it mattered");
        expect(copied).toContain("## Related FNDR context");
        expect(screen.getByText("Copied!")).toBeInTheDocument();
    });
});
