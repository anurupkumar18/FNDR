import { describe, expect, it } from "vitest";
import type { Task, ThreadDigest, WorkItem, WorkItemOutcome, WorkSet } from "@/shared/ipc/tauri";
import {
    asksBeforeOpening,
    changeLine,
    deadlineHeadline,
    dueLabel,
    layoutFor,
    outcomeStatus,
    pickSet,
    upcomingDeadlines,
} from "./homeWorkSet";

const HOUR = 60 * 60 * 1000;
const now = new Date("2026-10-09T10:00:00").getTime();

function item(memoryId: string): WorkItem {
    return { memoryId, label: memoryId, kind: "url", reopenRank: 4, appName: "Safari", capturedAt: 1 };
}

function set(id: string, ids: string[]): WorkSet {
    return { id, title: id, reason: "", score: 1, items: ids.map(item) };
}

function task(overrides: Partial<Task>): Task {
    return {
        id: "t",
        title: "Lab 4",
        description: "",
        source_app: "manual",
        source_memory_id: "m1",
        created_at: 0,
        due_date: now + 20 * HOUR,
        is_completed: false,
        is_dismissed: false,
        task_type: "Todo",
        linked_urls: [],
        linked_memory_ids: [],
        ...overrides,
    };
}

function digest(overrides: Partial<ThreadDigest>): ThreadDigest {
    return {
        thread_key: "k",
        title: "T",
        since_ms: new Date("2026-10-08T15:00:00").getTime(),
        first_view: false,
        new_memories: 0,
        page_count: 0,
        pages: [],
        file_count: 0,
        files: [],
        task_count: 0,
        tasks: [],
        commit_count: 0,
        newest_memory_id: null,
        ...overrides,
    };
}

describe("the one-tap rule", () => {
    it("asks before opening only when a set holds more than three items, as Notch Do does", () => {
        expect(asksBeforeOpening(3)).toBe(false);
        expect(asksBeforeOpening(4)).toBe(true);
    });
});

describe("picking the set that belongs to a thread or task", () => {
    it("takes the candidate sharing the most memories with the evidence", () => {
        const resolution = { kind: "ambiguous" as const, value: [set("a", ["x"]), set("b", ["m1", "m2"])] };
        expect(pickSet(resolution, ["m1", "m2", "m3"])?.id).toBe("b");
    });

    it("never guesses a set that shares nothing with the evidence", () => {
        expect(pickSet({ kind: "best", value: set("a", ["x"]) }, ["m1"])).toBeNull();
        expect(pickSet({ kind: "none", value: { why: "no" } }, ["m1"])).toBeNull();
    });
});

describe("item outcomes", () => {
    const base: WorkItemOutcome = { memoryId: "m", label: "Page", ok: true, detail: "Opened", outcome: { kind: "opened" } };

    it("reads opened, moved and needs permission from the typed outcome", () => {
        expect(outcomeStatus(base)).toBe("opened");
        expect(outcomeStatus({ ...base, outcome: { kind: "opened_moved", new_path: "/a/b.pdf" } })).toBe("moved");
        expect(outcomeStatus({ ...base, ok: false, outcome: undefined, detail: "Actions are turned off in Settings." })).toBe("needs_permission");
        expect(outcomeStatus({ ...base, ok: false, outcome: { kind: "missing", path: "/a" }, detail: "a is no longer there" })).toBe("failed");
        expect(outcomeStatus({ ...base, ok: false, outcome: undefined, detail: "Left out: FNDR does not reopen private or blocklisted memories" })).toBe("failed");
    });
});

describe("arranging side by side", () => {
    it("picks a layout that fits the number of items", () => {
        expect(layoutFor(1)).toBe("maximize");
        expect(layoutFor(2)).toBe("left_right_split");
        expect(layoutFor(3)).toBe("thirds");
        expect(layoutFor(5)).toBe("grid2x2");
    });
});

describe("deadlines", () => {
    it("keeps open, accepted tasks due within 72 hours, soonest first", () => {
        const tasks = [
            task({ id: "later", due_date: now + 60 * HOUR }),
            task({ id: "soon", due_date: now + 2 * HOUR }),
            task({ id: "far", due_date: now + 80 * HOUR }),
            task({ id: "none", due_date: null }),
            task({ id: "done", is_completed: true }),
            task({ id: "suggested", source_app: "Memory: Chrome" }),
            task({ id: "long-gone", due_date: now - 30 * HOUR }),
        ];
        expect(upcomingDeadlines(tasks, now).map((row) => row.id)).toEqual(["soon", "later"]);
    });

    it("says when a task is due in calendar words", () => {
        expect(dueLabel(now + 2 * HOUR, now)).toBe("due today");
        expect(dueLabel(now + 20 * HOUR, now)).toBe("due tomorrow");
        expect(dueLabel(new Date("2026-10-11T09:00:00").getTime(), now)).toBe("due Sunday");
        expect(dueLabel(now - HOUR, now)).toBe("overdue");
    });

    it("writes the card headline with how many items are ready", () => {
        expect(deadlineHeadline(task({}), 3, now)).toBe("Lab 4 due tomorrow, 3 items ready to reopen");
        expect(deadlineHeadline(task({}), 1, now)).toBe("Lab 4 due tomorrow, 1 item ready to reopen");
        expect(deadlineHeadline(task({}), 0, now)).toBe("Lab 4 due tomorrow");
    });
});

describe("what changed since", () => {
    it("counts each kind and says since when", () => {
        expect(changeLine(digest({ page_count: 3, task_count: 1 }), now)).toBe("3 new pages, 1 task since yesterday");
        expect(changeLine(digest({ file_count: 1, commit_count: 2, since_ms: new Date("2026-10-09T08:05:00").getTime() }), now))
            .toMatch(/^1 new file, 2 commits since 8:05/);
        expect(changeLine(digest({ page_count: 1, first_view: true }), now)).toBe("1 new page in the last day");
        expect(changeLine(digest({ page_count: 2, since_ms: new Date("2026-10-06T08:00:00").getTime() }), now)).toBe("2 new pages since Oct 6");
    });

    it("falls back to captures, and says nothing when nothing is new", () => {
        expect(changeLine(digest({ new_memories: 4 }), now)).toBe("4 new captures since yesterday");
        expect(changeLine(digest({}), now)).toBeNull();
    });
});
