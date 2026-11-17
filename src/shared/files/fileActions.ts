import { filesCopy, filesOpen, filesOpenCode, filesReveal, type FileHit } from "@/shared/ipc/files";
import { hitLine, hitMoment, hitPage, queryWords } from "./fileHit";

/** Opens a found file in its app, at the page or moment that matched; code in the chosen editor at its line. */
export function openFileHit(hit: FileHit, query: string): Promise<string> {
    if (hit.kind === "code") {
        return filesOpenCode(hit.path, hitLine(hit) ?? 1);
    }
    return filesOpen(hit.path, hitPage(hit), hitMoment(hit), queryWords(query));
}

export function revealFileHit(hit: FileHit): Promise<void> {
    return filesReveal(hit.path);
}

/** Puts the file itself on the pasteboard, to paste in Finder, Mail or a chat. */
export function copyFileHit(hit: FileHit): Promise<void> {
    return filesCopy(hit.path);
}
