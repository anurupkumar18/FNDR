import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ThreadDigest } from "@/shared/ipc/tauri";
import { ThreadChanges } from "./ThreadChanges";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const now = new Date("2026-10-09T10:00:00").getTime();

function digest(overrides: Partial<ThreadDigest> = {}): ThreadDigest {
    return {
        thread_key: "parser",
        title: "Parser",
        since_ms: new Date("2026-10-08T15:00:00").getTime(),
        first_view: false,
        new_memories: 9,
        page_count: 7,
        pages: ["One", "Two", "Three", "Four", "Five", "Six", "Seven"],
        file_count: 0,
        files: [],
        task_count: 1,
        tasks: ["Add the regression test"],
        commit_count: 0,
        newest_memory_id: "latest",
        ...overrides,
    };
}

function route(row: ThreadDigest | Error) {
    vi.mocked(invoke).mockImplementation(async (command) => {
        if (command === "what_changed_since") {
            if (row instanceof Error) throw row;
            return row;
        }
        if (command === "mark_thread_seen") return undefined;
        throw new Error(`unexpected ${command}`);
    });
}

beforeEach(() => {
    vi.mocked(invoke).mockReset();
});
afterEach(cleanup);

describe("What changed since, on a thread", () => {
    it("shows one compact line and marks nothing seen on its own", async () => {
        route(digest());
        render(<ThreadChanges threadKey="parser" title="Parser" nowMs={now} />);
        const toggle = await screen.findByRole("button", { name: "7 new pages, 1 task since yesterday" });
        expect(toggle).toHaveAttribute("aria-expanded", "false");
        expect(invoke).toHaveBeenCalledWith("what_changed_since", { threadKey: "parser" });
        expect(invoke).not.toHaveBeenCalledWith("mark_thread_seen", expect.anything());
    });

    it("opens a list of up to five names per kind and marks the thread seen once", async () => {
        route(digest());
        render(<ThreadChanges threadKey="parser" title="Parser" nowMs={now} />);
        const toggle = await screen.findByRole("button", { name: /7 new pages/ });
        fireEvent.click(toggle);
        expect(toggle).toHaveAttribute("aria-expanded", "true");
        expect(screen.getByText("Five")).toBeInTheDocument();
        expect(screen.queryByText("Six")).not.toBeInTheDocument();
        expect(screen.getByText("and 2 more")).toBeInTheDocument();
        expect(screen.getByText("Add the regression test")).toBeInTheDocument();
        await waitFor(() => expect(invoke).toHaveBeenCalledWith("mark_thread_seen", { threadKey: "parser" }));
        fireEvent.click(toggle);
        fireEvent.click(toggle);
        expect(vi.mocked(invoke).mock.calls.filter((call) => call[0] === "mark_thread_seen")).toHaveLength(1);
    });

    it("dismissing hides the line and marks the thread seen", async () => {
        route(digest());
        render(<ThreadChanges threadKey="parser" title="Parser" nowMs={now} />);
        fireEvent.click(await screen.findByRole("button", { name: "Dismiss what changed in Parser" }));
        expect(screen.queryByRole("button", { name: /7 new pages/ })).not.toBeInTheDocument();
        await waitFor(() => expect(invoke).toHaveBeenCalledWith("mark_thread_seen", { threadKey: "parser" }));
    });

    it("says nothing when nothing is new or the digest cannot be read", async () => {
        route(digest({ new_memories: 0, page_count: 0, pages: [], task_count: 0, tasks: [] }));
        const quiet = render(<ThreadChanges threadKey="parser" title="Parser" nowMs={now} />);
        await waitFor(() => expect(invoke).toHaveBeenCalledOnce());
        expect(quiet.container).toBeEmptyDOMElement();
        quiet.unmount();

        route(new Error("store offline"));
        const failed = render(<ThreadChanges threadKey="parser" title="Parser" nowMs={now} />);
        await waitFor(() => expect(invoke).toHaveBeenCalledTimes(2));
        expect(failed.container).toBeEmptyDOMElement();
    });
});
