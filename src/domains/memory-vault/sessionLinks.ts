import type { VaultDay, VaultRow } from "./vaultGrouping";

/** Two sessions are in the same stretch of work when no more than this
 *  separates the end of one from the start of the other. */
export const LINK_GAP_MS = 10 * 60 * 1000;

/** Another session, in another thread, that the evidence ties to this one. */
export interface SessionLink {
    /** The other row's lead memory id. */
    leadId: string;
    /** The other row's thread: an app or a project. */
    label: string;
    moments: number;
}

/** Words that appear in window titles whatever the work is. */
const COMMON_TITLE_WORDS = new Set([
    "about", "chrome", "document", "google", "https", "inbox", "microsoft", "safari", "search", "settings",
    "untitled", "window", "workspace",
]);

interface Session {
    row: VaultRow;
    label: string;
    thread: string;
    start: number;
    end: number;
    files: Set<string>;
    titleWords: Set<string>;
}

function baseName(path: string): string {
    return path.trim().toLowerCase().split(/[\\/]/).pop() ?? "";
}

function describe(row: VaultRow, thread: string, label: string): Session {
    const moments = [row.lead, ...row.similar];
    const times = moments.map((card) => card.timestamp);
    const files = new Set<string>();
    const titleWords = new Set<string>();
    const appWords = new Set<string>();
    for (const card of moments) {
        for (const file of card.files_touched ?? []) files.add(baseName(file));
        for (const word of card.app_name.toLowerCase().split(/[^\p{L}\p{N}]+/u)) appWords.add(word);
    }
    for (const card of moments) {
        for (const word of `${card.title} ${card.window_title}`.toLowerCase().split(/[^\p{L}\p{N}]+/u)) {
            if (word.length >= 5 && !appWords.has(word) && !COMMON_TITLE_WORDS.has(word)) titleWords.add(word);
        }
    }
    files.delete("");
    return { row, label, thread, start: Math.min(...times), end: Math.max(...times), files, titleWords };
}

function shared(a: Set<string>, b: Set<string>): number {
    let count = 0;
    for (const item of a) if (b.has(item)) count += 1;
    return count;
}

/** Same stretch of time, and a file or two distinctive title words in common. */
function tied(a: Session, b: Session): boolean {
    const apart = Math.max(a.start, b.start) - Math.min(a.end, b.end);
    if (apart > LINK_GAP_MS) return false;
    return shared(a.files, b.files) >= 1 || shared(a.titleWords, b.titleWords) >= 2;
}

/**
 * For each row of a day, the sessions in other threads that belong to the
 * same stretch of work, newest first. Rows are linked, never merged: being
 * open at the same time is not evidence, a shared file or title is.
 */
export function linkSessions(day: VaultDay): Map<string, SessionLink[]> {
    const sessions = day.threads.flatMap((thread) => thread.rows.map((row) => describe(row, thread.key, thread.label)));
    const links = new Map<string, SessionLink[]>();
    for (const session of sessions) {
        const others = sessions
            .filter((other) => other.thread !== session.thread && tied(session, other))
            .sort((a, b) => b.end - a.end)
            .map((other) => ({
                leadId: other.row.lead.id,
                label: other.label,
                moments: other.row.similar.length + 1,
            }));
        if (others.length > 0) links.set(session.row.lead.id, others);
    }
    return links;
}
