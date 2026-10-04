import { describe, expect, it } from "vitest";
import type { MemoryCard } from "@/shared/ipc/tauri";
import { groupVaultMemories, vaultSourceKind } from "../vaultGrouping";

/** Local-time timestamp so day buckets hold in any machine time zone. */
function at(month: number, day: number, hour: number, minute = 0): number {
    return new Date(2026, month - 1, day, hour, minute).getTime();
}

const NOW = at(9, 22, 21, 0);

function memory(id: string, overrides: Partial<MemoryCard> = {}): MemoryCard {
    return {
        id,
        title: `Memory ${id}`,
        summary: "",
        action: "",
        context: [],
        timestamp: at(9, 22, 9, 0),
        app_name: "VS Code",
        window_title: "",
        score: 1,
        source_count: 1,
        raw_snippets: [],
        ...overrides,
    };
}

/** Flattens the grouping into day > thread > lead[+similar] ids for compact assertions. */
function shape(memories: MemoryCard[]) {
    return groupVaultMemories(memories, NOW).map((day) => ({
        day: day.key,
        threads: day.threads.map((thread) => ({
            thread: thread.label,
            rows: thread.rows.map((row) =>
                row.similar.length > 0
                    ? `${row.lead.id}+${row.similar.map((card) => card.id).join(",")}`
                    : row.lead.id,
            ),
        })),
    }));
}

describe("groupVaultMemories", () => {
    it("buckets memories by local calendar day, newest day first, with Today and Yesterday labels", () => {
        const days = groupVaultMemories(
            [
                memory("late-yesterday", { timestamp: at(9, 21, 23, 30) }),
                memory("early-today", { timestamp: at(9, 22, 0, 15) }),
                memory("last-week", { timestamp: at(9, 15, 10, 0) }),
                memory("morning", { timestamp: at(9, 22, 9, 0) }),
            ],
            NOW,
        );

        expect(days.map((day) => day.key)).toEqual(["2026-09-22", "2026-09-21", "2026-09-15"]);
        expect(days.map((day) => day.label).slice(0, 2)).toEqual(["Today", "Yesterday"]);
        expect(days[2].label).not.toMatch(/Today|Yesterday/);
        expect(days.map((day) => day.count)).toEqual([2, 1, 1]);
    });

    it.each([
        {
            name: "same project across apps shares one thread labelled by the project",
            memories: [
                memory("rubric", { project: "HIST 2100 essay", app_name: "Google Chrome", timestamp: at(9, 22, 10) }),
                memory("reader", { project: " hist 2100 essay ", app_name: "Preview", timestamp: at(9, 22, 9) }),
            ],
            threads: [{ thread: "HIST 2100 essay", rows: ["rubric", "reader"] }],
        },
        {
            name: "no project falls back to one thread per app",
            memories: [
                memory("code-1", { app_name: "VS Code", timestamp: at(9, 22, 11) }),
                memory("web", { app_name: "Google Chrome", timestamp: at(9, 22, 10) }),
                memory("code-2", { app_name: "VS Code", timestamp: at(9, 22, 9) }),
            ],
            threads: [
                { thread: "VS Code", rows: ["code-1", "code-2"] },
                { thread: "Google Chrome", rows: ["web"] },
            ],
        },
        {
            name: "a blank project is treated as no project",
            memories: [
                memory("blank", { project: "   ", app_name: "Notes", timestamp: at(9, 22, 10) }),
                memory("named", { project: "Capstone", app_name: "Notes", timestamp: at(9, 22, 9) }),
            ],
            threads: [
                { thread: "Notes", rows: ["blank"] },
                { thread: "Capstone", rows: ["named"] },
            ],
        },
    ])("threads: $name", ({ memories, threads }) => {
        expect(shape(memories)).toEqual([{ day: "2026-09-22", threads }]);
    });

    it("orders threads by their newest memory and breaks timestamp ties by id, whatever the input order", () => {
        const memories = [
            memory("b", { project: "Alpha", timestamp: at(9, 22, 9) }),
            memory("a", { project: "Alpha", timestamp: at(9, 22, 9) }),
            memory("c", { project: "Beta", timestamp: at(9, 22, 12) }),
            memory("d", { project: "Alpha", timestamp: at(9, 22, 8) }),
        ];
        const expected = [
            {
                day: "2026-09-22",
                threads: [
                    { thread: "Beta", rows: ["c"] },
                    { thread: "Alpha", rows: ["a", "b", "d"] },
                ],
            },
        ];

        expect(shape(memories)).toEqual(expected);
        expect(shape([...memories].reverse())).toEqual(expected);
    });

    it("collapses near-duplicates by normalized title and app within one thread, keeping the newest as lead", () => {
        expect(
            shape([
                memory("draft-3", { title: "Drafted the demo script", app_name: "Notes", project: "Beta", timestamp: at(9, 22, 15, 40) }),
                memory("draft-2", { title: "drafted the  demo script!", app_name: "Notes", project: "Beta", timestamp: at(9, 22, 15, 25) }),
                memory("other-app", { title: "Drafted the demo script", app_name: "Pages", project: "Beta", timestamp: at(9, 22, 15, 20) }),
                memory("draft-1", { title: "Drafted the demo script.", app_name: "Notes", project: "Beta", timestamp: at(9, 22, 15, 10) }),
                memory("other-thread", { title: "Drafted the demo script", app_name: "Notes", project: "Capstone", timestamp: at(9, 22, 14) }),
                memory("other-day", { title: "Drafted the demo script", app_name: "Notes", project: "Beta", timestamp: at(9, 21, 15) }),
            ]),
        ).toEqual([
            {
                day: "2026-09-22",
                threads: [
                    { thread: "Beta", rows: ["draft-3+draft-2,draft-1", "other-app"] },
                    { thread: "Capstone", rows: ["other-thread"] },
                ],
            },
            {
                day: "2026-09-21",
                threads: [{ thread: "Beta", rows: ["other-day"] }],
            },
        ]);
    });

    it("counts collapsed memories in thread and day totals", () => {
        const [day] = groupVaultMemories(
            [
                memory("x1", { title: "Same", timestamp: at(9, 22, 10) }),
                memory("x2", { title: "Same", timestamp: at(9, 22, 9) }),
                memory("y", { title: "Different", timestamp: at(9, 22, 8) }),
            ],
            NOW,
        );

        expect(day.count).toBe(3);
        expect(day.threads[0].count).toBe(3);
        expect(day.threads[0].rows).toHaveLength(2);
    });

    it("returns no days for no memories", () => {
        expect(groupVaultMemories([], NOW)).toEqual([]);
    });
});

describe("vaultSourceKind", () => {
    it.each([
        { name: "web URL", overrides: { url: "https://docs.example.com/outline" }, kind: "page" },
        { name: "http reopen target", overrides: { reopen_target: "https://canvas.example.edu/a/7" }, kind: "page" },
        {
            name: "document path",
            overrides: { app_name: "Preview", reopen_target: "file:///tmp/fixtures/reader.pdf", reopen_page: 112 },
            kind: "document",
        },
        { name: "download tracker memory", overrides: { app_name: "Finder", window_title: "Downloads" }, kind: "download" },
        {
            name: "agent note, even with a URL",
            overrides: { source_type: "agent", url: "https://example.com" },
            kind: "agent",
        },
        { name: "plain screen capture", overrides: { app_name: "Terminal", files_touched: ["src/main.rs"] }, kind: "screen" },
    ])("$name maps to $kind", ({ overrides, kind }) => {
        expect(vaultSourceKind({ ...memory("m"), ...overrides })).toBe(kind);
    });
});
