import type { MemoryCard } from "@/shared/ipc/tauri";
import type { VaultRow } from "./vaultGrouping";

/** What a session row says about the whole session, composed from its own
 *  moments. No model writes it: every part is a sentence or a count a moment
 *  already carries. */
export interface SessionDigest {
    moments: number;
    /** Whole minutes from the first moment to the last. */
    minutes: number;
    /** Distinct files across the moments. */
    files: number;
    /** The most detailed sentence of an earlier moment, when it says something
     *  the row's own line does not. */
    earlier?: string;
}

/** A line written from the title when nothing better was known. */
function saysLittle(line: string): boolean {
    return (
        line.length === 0 ||
        /^(viewed|used|captured)\b.*\bat \d{1,2}:\d{2}\b/i.test(line) ||
        /^captured recent activity/i.test(line) ||
        /^used [^.]+\.$/i.test(line)
    );
}

function words(line: string): Set<string> {
    return new Set(
        line
            .toLowerCase()
            .split(/[^\p{L}\p{N}]+/u)
            .filter((word) => word.length >= 4),
    );
}

/** Whether `line` adds a fact `known` does not hold: at least three words of its own. */
function addsSomething(line: string, known: Set<string>): boolean {
    let fresh = 0;
    for (const word of words(line)) if (!known.has(word)) fresh += 1;
    return fresh >= 3;
}

/**
 * The digest of a session row, or null for a row with a single moment.
 * `lineOf` is the line a card shows, so the digest never surfaces text a
 * card would hide.
 */
export function sessionDigest(row: VaultRow, lineOf: (card: MemoryCard) => string): SessionDigest | null {
    if (row.similar.length === 0) return null;
    const moments = [row.lead, ...row.similar];
    const times = moments.map((card) => card.timestamp);
    const files = new Set<string>();
    for (const card of moments) for (const file of card.files_touched ?? []) files.add(file.trim().toLowerCase());
    files.delete("");

    const shown = words(`${row.lead.title} ${lineOf(row.lead)}`);
    let earlier: string | undefined;
    let most = 0;
    // Newest first, so equal detail keeps the later moment.
    for (const card of row.similar) {
        const line = lineOf(card).trim();
        if (saysLittle(line) || !addsSomething(line, shown)) continue;
        const detail = words(line).size;
        if (detail > most) {
            most = detail;
            earlier = line;
        }
    }
    return {
        moments: moments.length,
        minutes: Math.round((Math.max(...times) - Math.min(...times)) / 60_000),
        files: files.size,
        earlier,
    };
}

/** "11 moments over 42 min, 3 files" */
export function sessionDigestLabel(digest: SessionDigest): string {
    const parts = [`${digest.moments} moments`];
    if (digest.minutes >= 1) parts[0] += ` over ${digest.minutes} min`;
    if (digest.files > 0) parts.push(`${digest.files} ${digest.files === 1 ? "file" : "files"}`);
    return parts.join(", ");
}
