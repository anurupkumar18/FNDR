import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { AskPanel } from "./AskPanel";
import { fndrAnswer, type ComposedAnswer } from "@/shared/ipc/tauri";

vi.mock("@/shared/ipc/tauri", () => ({ fndrAnswer: vi.fn() }));
// The run itself is covered in Investigation.test.tsx; here only the wiring.
vi.mock("./Investigation", () => ({
    Investigation: ({ goal, onDismiss }: { goal: string; onDismiss: () => void }) => (
        <div data-testid="investigation">
            <span>{goal}</span>
            <button type="button" onClick={onDismiss}>Dismiss run</button>
        </div>
    ),
}));

const grounded: ComposedAnswer = {
    query: "what was the spam bug",
    answer: "LL-1482: reminder emails landed in spam.",
    evidence: { files: [], commands: [], decisions: [], errors: [], todos: [], urls: [] },
    cards: [],
    verify_outcome: { kind: "grounded", confidence: 0.8 },
    surfacing_reasons: [],
};

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

function open() {
    render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);
    return screen.getByRole("textbox", { name: "Search or ask FNDR" });
}

describe("AskPanel and Investigate", () => {
    it("starts an investigation of what was typed, and returns to the question box", () => {
        const input = open();
        const investigate = screen.getByRole("button", { name: "Investigate" });
        expect(investigate).toBeDisabled();

        fireEvent.change(input, { target: { value: "  the Smart Reminders launch " } });
        fireEvent.click(investigate);

        expect(screen.getByTestId("investigation")).toHaveTextContent("the Smart Reminders launch");
        expect(fndrAnswer).not.toHaveBeenCalled();
        // The examples give way to the run.
        expect(screen.queryByLabelText("Example questions")).not.toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Dismiss run" }));
        expect(screen.queryByTestId("investigation")).not.toBeInTheDocument();
        expect(screen.getByLabelText("Example questions")).toBeInTheDocument();
    });

    it("offers to investigate an answered question further", async () => {
        vi.mocked(fndrAnswer).mockResolvedValue(grounded);
        const input = open();
        fireEvent.change(input, { target: { value: "what was the spam bug" } });
        fireEvent.click(screen.getByRole("button", { name: "Search & Ask" }));

        fireEvent.click(await screen.findByRole("button", { name: "Investigate this further" }));

        expect(screen.getByTestId("investigation")).toHaveTextContent("what was the spam bug");
        // The answer steps aside for the run.
        expect(screen.queryByText(grounded.answer)).not.toBeInTheDocument();
    });

    it("asking again replaces a run with the answer", async () => {
        vi.mocked(fndrAnswer).mockResolvedValue(grounded);
        const input = open();
        fireEvent.change(input, { target: { value: "the launch" } });
        fireEvent.click(screen.getByRole("button", { name: "Investigate" }));
        expect(screen.getByTestId("investigation")).toBeInTheDocument();

        fireEvent.click(screen.getByRole("button", { name: "Search & Ask" }));
        expect(await screen.findByText(grounded.answer)).toBeInTheDocument();
        expect(screen.queryByTestId("investigation")).not.toBeInTheDocument();
    });
});
