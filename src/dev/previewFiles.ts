import {
    FILES_DOWNLOAD_EVENT,
    FILES_OPEN_SEARCH_EVENT,
    FILES_STATUS_EVENT,
    type FileHit,
    type FileKind,
    type FilesSettings,
    type FilesStatus,
} from "@/shared/ipc/files";

const ROOT = "/Users/preview";
const DOCUMENTS = `${ROOT}/Documents`;
const CODE = `${ROOT}/Projects/atlas`;
const now = Date.parse("2026-11-17T12:00:00Z") / 1000;
const fixtures: FileHit[] = [
    { path: `${DOCUMENTS}/atlas-plan.pdf`, kind: "pdf", segment: "page", loc: 3, excerpt: "Atlas planning: the launch review covered the search experience." },
    { path: `${DOCUMENTS}/atlas-notes.md`, kind: "doc", segment: "chunk", loc: null, excerpt: "Atlas notes: reviewed the browser preview and keyboard navigation." },
    { path: `${ROOT}/Pictures/atlas-mountains.jpg`, kind: "image", segment: "image", loc: null, excerpt: "Atlas mountains beside a blue lake." },
    { path: `${DOCUMENTS}/atlas-review.m4a`, kind: "audio", segment: "audio", loc: 42, excerpt: "Atlas review discussed the launch plan." },
    { path: `${DOCUMENTS}/atlas-demo.mov`, kind: "video", segment: "frame", loc: 72, excerpt: "Atlas demo showed the file search screen." },
    { path: `${CODE}/src/search.ts`, kind: "code", segment: "lines", loc: 12, excerpt: "export function searchAtlasFiles(query: string)" },
].map((hit, index) => ({
    ...hit,
    score: 0.95 - index * 0.03,
    z: 3,
    cosine: 0.8,
    keyword_rank: index + 1,
    segment_id: index + 1,
    loc_end: hit.loc === null ? null : hit.loc + 8,
    modified: now,
    size: 120_000,
    info: null,
    focus: hit.kind === "code" ? 12 : null,
})) as FileHit[];

const picture = `data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="480" height="320" viewBox="0 0 480 320"><rect width="480" height="320" fill="#cde4eb"/><path d="M0 240 120 60 260 240 360 100 480 240" fill="#527b76"/><rect y="240" width="480" height="80" fill="#80b8cf"/><text x="24" y="290" font-family="sans-serif" font-size="20" fill="#173b4b">Atlas synthetic preview</text></svg>')}`;

/** In-memory fixtures only: no disk access, downloads, clipboard or app launches. */
export function createPreviewFilesHandler(emit: (event: string, payload: unknown) => Promise<void>) {
    const onboarding = new URLSearchParams(globalThis.location?.search ?? "").has("onboarding");
    let settings: FilesSettings = {
        enabled: !onboarding,
        folders: [DOCUMENTS, `${ROOT}/Pictures`],
        excluded_folders: [],
        excluded_types: "",
        code_folders: [CODE],
        code_editor: null,
        include_datasets: false,
        paused: false,
        hours: { kind: "always" },
        index_on_battery: true,
    };
    let modelReady = !onboarding;
    let quickLookPath: string | null = null;
    const indexed = () => fixtures.filter((hit) =>
        (hit.kind === "code" ? settings.code_folders : settings.folders).some((folder) => hit.path.startsWith(`${folder}/`))
        && !settings.excluded_folders.some((folder) => hit.path.startsWith(`${folder}/`)),
    );
    const status = (): FilesStatus => {
        const files = settings.enabled ? indexed() : [];
        const counts = (rows: FileHit[]) => ({ files: rows.length, done: modelReady ? rows.length : 0, pending: modelReady ? 0 : rows.length, failed: 0, vectors: modelReady ? rows.length : 0 });
        return {
            state: !settings.enabled ? "off" : !modelReady ? "needs_model" : settings.paused ? "paused" : "idle",
            detail: null,
            counts: counts(files),
            code_counts: counts(files.filter((hit) => hit.kind === "code")),
            code_bytes: 120_000,
            model_ready: modelReady,
            helper_found: true,
            reader_found: true,
        };
    };
    const changed = async () => {
        const current = status();
        await emit(FILES_STATUS_EVENT, current);
        return current;
    };

    return async (command: string, payload?: unknown): Promise<unknown> => {
        const args = (payload ?? {}) as Record<string, unknown>;
        const fixture = () => {
            const hit = fixtures.find((row) => row.path === args.path);
            if (!hit) throw new Error("Files preview accepts only synthetic fixture paths.");
            return hit;
        };
        switch (command) {
            case "files_settings_get":
                return structuredClone(settings);
            case "files_settings_set": {
                const next = args.settings as FilesSettings | undefined;
                if (!next || typeof next.enabled !== "boolean" || !Array.isArray(next.folders) || !Array.isArray(next.code_folders)) {
                    throw new Error("Preview files_settings_set requires complete settings.");
                }
                settings = structuredClone(next);
                return changed();
            }
            case "files_status":
                return status();
            case "files_search": {
                if (typeof args.query !== "string") throw new Error("Preview files_search requires query.");
                const code = /^\s*code:/i.test(args.query);
                const text = args.query.replace(/^\s*(file|code):\s*/i, "").trim().toLowerCase();
                const words = text.split(/\W+/).filter(Boolean);
                const kinds = args.kinds as FileKind[] | undefined;
                const limit = typeof args.limit === "number" ? Math.max(0, Math.floor(args.limit)) : 40;
                const hits = settings.enabled && text
                    ? indexed().filter((hit) => (hit.kind === "code") === code
                        && (!kinds?.length || kinds.includes(hit.kind))
                        && words.some((word) => `${hit.path} ${hit.excerpt}`.toLowerCase().includes(word))).slice(0, limit)
                    : [];
                return { hits: structuredClone(hits), strong: hits.length, meaning: modelReady && args.wordsOnly !== true };
            }
            case "files_suggested_folders":
                return [DOCUMENTS, `${ROOT}/Pictures`, `${ROOT}/Downloads`];
            case "files_suggested_code_folders":
                return [CODE];
            case "files_pick_folder":
                return `${ROOT}/Downloads`;
            case "files_estimate": {
                const folders = (args.folders as string[] ?? []).map((folder) => ({
                    folder, files: { pdf: 1, doc: 1, audio: 1, video: 1 }, seconds: 45, later_seconds: 90, skipped: {},
                }));
                return { folders, seconds: folders.length * 45, later_seconds: folders.length * 90 };
            }
            case "files_folders":
                return settings.folders.map((folder) => {
                    const rows = indexed().filter((hit) => hit.path.startsWith(`${folder}/`));
                    return { folder, reach: "ok", files: rows.length, done: modelReady ? rows.length : 0, kinds: Object.fromEntries(rows.map((hit) => [hit.kind, 1])) };
                });
            case "files_subfolders":
                return [];
            case "files_editors":
                return [{ id: "com.microsoft.VSCode", name: "Visual Studio Code" }];
            case "files_model_download":
            case "files_model_import":
                modelReady = true;
                await emit(FILES_DOWNLOAD_EVENT, { bytes: 865_000_000, total: 865_000_000, done: true });
                await changed();
                return command === "files_model_import" ? ["synthetic-model.gguf"] : null;
            case "files_model_links":
                return ["https://example.test/synthetic-model.gguf"];
            case "files_forget":
                settings = { ...settings, enabled: false };
                if (args.model === true) modelReady = false;
                return changed();
            case "files_thumbnails":
                return Object.fromEntries(fixtures.filter((hit) => (args.paths as string[] ?? []).includes(hit.path)).map((hit) => [hit.path, picture]));
            case "files_preview":
                fixture();
                return picture;
            case "files_passage":
                return fixture().excerpt;
            case "files_clip":
                fixture();
                return "data:audio/wav;base64,UklGRiQAAABXQVZFZm10IBAAAAABAAEAgD4AAAB9AAACABAAZGF0YQAAAAA=";
            case "files_open":
                return fixture().kind === "pdf" ? "preview" : "default";
            case "files_open_code":
                fixture();
                return "Visual Studio Code";
            case "files_reveal":
            case "files_copy":
            case "files_drag":
                fixture();
                return null;
            case "files_quick_look":
                if (!args.path) quickLookPath = null;
                else {
                    const hit = fixture();
                    quickLookPath = args.follow || quickLookPath !== hit.path ? hit.path : null;
                }
                return quickLookPath !== null;
            case "files_open_in_window":
                await emit(FILES_OPEN_SEARCH_EVENT, { query: String(args.query ?? ""), kinds: args.kinds ?? null });
                return null;
            case "files_open_privacy_settings":
                return null;
            default:
                throw new Error(`Unhandled UI preview command: ${command}`);
        }
    };
}
