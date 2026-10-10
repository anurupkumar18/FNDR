/**
 * Every place that reads memories in bulk is listed here with how it keeps a
 * hidden memory hidden. Search, the Vault and Ask apply one rule
 * (`context_runtime::retrieve`: blocklist, soft delete, quality gate). On
 * 2026-10-09 four other surfaces were found reading the store directly and
 * showing memories from apps blocked since the capture: the memory toast,
 * the briefing, the Daily Summary and usage statistics.
 *
 * A new file that calls one of these store methods fails this test until it
 * is added below with its reason. So does an entry whose file no longer
 * reads, so the list stays true.
 */
// Vitest runs this in Node; the app tsconfig leaves Node's types out on purpose.
// @ts-expect-error -- node:fs exists in the test runtime.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { describe, expect, it } from "vitest";

const ROOT: string = (globalThis as typeof globalThis & { process: { cwd: () => string } }).process.cwd();
const SRC = "src-tauri/src";

/** Store methods that return many memories at once. */
const BULK_READS =
    /\.(list_all_memories|list_recent_results|get_memories_in_range|get_search_results_in_range|get_recent_memories|list_recent_by_session_or_project|vector_search|snippet_vector_search|image_vector_search|chunk_vector_search|chunk_keyword_search|keyword_search)\(/;

/** Why each reader is safe. "shown" readers name the check they apply. */
const READERS: Record<string, string> = {
    // The shared retrieval path: results are authorized in `retrieve`.
    "context_runtime/chunk_route.rs": "route of the shared retrieval path",
    "context_runtime/keyword_route.rs": "route of the shared retrieval path",
    "context_runtime/temporal_route.rs": "route of the shared retrieval path",
    "context_runtime/vector_route.rs": "route of the shared retrieval path",
    "context_runtime/mod.rs": "shown: authorizes direct reads before projection",
    // Shown to the person.
    "briefing.rs": "shown: result_is_permitted and the quality gate",
    "ipc/commands/search.rs": "shown: authorize_direct_results",
    "ipc/commands/stats.rs": "shown: result_is_permitted or memory_is_permitted on every read",
    "ipc/commands/quality.rs": "shown in diagnostics: memory_is_visible or memory_is_permitted",
    "main.rs": "shown: the toast goes through proactive::pick_suggestion; the decay job shows nothing",
    "resume/mod.rs": "shown: memory_is_visible",
    "mcp/mod.rs": "served to agents: load_memories_for_results (memory_is_visible); the health check reads one timestamp",
    // Never shown as read: linking, repair, review and indexing.
    "capture/clipboard.rs": "internal: links a clipboard copy to the newest capture",
    "capture/mod.rs": "internal: duplicate and merge detection at capture",
    "downloads.rs": "internal: links a download to its source capture",
    "graph/legacy.rs": "internal: graph edges between captures",
    "ipc/commands/graph.rs": "internal: graph backfill",
    "ipc/commands/maintenance.rs": "internal: reindex and repair",
    "memory_review/backfill.rs": "internal: review queue",
    "memory_review/daily.rs": "internal: review queue",
    "memory_review/pipeline.rs": "internal: review queue",
    "memory_review/repair_truncated.rs": "internal: summary repair",
};

function rustFiles(dir: string): string[] {
    return (readdirSync(`${ROOT}/${dir}`) as string[]).flatMap((name) => {
        const path = `${dir}/${name}`;
        if (statSync(`${ROOT}/${path}`).isDirectory()) return rustFiles(path);
        return name.endsWith(".rs") ? [path] : [];
    });
}

/** The storage layer defines the reads; test-only files may call anything. */
function governed(path: string): boolean {
    return !path.startsWith(`${SRC}/storage/`) && !/(^|\/)tests\.rs$|_tests\.rs$/.test(path);
}

const readers = rustFiles(SRC)
    .filter(governed)
    .filter((path) => BULK_READS.test(readFileSync(`${ROOT}/${path}`, "utf8") as string))
    .map((path) => path.slice(SRC.length + 1))
    .sort();

describe("bulk memory reads", () => {
    it("every reader is listed with how it keeps hidden memories hidden", () => {
        expect(readers.filter((path) => !(path in READERS))).toEqual([]);
    });

    it("the list names only files that still read", () => {
        expect(Object.keys(READERS).filter((path) => !readers.includes(path))).toEqual([]);
    });
});
