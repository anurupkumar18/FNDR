import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { HandOffDraft, HandOffRecord, HandOffStatus } from "@/shared/ipc/tauri";
import { HandOffPanel } from "./HandOffPanel";

const ipc = vi.hoisted(() => ({
    handOffStatus: vi.fn(),
    handOffDraft: vi.fn(),
    handOffUpdateDraft: vi.fn(),
    handOffInstallToolchain: vi.fn(),
    handOffStart: vi.fn(),
    handOffStop: vi.fn(),
    handOffOpen: vi.fn(),
    handOffRate: vi.fn(),
}));
vi.mock("@/shared/ipc/tauri", async (importOriginal) => ({
    ...(await importOriginal<typeof import("@/shared/ipc/tauri")>()),
    ...ipc,
}));

const events = vi.hoisted(() => new Map<string, (payload: unknown) => void>());
vi.mock("@/shared/hooks/useTauriEvent", () => ({
    useTauriEvent: (name: string, handler: (payload: unknown) => void) => {
        events.set(name, handler);
    },
}));

function emit(name: string, payload: unknown) {
    act(() => events.get(name)?.(payload));
}

const draft: HandOffDraft = {
    taskText: "Own a one-page Q4 onboarding survey readout",
    taskMemoryId: "m_mail",
    goal: "doc",
    items: [
        { kind: "memory", memoryId: "m_mail", label: "Mail: Q4 readout", appName: "Mail", capturedAt: 1, ticked: true },
        { kind: "memory", memoryId: "m_notes", label: "Notes: churn guesses", appName: "Notes", capturedAt: 1, ticked: true },
        { kind: "file", memoryId: "m_csv", label: "q4-survey.csv", appName: "Finder", capturedAt: 1, ticked: true, bytes: 15_184 },
    ],
    rendered: "TASK\n\"Own a one-page Q4 onboarding survey readout\"",
    estimateBytes: 16_127,
};

const installed: HandOffStatus = { toolchain: "installed", running: null };

const record: HandOffRecord = {
    id: "job-1",
    createdAtMs: 1,
    request: draft.taskText,
    goal: "doc",
    memoryIds: ["m_mail", "m_notes"],
    fileMemoryIds: ["m_csv"],
    folder: "/Users/sam/Documents/FNDR Hand-offs/2026-10-16 Q4 readout",
    status: "ready",
    reason: null,
    finishedAtMs: 2,
    bytesSent: 62_156,
    checked: 3,
    unsupported: 0,
    notChecked: 0,
    deliverables: ["out/q4-readout.docx"],
    rating: null,
};

const source = { kind: "task", value: "task-1" } as const;

beforeEach(() => {
    events.clear();
    ipc.handOffStatus.mockResolvedValue(installed);
    ipc.handOffDraft.mockResolvedValue(draft);
    ipc.handOffUpdateDraft.mockImplementation(async (choice: { memoryIds: string[]; fileMemoryIds: string[] }) => ({
        ...draft,
        items: draft.items.map((item) => ({
            ...item,
            ticked: (item.kind === "memory" ? choice.memoryIds : choice.fileMemoryIds).includes(item.memoryId),
        })),
    }));
    ipc.handOffStart.mockResolvedValue(undefined);
    ipc.handOffStop.mockResolvedValue(undefined);
    ipc.handOffOpen.mockResolvedValue(undefined);
    ipc.handOffRate.mockResolvedValue({ ...record, rating: "worked" });
});

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("HandOffPanel", () => {
    it("shows the draft and hands off exactly what was reviewed", async () => {
        render(<HandOffPanel source={source} onClose={() => {}} />);
        expect(await screen.findByText(/Own a one-page Q4 onboarding survey readout/)).toBeInTheDocument();
        const list = screen.getByRole("list", { name: "What would be sent" });
        expect(within(list).getAllByRole("checkbox")).toHaveLength(3);
        expect(list).toHaveTextContent("file · 15 KB");
        expect(screen.getByText(/cannot open apps, read other files, send anything or reach FNDR/)).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Doc" })).toHaveAttribute("aria-pressed", "true");

        fireEvent.click(screen.getByRole("button", { name: "Hand off" }));
        await waitFor(() => expect(ipc.handOffStart).toHaveBeenCalledTimes(1));
        expect(ipc.handOffStart).toHaveBeenCalledWith({
            taskText: draft.taskText,
            taskMemoryId: "m_mail",
            goal: "doc",
            memoryIds: ["m_mail", "m_notes"],
            fileMemoryIds: ["m_csv"],
            reviewed: draft.rendered,
        });
        expect(await screen.findByRole("button", { name: "Stop" })).toBeInTheDocument();
    });

    it("drafts again without an item the person unticks", async () => {
        render(<HandOffPanel source={source} onClose={() => {}} />);
        fireEvent.click(await screen.findByRole("checkbox", { name: /Notes: churn guesses/ }));
        await waitFor(() =>
            expect(ipc.handOffUpdateDraft).toHaveBeenCalledWith(
                expect.objectContaining({ memoryIds: ["m_mail"], fileMemoryIds: ["m_csv"] }),
            ),
        );
        expect(await screen.findByRole("checkbox", { name: /Notes: churn guesses/ })).not.toBeChecked();
    });

    it("keeps the person's choices when its parent renders again", async () => {
        const { rerender } = render(<HandOffPanel source={{ kind: "task", value: "task-1" }} onClose={() => {}} />);
        fireEvent.click(await screen.findByRole("checkbox", { name: /Notes: churn guesses/ }));
        expect(await screen.findByRole("checkbox", { name: /Notes: churn guesses/ })).not.toBeChecked();
        rerender(<HandOffPanel source={{ kind: "task", value: "task-1" }} onClose={() => {}} />);
        await waitFor(() => expect(ipc.handOffDraft).toHaveBeenCalledTimes(1));
        expect(screen.getByRole("checkbox", { name: /Notes: churn guesses/ })).not.toBeChecked();
    });

    it("asks for the document tools before anything can be handed off", async () => {
        ipc.handOffStatus.mockResolvedValue({ toolchain: "missing", running: null });
        ipc.handOffInstallToolchain.mockResolvedValue(installed);
        render(<HandOffPanel source={source} onClose={() => {}} />);
        const handOff = await screen.findByRole("button", { name: "Hand off" });
        expect(handOff).toBeDisabled();
        fireEvent.click(screen.getByRole("button", { name: "Install document tools" }));
        await waitFor(() => expect(screen.getByRole("button", { name: "Hand off" })).toBeEnabled());
        expect(screen.queryByRole("button", { name: "Install document tools" })).not.toBeInTheDocument();
    });

    it("shows the work as it happens, then the checked result with its files", async () => {
        render(<HandOffPanel source={source} onClose={() => {}} />);
        fireEvent.click(await screen.findByRole("button", { name: "Hand off" }));
        await screen.findByRole("button", { name: "Stop" });

        emit("hand_off://progress", { kind: "readFile", name: "q4-survey.csv" });
        emit("hand_off://progress", { kind: "ranCode", ok: true, ms: 900 });
        emit("hand_off://progress", { kind: "tokens", total: 11_000 });
        // The trace shows its latest step in its summary and in the list.
        expect(screen.getAllByText("Ran analysis code").length).toBeGreaterThan(0);
        expect(screen.queryByText(/q4-survey\.csv/)).not.toBeInTheDocument();

        emit("hand_off://finished", { record, error: null });
        expect(await screen.findByRole("heading", { name: "Ready" })).toBeInTheDocument();
        expect(screen.getByText(/3 of 3 claims checked against their sources by FNDR/)).toBeInTheDocument();
        const files = screen.getByRole("list", { name: "What it made" });
        fireEvent.click(within(files).getByRole("button", { name: "Open" }));
        expect(ipc.handOffOpen).toHaveBeenCalledWith("job-1", "out/q4-readout.docx");

        fireEvent.click(screen.getByRole("button", { name: "Yes" }));
        await waitFor(() => expect(ipc.handOffRate).toHaveBeenCalledWith("job-1", true));
        expect(await screen.findByText("Thanks, noted.")).toBeInTheDocument();
    });

    it("starts on its close button and closes on Escape", async () => {
        const onClose = vi.fn();
        render(<HandOffPanel source={source} onClose={onClose} />);
        await screen.findByRole("button", { name: "Hand off" });
        const close = screen.getByRole("button", { name: "Close hand-off" });
        expect(close).toHaveFocus();
        fireEvent.keyDown(close, { key: "Escape" });
        expect(onClose).toHaveBeenCalledTimes(1);
    });

    it("says why when a job never started", async () => {
        render(<HandOffPanel source={source} onClose={() => {}} />);
        fireEvent.click(await screen.findByRole("button", { name: "Hand off" }));
        await screen.findByRole("button", { name: "Stop" });
        emit("hand_off://finished", { record: null, error: "The draft changed since it was shown." });
        expect(await screen.findByRole("heading", { name: "It did not start" })).toBeInTheDocument();
        expect(screen.getByText("The draft changed since it was shown.")).toBeInTheDocument();
    });
});
