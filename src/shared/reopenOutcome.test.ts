import { describe, expect, it } from "vitest";
import type { ReopenOutcome } from "@/shared/ipc/tauri";
import { reopenOutcomeMessage } from "./reopenOutcome";

describe("reopenOutcomeMessage", () => {
    const cases: [ReopenOutcome, string][] = [
        [{ kind: "opened" }, "Opened."],
        [
            { kind: "opened_moved", new_path: "/Users/qa/moved.pdf" },
            "File was moved. Opened it from /Users/qa/moved.pdf.",
        ],
        [
            { kind: "missing", path: "/Users/qa/gone.pdf" },
            "This file no longer exists at /Users/qa/gone.pdf. The memory is still here.",
        ],
        [
            { kind: "drive_not_connected", volume: "RE07USB", path: "/Volumes/RE07USB/doc.pdf" },
            "Connect the drive RE07USB to open this file.",
        ],
        [
            { kind: "app_missing", bundle_id: "com.example.Gone", app_name: "Gone" },
            "Gone is no longer installed.",
        ],
        [
            { kind: "app_missing", bundle_id: "com.example.Gone" },
            "com.example.Gone is no longer installed.",
        ],
        [
            { kind: "app_only", app_name: "Preview" },
            "Opened Preview. No specific page or file was saved for this memory.",
        ],
        [
            { kind: "app_only" },
            "Opened the app. No specific page or file was saved for this memory.",
        ],
        [{ kind: "blocked", target: "chrome://settings" }, "FNDR does not open this kind of link."],
        [{ kind: "no_target" }, "Nothing to reopen for this memory."],
    ];

    it("returns one line for every outcome", () => {
        for (const [outcome, expected] of cases) {
            expect(reopenOutcomeMessage(outcome)).toBe(expected);
        }
    });
});
