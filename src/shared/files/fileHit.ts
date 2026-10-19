import type { FileHit, FileKind, FilesStatus } from "@/shared/ipc/files";

/** `file: ...` searches files only; `code: ...` the code index only (ADR 033). */
export function isFileQuery(query: string): boolean {
    return /^\s*(file|code):/i.test(query);
}

/** The file's name, without its folders. */
export function fileName(path: string): string {
    return path.split("/").filter(Boolean).pop() ?? path;
}

/** The folder it is in, home shortened to ~. */
export function fileFolder(path: string, home?: string): string {
    const folder = path.split("/").slice(0, -1).join("/") || "/";
    return home && folder.startsWith(home) ? `~${folder.slice(home.length)}` : folder;
}

/** The PDF page that matched, from 1. */
export function hitPage(hit: FileHit): number | undefined {
    if (hit.kind !== "pdf" || (hit.segment !== "page" && hit.segment !== "page_image") || hit.loc == null) {
        return undefined;
    }
    return Math.round(hit.loc);
}

/** Seconds into a recording or video where the match starts. */
export function hitMoment(hit: FileHit): number | undefined {
    if ((hit.kind !== "audio" && hit.kind !== "video") || (hit.segment !== "audio" && hit.segment !== "frame")) {
        return undefined;
    }
    return hit.loc ?? undefined;
}

/** 0:06, 12:40, 1:02:03. */
export function clock(seconds: number): string {
    const whole = Math.max(0, Math.floor(seconds));
    const hours = Math.floor(whole / 3600);
    const minutes = Math.floor((whole % 3600) / 60);
    const rest = String(whole % 60).padStart(2, "0");
    return hours > 0 ? `${hours}:${String(minutes).padStart(2, "0")}:${rest}` : `${minutes}:${rest}`;
}

/** Code: the line to open at, from 1. */
export function hitLine(hit: FileHit): number | undefined {
    if (hit.kind !== "code" || hit.segment !== "lines") return undefined;
    return Math.max(1, Math.round(hit.focus ?? hit.loc ?? 1));
}

/** Where in the file the match is ("p. 3", "at 0:25", "line 42"), or nothing for the whole file. */
export function hitPlace(hit: FileHit): string | null {
    const line = hitLine(hit);
    if (line !== undefined) return `line ${line}`;
    if (hit.kind === "code" && hit.segment === "cells" && hit.loc != null) return `cell ${Math.round(hit.loc)}`;
    const page = hitPage(hit);
    if (page !== undefined) return `p. ${page}`;
    const moment = hitMoment(hit);
    if (moment !== undefined) return `at ${clock(moment)}`;
    return null;
}

const KIND_LABELS: Record<FileKind, string> = {
    screenshot: "Screenshot",
    image: "Picture",
    pdf: "PDF",
    doc: "Document",
    audio: "Recording",
    video: "Video",
    code: "Code",
};

export function kindLabel(kind: FileKind): string {
    return KIND_LABELS[kind];
}

/** Kinds with a picture preview. */
export function hasPreview(kind: FileKind): boolean {
    return kind === "image" || kind === "screenshot" || kind === "pdf" || kind === "video";
}

/** The excerpt with the words that matched, split for marking: «marked» parts come back flagged. */
export function excerptParts(excerpt: string): { text: string; marked: boolean }[] {
    const parts: { text: string; marked: boolean }[] = [];
    for (const piece of excerpt.split(/(«[^»]*»)/)) {
        if (!piece) continue;
        const marked = piece.startsWith("«") && piece.endsWith("»");
        parts.push({ text: marked ? piece.slice(1, -1) : piece, marked });
    }
    return parts;
}

/** The query's words, for a PDF to open with one of them in Preview's search. */
export function queryWords(query: string): string[] {
    return query
        .replace(/^\s*(file|code):/i, "")
        .split(/[^\p{L}\p{M}\p{N}]+/u)
        .filter((word) => word.length >= 3);
}

/** "1,204 files" and "1 file". */
function files(count: number): string {
    return `${count.toLocaleString("en-US")} ${count === 1 ? "file" : "files"}`;
}

/**
 * Why file results may be incomplete right now, or null: DigUp's status line.
 * `phase` is the search's ("words" while typing, "words_only" when the full
 * search had no model).
 */
export function filesNote(phase: "words" | "meaning" | "words_only" | null, status: FilesStatus | null): string | null {
    if (phase === "words") return "Matching words…";
    if (phase === "words_only") {
        return status && !status.model_ready
            ? "Matching words only: the Files model isn’t downloaded"
            : "Words only: meaning search isn’t available";
    }
    if (!status) return null;
    const { done, pending } = status.counts;
    if (status.state === "indexing" && pending > 0) {
        return `Still indexing (${Math.floor((done / (done + pending)) * 100)}%): some files aren’t in yet`;
    }
    if (pending > 0) return `${files(pending)} ${pending === 1 ? "isn’t" : "aren’t"} indexed yet`;
    return null;
}

/** What Enter does with this hit: "Open at 0:06", "Open p. 3", "Open in Cursor". */
export function openTitle(hit: FileHit, editor?: string | null): string {
    if (hit.kind === "code") return editor ? `Open in ${editor}` : "Open";
    const page = hitPage(hit);
    if (page !== undefined) return `Open at p. ${page}`;
    const moment = hitMoment(hit);
    if (moment !== undefined) return `Open at ${clock(moment)}`;
    return "Open";
}

/** The z-score above which a hit matched by meaning, as the searcher gates it. */
const MEANING_GATE = 2;

/** Why the hit is here, DigUp's line under the preview: "Matches the frame at 0:06", "Has your words". */
export function matchReason(hit: FileHit): string {
    const meaning = hit.z !== null && hit.z >= MEANING_GATE;
    if (!meaning) return hit.keyword_rank !== null ? "Has your words" : "Matches its name";
    const place = hit.loc !== null ? clock(hit.loc) : "";
    const page = hit.loc !== null ? `page ${Math.round(hit.loc)}` : "a page";
    switch (hit.segment) {
        case "image":
            return hit.kind === "screenshot" ? "Matches what’s on screen" : "Matches the picture";
        case "ocr":
            return "Matches the text in it";
        case "frame":
            return `Matches the frame at ${place}`;
        case "audio":
            return hit.kind === "video" ? `Matches the sound at ${place}` : `Matches what’s heard at ${place}`;
        case "page":
            return `Matches the text on ${page}`;
        case "page_image":
            return `Matches ${page}`;
        case "chunk":
            return "Matches its text";
        case "name":
            return "Matches its name";
        case "lines":
        case "cells":
            return hitPlace(hit) ? `Matches ${hitPlace(hit)}` : "Matches the code";
    }
}

const LANGUAGES: Record<string, string> = {
    py: "Python", ts: "TypeScript", tsx: "TypeScript", js: "JavaScript", jsx: "JavaScript", rs: "Rust", go: "Go",
    swift: "Swift", java: "Java", kt: "Kotlin", rb: "Ruby", c: "C", h: "C", cpp: "C++", cc: "C++", hpp: "C++",
    cs: "C#", php: "PHP", sh: "Shell", sql: "SQL", ipynb: "Notebook", m: "Objective-C", scala: "Scala", lua: "Lua",
};

function bytes(size: number): string {
    if (size < 1000) return `${size} bytes`;
    if (size < 1_000_000) return `${Math.round(size / 1000)} KB`;
    if (size < 1_000_000_000) return `${(size / 1_000_000).toFixed(1).replace(/\.0$/, "")} MB`;
    return `${(size / 1_000_000_000).toFixed(1).replace(/\.0$/, "")} GB`;
}

function count(value: unknown): number | null {
    return typeof value === "number" && Number.isFinite(value) ? value : null;
}

/**
 * "Screenshot · Oct 1, 2026 · 1440 × 900 · 2.1 MB", "PDF · Oct 1, 2026 ·
 * 463 pages, 120 read so far · 23 MB", "Python · 63 lines · Oct 1, 2026 · 2 KB".
 */
export function fileDetails(hit: FileHit): string {
    const info = hit.info ?? {};
    const date = new Date(hit.modified * 1000).toLocaleDateString("en-US", {
        month: "short",
        day: "numeric",
        year: "numeric",
    });
    const parts: string[] = [];
    if (hit.kind === "code") {
        const extension = hit.path.split(".").pop()?.toLowerCase() ?? "";
        parts.push(LANGUAGES[extension] ?? "Code");
        const cells = count(info.cells);
        const lines = count(info.lines);
        if (cells !== null) parts.push(cells === 1 ? "1 cell" : `${cells} cells`);
        else if (lines !== null) parts.push(lines === 1 ? "1 line" : `${lines.toLocaleString("en-US")} lines`);
        parts.push(date);
    } else {
        parts.push(kindLabel(hit.kind), date);
        const width = count(info.width);
        const height = count(info.height);
        if (width !== null && height !== null) parts.push(`${width} × ${height}`);
        const pages = count(info.pages);
        const readTo = count(info.read_to);
        if (pages !== null) {
            parts.push((pages === 1 ? "1 page" : `${pages} pages`) + (readTo !== null ? `, ${readTo} read so far` : ""));
        } else if (readTo !== null) {
            parts.push("partly read so far");
        }
        const duration = count(info.duration);
        if (duration !== null) parts.push(clock(duration));
    }
    if (hit.size > 0) parts.push(bytes(hit.size));
    return parts.join(" · ");
}
