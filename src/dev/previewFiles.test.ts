import { afterEach, describe, expect, it } from "vitest";
import type { FileHit, FilesSearchResult, FilesSettings } from "@/shared/ipc/files";
import { createPreviewIpcHandler } from "./previewIpc";

afterEach(() => window.history.replaceState(null, "", "/"));

describe("synthetic Files preview IPC", () => {
    it("keeps Files settings isolated and leaves the memory pipeline unchanged", async () => {
        const invoke = createPreviewIpcHandler();
        const before = await invoke("get_status");
        const settings = await invoke("files_settings_get") as FilesSettings;
        expect(settings.enabled).toBe(true);
        expect(settings.folders.length).toBeGreaterThan(0);
        await invoke("files_settings_set", { settings: { ...settings, enabled: false } });
        await expect(invoke("files_search", { query: "atlas" })).resolves.toMatchObject({ hits: [], strong: 0 });
        expect(await invoke("get_status")).toEqual(before);
        await expect(createPreviewIpcHandler()("files_settings_get")).resolves.toMatchObject({ enabled: true });
    });

    it("filters synthetic hits by prefix, kind, query and limit", async () => {
        const invoke = createPreviewIpcHandler();
        const all = await invoke("files_search", { query: "file: atlas" }) as FilesSearchResult;
        expect(all.hits.length).toBeGreaterThan(1);
        expect(all.hits.every((hit) => hit.kind !== "code")).toBe(true);
        const pdf = await invoke("files_search", { query: "atlas", kinds: ["pdf"], limit: 1, wordsOnly: true }) as FilesSearchResult;
        expect(pdf.hits).toHaveLength(1);
        expect(pdf.hits[0]).toMatchObject({ kind: "pdf", loc: 3 });
        expect(pdf.meaning).toBe(false);
        const code = await invoke("files_search", { query: "code: atlas" }) as FilesSearchResult;
        expect(code.hits).toHaveLength(1);
        expect(code.hits[0].kind).toBe("code");
        await expect(invoke("files_search", { query: "unmatchedxyz" })).resolves.toMatchObject({ hits: [] });
    });

    it("offers Files opt-in during onboarding and simulates model readiness", async () => {
        window.history.replaceState(null, "", "/ui-preview.html?onboarding=search_files");
        const invoke = createPreviewIpcHandler();
        await expect(invoke("get_onboarding_state")).resolves.toMatchObject({ step: "search_files" });
        const settings = await invoke("files_settings_get") as FilesSettings;
        expect(settings.enabled).toBe(false);
        await expect(invoke("files_status")).resolves.toMatchObject({ state: "off", model_ready: false });
        await invoke("files_settings_set", { settings: { ...settings, enabled: true } });
        await expect(invoke("files_status")).resolves.toMatchObject({ state: "needs_model" });
        await invoke("files_model_download");
        await expect(invoke("files_status")).resolves.toMatchObject({ state: "idle", model_ready: true });
    });

    it("previews only fixtures and rejects unknown commands and local paths", async () => {
        const invoke = createPreviewIpcHandler();
        const result = await invoke("files_search", { query: "atlas", kinds: ["pdf"] }) as FilesSearchResult;
        const hit: FileHit = result.hits[0];
        await expect(invoke("files_passage", { path: hit.path, segmentId: hit.segment_id })).resolves.toContain("Atlas");
        await expect(invoke("files_preview", { path: hit.path, kind: "pdf", page: 3 })).resolves.toMatch(/^data:image\//);
        await expect(invoke("files_open", { path: "/private/real.pdf" })).rejects.toThrow(/synthetic/);
        await expect(invoke("files_unknown")).rejects.toThrow(/Unhandled.*files_unknown/);
    });
});
