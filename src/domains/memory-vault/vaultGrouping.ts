import type { MemoryCard } from "@/shared/ipc/tauri";

/** Where a memory came from, for the Vault row icon. */
export type VaultSourceKind = "page" | "document" | "download" | "agent" | "screen";

/** One work session never spans a gap longer than this between two captures. */
export const SESSION_GAP_MS = 30 * 60 * 1000;

/** One visible Vault row: the newest memory of a session, with the session's
 *  older moments and near-duplicates folded under it. Display only; nothing
 *  stored changes. */
export interface VaultRow {
    lead: MemoryCard;
    similar: MemoryCard[];
}

/** Memories from one project, or one app when no project is known, on one day. */
export interface VaultThread {
    key: string;
    label: string;
    count: number;
    rows: VaultRow[];
}

export interface VaultDay {
    /** Local calendar day, `YYYY-MM-DD`. */
    key: string;
    label: string;
    count: number;
    threads: VaultThread[];
}

/** Newest first; equal timestamps fall back to id so the order never depends on input order. */
function newestFirst(a: MemoryCard, b: MemoryCard): number {
    return b.timestamp - a.timestamp || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
}

function localDayKey(date: Date): string {
    const month = String(date.getMonth() + 1).padStart(2, "0");
    const day = String(date.getDate()).padStart(2, "0");
    return `${date.getFullYear()}-${month}-${day}`;
}

function dayLabel(timestamp: number, now: number): string {
    const date = new Date(timestamp);
    const today = new Date(now);
    const yesterday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 1);
    const key = localDayKey(date);
    if (key === localDayKey(today)) return "Today";
    if (key === localDayKey(yesterday)) return "Yesterday";
    return date.toLocaleDateString(undefined, {
        weekday: "long",
        month: "short",
        day: "numeric",
        ...(date.getFullYear() === today.getFullYear() ? {} : { year: "numeric" }),
    });
}

/** Project when the memory has one; otherwise the app stands in for the capture session. */
function threadOf(card: MemoryCard): { key: string; label: string } {
    const project = card.project?.trim();
    if (project) return { key: `project:${project.toLowerCase()}`, label: project };
    const app = card.app_name.trim() || "Other";
    return { key: `app:${app.toLowerCase()}`, label: app };
}

function nearDuplicateKey(card: MemoryCard): string {
    const title = card.title.toLowerCase().replace(/[^\p{L}\p{N}]+/gu, " ").trim();
    return `${title}|${card.app_name.trim().toLowerCase()}`;
}

/**
 * Groups Vault memories by local day, then by thread, newest first. Within a
 * thread, captures no more than `SESSION_GAP_MS` apart form one session and
 * fold under its newest memory, as do near-duplicates (same normalized title
 * and app). Agent notes always stand alone. `now` only decides the Today and
 * Yesterday labels.
 */
export function groupVaultMemories(memories: MemoryCard[], now: number): VaultDay[] {
    const days = new Map<string, VaultDay>();
    const threads = new Map<string, VaultThread>();
    const rows = new Map<string, VaultRow>();
    const openSessions = new Map<string, { row: VaultRow; oldest: number }>();
    for (const card of [...memories].sort(newestFirst)) {
        const dayKey = localDayKey(new Date(card.timestamp));
        let day = days.get(dayKey);
        if (!day) {
            day = { key: dayKey, label: dayLabel(card.timestamp, now), count: 0, threads: [] };
            days.set(dayKey, day);
        }
        const { key, label } = threadOf(card);
        const threadKey = `${dayKey}\n${key}`;
        let thread = threads.get(threadKey);
        if (!thread) {
            thread = { key, label, count: 0, rows: [] };
            threads.set(threadKey, thread);
            day.threads.push(thread);
        }
        const rowKey = `${threadKey}\n${nearDuplicateKey(card)}`;
        const isNote = (card as MemoryCard & { source_type?: string }).source_type === "agent";
        const session = isNote ? undefined : openSessions.get(threadKey);
        const inSession = session !== undefined && session.oldest - card.timestamp <= SESSION_GAP_MS;
        const row = rows.get(rowKey) ?? (inSession ? session.row : undefined);
        if (row) {
            row.similar.push(card);
            if (inSession && row === session.row) session.oldest = card.timestamp;
        } else {
            const lead: VaultRow = { lead: card, similar: [] };
            rows.set(rowKey, lead);
            thread.rows.push(lead);
            if (!isNote) openSessions.set(threadKey, { row: lead, oldest: card.timestamp });
        }
        thread.count += 1;
        day.count += 1;
    }
    return [...days.values()];
}

/** Derives the row icon from fields the card already carries. */
export function vaultSourceKind(card: MemoryCard & { source_type?: string }): VaultSourceKind {
    if (card.source_type === "agent") return "agent";
    // Shape written by the downloads tracker (src-tauri/src/downloads.rs).
    if (card.app_name === "Finder" && card.window_title === "Downloads") return "download";
    const target = card.reopen_target?.trim() ?? "";
    if (card.url || /^https?:\/\//i.test(target)) return "page";
    if (/^file:\/\//i.test(target) || target.startsWith("/")) return "document";
    return "screen";
}
