import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { ImportSources } from "./ImportSources";

const ipc = vi.hoisted(() => ({
    importListSources: vi.fn(), importGetConsent: vi.fn(), importSetConsent: vi.fn(),
    importStart: vi.fn(), importStatus: vi.fn(), importCancel: vi.fn(), importForget: vi.fn(),
    importFirstAnswerMs: vi.fn(), importRecordFirstAnswer: vi.fn(),
    listMemoryCards: vi.fn(), searchMemoryCards: vi.fn(), reopenMemory: vi.fn(),
}));
vi.mock("@/shared/ipc/tauri", () => ipc);

const kinds = ["browser", "file", "calendar", "mail", "slack", "git"];
const status = (source: string, state = "idle", stored = 0) => ({
    source, state, window_days: 30, scanned: stored, stored, updated: 0,
    duplicates: 0, redacted: 0, skipped: 0, skip_reasons: {},
    started_at_ms: 1, finished_at_ms: state === "done" ? 2 : null, error: null,
});

beforeEach(() => {
    ipc.importListSources.mockResolvedValue(kinds.map((kind) => ({
        id: kind, kind, label: kind, permissions: [], row_count: 0, status: status(kind),
    })));
    ipc.importGetConsent.mockResolvedValue(false);
    ipc.importSetConsent.mockResolvedValue(undefined);
    ipc.importStart.mockImplementation((source: string) => Promise.resolve(status(source, "running")));
    ipc.importStatus.mockImplementation((source: string) => Promise.resolve(status(source, "running")));
    ipc.importCancel.mockResolvedValue(true);
    ipc.importForget.mockResolvedValue(0);
    ipc.importFirstAnswerMs.mockResolvedValue(null);
    ipc.importRecordFirstAnswer.mockResolvedValue(undefined);
    ipc.listMemoryCards.mockResolvedValue([]);
    ipc.searchMemoryCards.mockResolvedValue([]);
    ipc.reopenMemory.mockResolvedValue({ kind: "opened" });
});
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("Bring your past", () => {
    it.each(kinds)("requires an explicit switch for %s", async (kind) => {
        render(<ImportSources mode="onboarding" onContinue={vi.fn()} />);
        const labels: Record<string, string> = {
            browser: "Browser history", file: "Recent files", calendar: "Calendar",
            mail: "Mail", slack: "Slack export", git: "GitHub and git",
        };
        const toggle = await screen.findByRole("switch", { name: labels[kind] });
        await waitFor(() => expect(toggle).toBeEnabled());
        fireEvent.click(toggle);
        await waitFor(() => expect(ipc.importSetConsent).toHaveBeenCalledWith(kind, true));
        expect(ipc.importStart).toHaveBeenCalledWith(kind, 30);
    });

    it("shows all source cards off with clear permissions and an opt-out", async () => {
        render(<ImportSources mode="onboarding" onContinue={vi.fn()} />);
        expect(await screen.findByRole("heading", { name: "Bring your past" })).toBeInTheDocument();
        expect(screen.getAllByRole("switch")).toHaveLength(6);
        expect(screen.getByRole("switch", { name: "Browser history" })).not.toBeChecked();
        expect(screen.getByText(/Safari.*Full Disk Access/i)).toBeInTheDocument();
        expect(screen.getByText(/Slack export ZIP/i)).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Skip imports" })).toBeInTheDocument();
    });

    it("recommends only browser, files and calendar with a 30 day window", async () => {
        render(<ImportSources mode="onboarding" onContinue={vi.fn()} />);
        fireEvent.click(await screen.findByRole("button", { name: "Import everything recommended" }));
        await waitFor(() => expect(ipc.importStart).toHaveBeenCalledTimes(3));
        expect(ipc.importStart.mock.calls).toEqual([["browser", 30], ["file", 30], ["calendar", 30]]);
        expect(ipc.importSetConsent.mock.calls).toEqual([["browser", true], ["file", true], ["calendar", true]]);
        expect(ipc.importSetConsent).not.toHaveBeenCalledWith("mail", true);
        expect(ipc.importSetConsent).not.toHaveBeenCalledWith("slack", true);
    });

    it("shows progress, allows cancel, and confirms Forget with an exact count", async () => {
        const source = { id: "browser", kind: "browser", label: "Browser history", permissions: [], row_count: 4, status: status("browser", "running", 4) };
        ipc.importListSources.mockResolvedValueOnce([source]).mockResolvedValueOnce([source]);
        ipc.importStatus.mockResolvedValueOnce(status("browser", "cancelled", 4));
        render(<ImportSources mode="settings" />);
        const card = await screen.findByRole("group", { name: "Browser history" });
        expect(within(card).getByText(/4 items imported/i)).toBeInTheDocument();
        fireEvent.click(within(card).getByRole("button", { name: "Cancel" }));
        await waitFor(() => expect(ipc.importCancel).toHaveBeenCalledWith("browser"));
        await waitFor(() => expect(within(card).getByRole("button", { name: "Forget" })).toBeEnabled());
        fireEvent.click(within(card).getByRole("button", { name: "Forget" }));
        expect(await screen.findByRole("alertdialog", { name: "Forget source" })).toHaveTextContent("remove 4 items");
        fireEvent.click(screen.getByRole("button", { name: "Remove 4 items" }));
        await waitFor(() => expect(ipc.importForget).toHaveBeenCalledWith("browser"));
    });

    it("searches imported titles and records an opened answer locally", async () => {
        ipc.importListSources.mockResolvedValueOnce([{ id: "file", kind: "file", label: "Recent files", permissions: [], row_count: 1, status: status("file", "done", 1) }]);
        ipc.listMemoryCards.mockResolvedValueOnce([{ id: "imp_1", title: "Atlas planning notes", source_type: "import:file" }]);
        ipc.searchMemoryCards.mockResolvedValueOnce([{ id: "imp_1", title: "Atlas planning notes", source_type: "import:file" }]);
        render(<ImportSources mode="onboarding" onContinue={vi.fn()} />);
        expect(await screen.findByRole("button", { name: "Atlas planning" })).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "planning notes" })).toBeInTheDocument();
        fireEvent.click(await screen.findByRole("button", { name: /Atlas planning notes/i }));
        expect(screen.getByRole("searchbox", { name: "Where did I see that?" })).toHaveValue("Atlas planning notes");
        fireEvent.keyDown(screen.getByRole("searchbox", { name: "Where did I see that?" }), { key: "Enter" });
        fireEvent.click(await screen.findByRole("button", { name: /Open Atlas planning notes/i }));
        await waitFor(() => expect(ipc.reopenMemory).toHaveBeenCalledWith("imp_1"));
        expect(ipc.importRecordFirstAnswer).toHaveBeenCalled();
    });

    it("offers retry on source-list errors", async () => {
        ipc.importListSources.mockRejectedValueOnce(new Error("unavailable"));
        render(<ImportSources mode="onboarding" onContinue={vi.fn()} />);
        expect(await screen.findByRole("alert")).toHaveTextContent("Could not load import sources");
        fireEvent.click(screen.getByRole("button", { name: "Retry" }));
        await waitFor(() => expect(screen.getAllByRole("switch")).toHaveLength(6));
    });

    it("keeps empty source lists opt-out only and clamps a huge window", async () => {
        ipc.importListSources.mockResolvedValueOnce([]);
        const view = render(<ImportSources mode="onboarding" onContinue={vi.fn()} />);
        expect(await screen.findByRole("button", { name: "Import everything recommended" })).toBeDisabled();
        view.unmount();
        ipc.importListSources.mockResolvedValueOnce([{ id: "file", kind: "file", label: "Recent files", permissions: [], row_count: 0, status: status("file") }]);
        render(<ImportSources mode="settings" />);
        const windowInput = await screen.findByRole("spinbutton", { name: "Window days" });
        fireEvent.change(windowInput, { target: { value: "999999" } });
        expect(windowInput).toHaveValue(90);
    });

    it("keeps Unicode titles intact and returns focus when Forget is cancelled", async () => {
        ipc.importListSources.mockResolvedValueOnce([{ id: "file", kind: "file", label: "Recent files", permissions: [], row_count: 1, status: status("file", "done", 1) }]);
        ipc.listMemoryCards.mockResolvedValueOnce([{ id: "imp_1", title: "Résumé 東京", source_type: "import:file" }]);
        const view = render(<ImportSources mode="onboarding" onContinue={vi.fn()} />);
        expect(await screen.findByRole("button", { name: "Résumé 東京" })).toBeInTheDocument();
        view.unmount();
        ipc.importListSources.mockResolvedValueOnce([{ id: "file", kind: "file", label: "Recent files", permissions: [], row_count: 1, status: status("file", "done", 1) }]);
        render(<ImportSources mode="settings" />);
        const forget = await screen.findByRole("button", { name: "Forget" });
        forget.focus();
        fireEvent.click(forget);
        const keep = await screen.findByRole("button", { name: "Keep items" });
        expect(keep).toHaveFocus();
        const remove = screen.getByRole("button", { name: "Remove 0 items" });
        remove.focus();
        fireEvent.keyDown(remove, { key: "Tab" });
        expect(keep).toHaveFocus();
        fireEvent.keyDown(screen.getByRole("alertdialog", { name: "Forget source" }), { key: "Escape" });
        expect(screen.queryByRole("alertdialog")).toBeNull();
        expect(forget).toHaveFocus();
        expect(ipc.importForget).not.toHaveBeenCalled();
    });

    it("reports a permission refusal and ignores an empty guided query", async () => {
        ipc.importStart.mockRejectedValueOnce(new Error("Calendar permission denied"));
        ipc.importListSources.mockResolvedValueOnce([{
            id: "calendar", kind: "calendar", label: "Calendar", permissions: ["calendar"],
            row_count: 1, status: status("calendar", "done", 1),
        }]);
        ipc.listMemoryCards.mockResolvedValueOnce([{ id: "imp_1", title: "Team review", source_type: "import:calendar" }]);
        render(<ImportSources mode="onboarding" onContinue={vi.fn()} />);
        const toggle = await screen.findByRole("switch", { name: "Calendar" });
        await waitFor(() => expect(toggle).toBeEnabled());
        fireEvent.click(toggle);
        expect(await screen.findByRole("alert")).toHaveTextContent("Calendar permission denied");
        fireEvent.click(screen.getByRole("button", { name: "Search" }));
        expect(ipc.searchMemoryCards).not.toHaveBeenCalled();
    });
});
