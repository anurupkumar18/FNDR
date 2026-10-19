import { filesCopy, filesOpen, filesReveal, type FileHit } from "@/shared/ipc/files";
import { hitMoment, hitPage, queryWords } from "./fileHit";

/** Opens a found file in its app, at the page or moment that matched. */
export function openFileHit(hit: FileHit, query: string): Promise<string> {
    return filesOpen(hit.path, hitPage(hit), hitMoment(hit), queryWords(query));
}

export function revealFileHit(hit: FileHit): Promise<void> {
    return filesReveal(hit.path);
}

/** Puts the file itself on the pasteboard, to paste in Finder, Mail or a chat. */
export function copyFileHit(hit: FileHit): Promise<void> {
    return filesCopy(hit.path);
}
