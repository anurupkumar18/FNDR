import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
    CAPTURE_STATUS_EVENT,
    SCREEN_GUIDE_STATE_EVENT,
    type CaptureStatus,
    type ScreenGuideStateEvent,
} from "@/shared/ipc/tauri";
import { createPreviewIpcHandler } from "./previewIpc";
import { resolvePreviewConfig } from "./previewConfig";

afterEach(() => {
    clearMocks();
});

describe("development UI preview IPC", () => {
    it("defaults visual QA to a stable Film dark scene", () => {
        expect(resolvePreviewConfig("")).toEqual({
            theme: "dark",
            palette: "system",
            motion: "off",
        });
        expect(resolvePreviewConfig("?theme=light&palette=matrix&motion=on")).toEqual({
            theme: "light",
            palette: "matrix",
            motion: "on",
        });
    });

    it("boots directly into a populated, unlocked FNDR workspace", async () => {
        const invoke = createPreviewIpcHandler();

        await expect(invoke("get_onboarding_state")).resolves.toMatchObject({
            step: "complete",
            biometric_enabled: false,
            model_downloaded: true,
            display_name: "Anurup",
        });
        await expect(invoke("get_status")).resolves.toMatchObject({
            is_capturing: true,
            is_paused: false,
            embedding_degraded: false,
        });
        await expect(invoke("get_fun_greeting")).resolves.toBe("Good evening, Anurup.");
        await expect(invoke("get_privacy_alerts")).resolves.toEqual([]);
        await expect(invoke("list_memory_cards")).resolves.toEqual(
            expect.arrayContaining([
                expect.objectContaining({
                    id: "memory-design-review",
                    title: "Reviewed the FNDR home experience",
                }),
            ]),
        );
        await expect(invoke("list_needs_signal_memory_cards")).resolves.toEqual(
            expect.arrayContaining([
                expect.objectContaining({
                    card: expect.objectContaining({ id: "memory-needs-review" }),
                }),
            ]),
        );
    });

    it("models the bounded Memory Vault detail actions", async () => {
        const invoke = createPreviewIpcHandler();

        await expect(
            invoke("fndr_get_related_memories", {
                memoryId: "memory-design-review",
                limit: 4,
            }),
        ).resolves.toEqual(
            expect.arrayContaining([
                expect.objectContaining({ id: "memory-accessibility-notes" }),
            ]),
        );
        await expect(
            invoke("fndr_get_memory_subgraph", {
                seedIds: ["memory-design-review"],
                maxHops: 2,
            }),
        ).resolves.toMatchObject({
            seed_ids: ["memory-design-review"],
            node_count: 3,
            edge_count: 2,
        });
        await expect(
            invoke("find_visually_similar_memories", {
                seedMemoryId: "memory-design-review",
                limit: 6,
            }),
        ).resolves.toEqual(
            expect.arrayContaining([
                expect.objectContaining({ id: "similar-home-review" }),
            ]),
        );
        await expect(
            invoke("fndr_build_context_pack", { query: "Reviewed the FNDR home experience" }),
        ).resolves.toMatchObject({
            query: "Reviewed the FNDR home experience",
            summary: expect.stringContaining("synthetic"),
        });
        await expect(
            invoke("reopen_memory", { memoryId: "memory-accessibility-notes" }),
        ).resolves.toBe(true);
        await expect(
            invoke("delete_memory", { memoryId: "memory-design-review" }),
        ).resolves.toBe(true);
        await expect(invoke("list_memory_cards")).resolves.not.toEqual(
            expect.arrayContaining([
                expect.objectContaining({ id: "memory-design-review" }),
            ]),
        );
    });

    it("models search, search summaries, and grounded Ask without native services", async () => {
        const invoke = createPreviewIpcHandler();

        await expect(
            invoke("search_memory_cards", {
                query: "accessibility",
                appFilter: "Google Chrome",
                limit: 2,
            }),
        ).resolves.toEqual([
            expect.objectContaining({ id: "memory-accessibility-notes" }),
        ]);
        await expect(
            invoke("summarize_search", {
                query: "accessibility",
                resultsSnippets: ["Keyboard checks", "Contrast checks"],
            }),
        ).resolves.toMatch(/preview-only/i);
        await expect(
            invoke("transcribe_voice_input", {
                audioBytes: [1, 2, 3],
                mimeType: "audio/webm",
            }),
        ).resolves.toEqual({
            text: "Search for the FNDR browser preview",
            backend: "preview-synthetic-audio",
        });
        await expect(
            invoke("fndr_answer", { query: "How does the browser preview avoid permissions?" }),
        ).resolves.toMatchObject({
            query: "How does the browser preview avoid permissions?",
            answer: expect.stringContaining("synthetic"),
            verify_outcome: { kind: "grounded" },
            cards: expect.arrayContaining([
                expect.objectContaining({ id: "memory-preview-harness" }),
            ]),
        });
        await expect(
            invoke("fndr_answer", { query: "What is the launch code for Europa?" }),
        ).resolves.toMatchObject({
            verify_outcome: { kind: "not_enough_evidence" },
            cards: [],
        });
    });

    it("models Daily Summary, safe PDF actions, and follow-up state", async () => {
        const invoke = createPreviewIpcHandler();

        await expect(
            invoke("generate_daily_summary_for_date", { dateStr: "2026-09-22" }),
        ).resolves.toContain("September 22");
        await expect(
            invoke("get_daily_summary_overview", { dateStr: "2026-09-22" }),
        ).resolves.toContain("follow-up");

        const followups = await invoke("get_daily_summary_followups");
        expect(followups).toEqual(
            expect.arrayContaining([
                expect.objectContaining({ id: "task-share-audit", task_type: "Followup" }),
            ]),
        );
        await expect(
            invoke("set_todo_completed", {
                taskId: "task-share-audit",
                isCompleted: true,
            }),
        ).resolves.toBe(true);
        await expect(invoke("get_daily_summary_followups")).resolves.toEqual([]);

        const path = await invoke("export_daily_summary_pdf", {
            dateStr: "2026-09-22",
            summaryText: "Preview recap",
        });
        expect(path).toBe("preview-only://exports/daily-summary-2026-09-22.pdf");
        await expect(invoke("open_exported_pdf", { path })).resolves.toBeUndefined();
        await expect(
            invoke("open_exported_pdf", { path: "/tmp/real-file.pdf" }),
        ).rejects.toThrow("preview-only export path");
    });

    it("models mutable To-dos and the daily briefing", async () => {
        const invoke = createPreviewIpcHandler();

        await expect(invoke("generate_daily_briefing", { mode: "daily" })).resolves.toContain(
            "preview",
        );
        const created = await invoke("add_todo", {
            title: "Review the browser walkthrough",
            taskType: "Todo",
        });
        expect(created).toMatchObject({
            title: "Review the browser walkthrough",
            task_type: "Todo",
            is_completed: false,
        });
        const createdId = (created as { id: string }).id;

        await expect(
            invoke("update_todo", {
                taskId: createdId,
                title: "Review every browser path",
                taskType: "Reminder",
            }),
        ).resolves.toMatchObject({
            id: createdId,
            title: "Review every browser path",
            task_type: "Reminder",
        });
        await expect(invoke("complete_todo", { taskId: createdId })).resolves.toBe(true);
        await expect(invoke("get_todos")).resolves.not.toEqual(
            expect.arrayContaining([expect.objectContaining({ id: createdId })]),
        );
        await expect(invoke("complete_todo", { taskId: "missing-task" })).resolves.toBe(false);
    });

    it("models Wrapped generation and keeps its export virtual", async () => {
        const invoke = createPreviewIpcHandler();

        await expect(
            invoke("get_weekly_wrapped", {
                startDate: "2026-09-21",
                endDate: "2026-09-22",
            }),
        ).resolves.toMatchObject({
            start_date: "2026-09-21",
            end_date: "2026-09-22",
            total_captures: 1842,
            apps: expect.arrayContaining([
                expect.objectContaining({ name: "Visual Studio Code" }),
            ]),
        });
        await expect(
            invoke("export_weekly_wrapped_pdf", {
                startDate: "2026-09-21",
                endDate: "2026-09-22",
                recapText: "A synthetic weekly recap",
            }),
        ).resolves.toBe("preview-only://exports/fndr-wrapped-2026-09-21-to-2026-09-22.pdf");
    });

    it("models Screen Guide settings and hold-to-talk without capture permissions", async () => {
        const invoke = createPreviewIpcHandler();

        await expect(invoke("get_screen_guide_settings")).resolves.toMatchObject({
            enabled: true,
            shortcut: "Control+Alt+Space",
            speak_responses: false,
            show_cursor: true,
        });
        await expect(
            invoke("set_screen_guide_settings", {
                settings: {
                    enabled: true,
                    shortcut: "Command+Shift+Space",
                    speak_responses: true,
                    show_cursor: false,
                },
            }),
        ).resolves.toMatchObject({
            shortcut: "Command+Shift+Space",
            speak_responses: true,
            show_cursor: false,
        });

        await expect(invoke("screen_guide_press")).resolves.toBe(1);
        await expect(invoke("screen_guide_release", { generation: 1 })).resolves.toBeUndefined();
        await expect(
            invoke("submit_screen_guide_text", { text: "Where is the settings button?" }),
        ).resolves.toBeUndefined();
        await expect(invoke("screen_guide_press")).resolves.toBe(2);
    });

    it("returns populated Stats, Engine diagnostics, and Privacy Activity fixtures", async () => {
        const invoke = createPreviewIpcHandler();

        await expect(invoke("get_stats")).resolves.toMatchObject({
            total_records: 1842,
            today_count: 126,
            apps: expect.arrayContaining([
                expect.objectContaining({ name: "Visual Studio Code" }),
            ]),
        });
        await expect(invoke("get_runtime_metrics")).resolves.toMatchObject({
            process_rss_bytes: 284_164_096,
            embedding: { backend: "ONNX Runtime", degraded: false },
            system: {
                host_memory: { pressure_label: "low" },
                model_memory: expect.arrayContaining([
                    expect.objectContaining({ id: "qwen3-vl-2b", loaded: true }),
                ]),
            },
        });
        await expect(invoke("get_memory_review_status")).resolves.toMatchObject({
            worker_enabled: true,
            pressure_blocked: false,
            queue_depth: 3,
        });
        await expect(invoke("get_privacy_proof")).resolves.toMatchObject({
            evaluated: 2159,
            stored: 1842,
            skipped_by_reason: {
                sensitive_context: 14,
                blocklist: 42,
            },
            egress_requests: 0,
        });
    });

    it("models Settings reads and reversible local controls", async () => {
        const invoke = createPreviewIpcHandler();

        await expect(invoke("get_blocklist")).resolves.toEqual([
            "1Password",
            "bank.example",
        ]);
        await expect(invoke("list_available_models")).resolves.toEqual(
            expect.arrayContaining([
                expect.objectContaining({
                    id: "qwen3-vl-2b",
                    download_url: "already_downloaded",
                }),
            ]),
        );
        await expect(invoke("fndr_quality_status")).resolves.toMatchObject({
            stored_count: 1842,
            dropped_count: 317,
        });

        await invoke("pause_capture");
        await expect(invoke("get_status")).resolves.toMatchObject({ is_paused: true });
        await invoke("resume_capture");
        await expect(invoke("get_status")).resolves.toMatchObject({ is_paused: false });

        await invoke("set_blocklist", { apps: ["Private Browser"] });
        await expect(invoke("get_blocklist")).resolves.toEqual(["Private Browser"]);

        const onboarding = await invoke("get_onboarding_state");
        await invoke("save_onboarding_state", {
            state: { ...(onboarding as object), display_name: "Ada" },
        });
        await expect(invoke("get_onboarding_state")).resolves.toMatchObject({
            display_name: "Ada",
        });
    });

    it("emits capture status changes so shell and command palette stay synchronized", async () => {
        mockIPC(createPreviewIpcHandler(), { shouldMockEvents: true });
        const statusListener = vi.fn();
        const unlisten = await listen<CaptureStatus>(CAPTURE_STATUS_EVENT, statusListener);

        await tauriInvoke("pause_capture");
        expect(statusListener).toHaveBeenLastCalledWith(
            expect.objectContaining({
                event: CAPTURE_STATUS_EVENT,
                payload: expect.objectContaining({ is_paused: true }),
            }),
        );

        await tauriInvoke("resume_capture");
        expect(statusListener).toHaveBeenLastCalledWith(
            expect.objectContaining({
                event: CAPTURE_STATUS_EVENT,
                payload: expect.objectContaining({ is_paused: false }),
            }),
        );
        await unlisten();
    });

    it("emits an explicit preview-only Screen Guide result without capturing a screen", async () => {
        mockIPC(createPreviewIpcHandler(), { shouldMockEvents: true });
        const statusListener = vi.fn();
        const unlisten = await listen<ScreenGuideStateEvent>(
            SCREEN_GUIDE_STATE_EVENT,
            statusListener,
        );

        await tauriInvoke("submit_screen_guide_text", {
            text: "Where is the settings button?",
        });
        expect(statusListener).toHaveBeenLastCalledWith(
            expect.objectContaining({
                event: SCREEN_GUIDE_STATE_EVENT,
                payload: {
                    phase: "answer",
                    message: "Preview-only response ready; no screen was captured.",
                },
            }),
        );
        await unlisten();
    });

    it("fails loudly when a screen needs an unmodeled command", async () => {
        const invoke = createPreviewIpcHandler();

        await expect(invoke("not_a_real_fndr_command")).rejects.toThrow(
            "Unhandled UI preview command: not_a_real_fndr_command",
        );
    });
});
