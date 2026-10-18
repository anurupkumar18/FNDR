import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import {
    importCancel,
    importForget,
    importFirstAnswerMs,
    importGetConsent,
    importListSources,
    importRecordFirstAnswer,
    importSetConsent,
    importStart,
    importStatus,
} from "./tauri";

afterEach(clearMocks);

it("uses the import command contract and source arguments", async () => {
    const calls: Array<[string, unknown]> = [];
    mockIPC((command, payload) => {
        calls.push([command, payload]);
        if (command === "import_list_sources") return [];
        if (command === "import_start" || command === "import_status") {
            return { source: "mail", state: "done", window_days: 30 };
        }
        if (command === "import_cancel") return true;
        if (command === "import_forget") return 2;
        if (command === "import_get_consent") return false;
        if (command === "import_set_consent" || command === "import_record_first_answer") return null;
        if (command === "import_first_answer_ms") return 1200;
        throw new Error(`Unexpected command: ${command}`);
    });

    await expect(importListSources()).resolves.toEqual([]);
    await expect(importStart("mail", 30)).resolves.toMatchObject({ source: "mail" });
    await expect(importStatus("mail")).resolves.toMatchObject({ state: "done" });
    await expect(importCancel("mail")).resolves.toBe(true);
    await expect(importForget("mail")).resolves.toBe(2);
    await expect(importGetConsent("mail")).resolves.toBe(false);
    await expect(importSetConsent("mail", true)).resolves.toBeNull();
    await expect(importRecordFirstAnswer(1200)).resolves.toBeNull();
    await expect(importFirstAnswerMs()).resolves.toBe(1200);
    expect(calls).toEqual([
        ["import_list_sources", {}],
        ["import_start", { source: "mail", windowDays: 30 }],
        ["import_status", { source: "mail" }],
        ["import_cancel", { source: "mail" }],
        ["import_forget", { source: "mail" }],
        ["import_get_consent", { source: "mail" }],
        ["import_set_consent", { source: "mail", enabled: true }],
        ["import_record_first_answer", { elapsedMs: 1200 }],
        ["import_first_answer_ms", {}],
    ]);
});
