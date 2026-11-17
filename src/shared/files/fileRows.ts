import type { FileHit, FileKind } from "@/shared/ipc/files";
import { kindLabel } from "./fileHit";

/** Rows shown per kind before "Show all"; code is one group, so it gets the room of three. */
export const ROWS_PER_GROUP = 6;
export const CODE_ROWS = ROWS_PER_GROUP * 3;

export type FileRow =
    | { type: "hit"; hit: FileHit; weaker: boolean; heading?: string }
    | { type: "more"; kind: FileKind; count: number; heading?: undefined }
    | { type: "weaker"; count: number; open: boolean; heading?: undefined };

const PLURALS: Record<FileKind, string> = {
    screenshot: "screenshots",
    image: "pictures",
    pdf: "PDFs",
    doc: "documents",
    audio: "recordings",
    video: "videos",
    code: "code results",
};

export function kindPlural(kind: FileKind): string {
    return PLURALS[kind];
}

/**
 * Quick Find's file rows, as DigUp's search panel lays them out: the strong
 * matches grouped by kind (the group of the best match first), a few rows
 * each with a "Show all" row for the rest, then one row that folds the
 * weaker matches open or shut.
 */
export function fileRows(hits: FileHit[], strong: number, showWeaker: boolean): FileRow[] {
    const groups = new Map<FileKind, FileHit[]>();
    for (const hit of hits.slice(0, strong)) {
        const group = groups.get(hit.kind);
        if (group) group.push(hit);
        else groups.set(hit.kind, [hit]);
    }
    const rows: FileRow[] = [];
    const several = groups.size > 1;
    for (const [kind, group] of groups) {
        const room = kind === "code" ? CODE_ROWS : ROWS_PER_GROUP;
        group.slice(0, room).forEach((hit, index) =>
            rows.push({ type: "hit", hit, weaker: false, heading: several && index === 0 ? kindLabel(kind) : undefined }),
        );
        if (group.length > room) rows.push({ type: "more", kind, count: group.length });
    }
    const weaker = hits.slice(strong);
    if (weaker.length) {
        rows.push({ type: "weaker", count: weaker.length, open: showWeaker });
        if (showWeaker) for (const hit of weaker) rows.push({ type: "hit", hit, weaker: true });
    }
    return rows;
}
