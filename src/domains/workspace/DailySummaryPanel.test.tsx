import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { DailyNarrative } from "@/shared/ipc/tauri";
import { DailySummaryPanel } from "./DailySummaryPanel";

/** A fictional day for Sam Example. */
function narrativeFixture(overrides: Partial<DailyNarrative> = {}): DailyNarrative {
    return {
        date: "2026-10-21",
        mode: "narrative",
        backend: "apple",
        basic_reason: null,
        paragraphs: [
            { text: "Spent the afternoon in Xcode on the Payments project.", memory_ids: ["m1", "m2"] },
            { text: "Read a message from Priya Shah about the Q3 budget draft.", memory_ids: ["m4"] },
        ],
        action_items: [
            { id: "action_item-a", text: "Send the Q3 budget draft to Priya Shah", memory_ids: ["m4"], task_id: "task-m4", due_label: null },
            { id: "action_item-b", text: "Book the dentist", memory_ids: ["m6"], task_id: null, due_label: "Fri, Oct 23" },
        ],
        reminders: [
            { id: "reminder-a", text: "Renew the parking permit", memory_ids: ["m7"], task_id: null, due_label: "Thu, Oct 22" },
        ],
        groups_used: 3,
        items_dropped: 1,
        model_ms: 2100,
        ...overrides,
    };
}

const ipc = vi.hoisted(() => ({
    addTodo: vi.fn(),
    exportDailySummaryPdf: vi.fn(),
    generateDailySummaryForDate: vi.fn(),
    generateDailySummaryNarrative: vi.fn(),
    getDailySummaryFollowups: vi.fn(),
    openExportedPdf: vi.fn(),
    setTodoCompleted: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ipc);

beforeEach(() => {
    vi.clearAllMocks();
    ipc.generateDailySummaryForDate.mockImplementation(async (date: string) => `Summary for ${date}`);
    ipc.generateDailySummaryNarrative.mockResolvedValue(narrativeFixture());
    ipc.getDailySummaryFollowups.mockResolvedValue([]);
    ipc.setTodoCompleted.mockResolvedValue(true);
    window.localStorage.clear();
});

afterEach(cleanup);

describe("DailySummaryPanel", () => {
    it("traces the observed summary request and verified result", async () => {
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);

        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        const trace = await screen.findByLabelText("Daily summary activity");
        expect(await within(trace).findByText("Daily summary ready")).toBeInTheDocument();
        fireEvent.click(within(trace).getByRole("button", { name: "Show Daily summary activity details" }));
        expect(within(trace).getByText(/verified result/i)).toBeInTheDocument();
        expect(within(trace).queryByText(/summary for 20/i)).toBeNull();
    });

    it("clears content tied to the previous date when the selected date changes", async () => {
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);

        const dateInput = screen.getByLabelText(/select summary date/i) as HTMLInputElement;
        const initialDate = dateInput.value;
        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));
        expect(await screen.findByText(`Summary for ${initialDate}`)).toBeInTheDocument();
        expect(screen.getByRole("button", { name: /download pdf/i })).toBeInTheDocument();

        fireEvent.change(dateInput, {
            target: { value: "2020-01-02" },
        });

        expect(screen.queryByText(`Summary for ${initialDate}`)).toBeNull();
        expect(screen.queryByRole("button", { name: /download pdf/i })).toBeNull();
        expect(screen.getByText(/generate a summary for thursday, january 2, 2020/i)).toBeInTheDocument();
    });

    it("describes follow-ups as current work instead of claiming they came from the selected day", async () => {
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);

        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        expect(await screen.findByText(/no open tasks right now/i)).toBeInTheDocument();
        expect(screen.queryByText(/no open follow-ups for this day/i)).toBeNull();
    });

    it("traps focus, closes on Escape, and restores the invoking control", async () => {
        const invoker = document.createElement("button");
        invoker.textContent = "Open Daily Summary";
        document.body.appendChild(invoker);
        invoker.focus();
        const onClose = vi.fn();
        const openMemory = vi.fn();
        const { rerender } = render(
            <DailySummaryPanel isVisible onClose={onClose} onOpenMemoryById={openMemory} />,
        );

        const dialog = screen.getByRole("dialog", { name: /daily summary/i });
        const closeButton = screen.getByRole("button", { name: /close daily summary/i });
        expect(dialog).toHaveAttribute("aria-modal", "true");
        await waitFor(() => expect(closeButton).toHaveFocus());
        await waitFor(() => {
            expect((screen.getByLabelText(/select summary date/i) as HTMLInputElement).value).not.toBe("");
        });

        const focusable = dialog.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled])");
        focusable[focusable.length - 1].focus();
        fireEvent.keyDown(document, { key: "Tab" });
        expect(closeButton).toHaveFocus();

        fireEvent.keyDown(document, { key: "Escape" });
        expect(onClose).toHaveBeenCalledOnce();
        rerender(<DailySummaryPanel isVisible={false} onClose={onClose} onOpenMemoryById={openMemory} />);
        expect(invoker).toHaveFocus();
        invoker.remove();
    });

    it("shows a loading state while the local model writes, then the paragraphs", async () => {
        let resolve: (value: DailyNarrative) => void = () => undefined;
        ipc.generateDailySummaryNarrative.mockReturnValue(new Promise<DailyNarrative>((done) => { resolve = done; }));
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);

        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        expect(await screen.findByText("Writing your day")).toBeInTheDocument();
        expect(screen.queryByText(/spent the afternoon in xcode/i)).toBeNull();
        expect(screen.getByRole("button", { name: /download pdf/i })).toBeDisabled();

        resolve(narrativeFixture());
        expect(await screen.findByText(/spent the afternoon in xcode/i)).toBeInTheDocument();
        expect(screen.queryByText("Writing your day")).toBeNull();
        expect(screen.getByText(/written on this mac by apple's on-device model/i)).toBeInTheDocument();
        expect(screen.queryByText("Basic summary")).toBeNull();
    });

    it("orders Your day, Action items, Reminders and a closed All activity", async () => {
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        const day = await screen.findByRole("heading", { name: "Your day" });
        const actions = screen.getByRole("heading", { name: "Action items" });
        const reminders = screen.getByRole("heading", { name: "Reminders" });
        const activity = screen.getByText("All activity");
        const follows = (a: Node, b: Node) => Boolean(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);
        expect(follows(day, actions)).toBe(true);
        expect(follows(actions, reminders)).toBe(true);
        expect(follows(reminders, activity)).toBe(true);
        const disclosure = activity.closest("details");
        expect(disclosure).not.toBeNull();
        expect(disclosure).not.toHaveAttribute("open");
        expect(disclosure).toContainElement(screen.getByText(/no open tasks right now/i));
    });

    it("opens the memory behind a paragraph or an item", async () => {
        const openMemory = vi.fn();
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={openMemory} />);
        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        fireEvent.click(await screen.findByRole("button", { name: /open the memory behind: spent the afternoon in xcode/i }));
        expect(openMemory).toHaveBeenLastCalledWith("m1");
        fireEvent.click(screen.getByRole("button", { name: /open the memory behind: send the q3 budget draft/i }));
        expect(openMemory).toHaveBeenLastCalledWith("m4");
    });

    it("checks an action item, completing its task when it has one", async () => {
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        const withTask = await screen.findByRole("checkbox", { name: /mark done: send the q3 budget draft/i });
        fireEvent.click(withTask);
        await waitFor(() => expect(ipc.setTodoCompleted).toHaveBeenCalledWith("task-m4", true));
        expect(screen.getByRole("checkbox", { name: /undo: send the q3 budget draft/i })).toBeChecked();

        ipc.setTodoCompleted.mockClear();
        fireEvent.click(screen.getByRole("checkbox", { name: /mark done: book the dentist/i }));
        expect(ipc.setTodoCompleted).not.toHaveBeenCalled();
        expect(screen.getByRole("checkbox", { name: /undo: book the dentist/i })).toBeChecked();
        expect(screen.getByText("Fri, Oct 23")).toBeInTheDocument();
    });

    it("puts a checked item back when its task cannot be updated", async () => {
        ipc.setTodoCompleted.mockRejectedValue(new Error("Store is busy"));
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        fireEvent.click(await screen.findByRole("checkbox", { name: /mark done: send the q3 budget draft/i }));
        expect(await screen.findByText("Store is busy")).toBeInTheDocument();
        expect(screen.getByRole("checkbox", { name: /mark done: send the q3 budget draft/i })).not.toBeChecked();
    });

    it("labels the basic summary and says why the model was not used", async () => {
        ipc.generateDailySummaryNarrative.mockResolvedValue(narrativeFixture({
            mode: "basic",
            backend: "none",
            basic_reason: "the on-device model did not answer",
            action_items: [],
            reminders: [],
        }));
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        expect(await screen.findByText("Basic summary")).toBeInTheDocument();
        expect(screen.getByText(/composed from your captures without a model: the on-device model did not answer/i)).toBeInTheDocument();
        expect(screen.queryByRole("heading", { name: "Action items" })).toBeNull();
        expect(screen.queryByRole("heading", { name: "Reminders" })).toBeNull();
    });

    it("shows an error for the narrative and keeps the activity list reachable", async () => {
        ipc.generateDailySummaryNarrative.mockRejectedValue(new Error("Model crashed"));
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        expect(await screen.findByText("Your day could not be written")).toBeInTheDocument();
        expect(screen.getByText("Model crashed")).toBeInTheDocument();
        expect(screen.getByText("All activity")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: /download pdf/i })).toBeEnabled();
    });

    it("says so when nothing was recorded", async () => {
        ipc.generateDailySummaryNarrative.mockResolvedValue(narrativeFixture({
            mode: "empty", backend: "none", paragraphs: [], action_items: [], reminders: [], groups_used: 0,
        }));
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        expect(await screen.findByText("No memories were recorded for this date.")).toBeInTheDocument();
    });

    it("includes the narrative sections in the exported PDF", async () => {
        ipc.exportDailySummaryPdf.mockResolvedValue("/Users/sam/Downloads/FNDR_Daily_Summary.pdf");
        render(<DailySummaryPanel isVisible onClose={vi.fn()} onOpenMemoryById={vi.fn()} />);
        const dateInput = screen.getByLabelText(/select summary date/i) as HTMLInputElement;
        fireEvent.click(screen.getByRole("button", { name: /generate summary/i }));

        fireEvent.click(await screen.findByRole("button", { name: /download pdf/i }));
        await waitFor(() => expect(ipc.exportDailySummaryPdf).toHaveBeenCalledTimes(1));
        expect(ipc.exportDailySummaryPdf).toHaveBeenCalledWith(
            dateInput.value,
            `Summary for ${dateInput.value}`,
            {
                basic: false,
                paragraphs: [
                    "Spent the afternoon in Xcode on the Payments project.",
                    "Read a message from Priya Shah about the Q3 budget draft.",
                ],
                action_items: ["Send the Q3 budget draft to Priya Shah", "Book the dentist"],
                reminders: ["Renew the parking permit"],
            },
        );
    });
});
