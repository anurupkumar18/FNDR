import type { Task, ThreadDigest, WorkItemOutcome, WorkSet, WorkSetLayout, WorkSetResolution } from "@/shared/ipc/tauri";
import { isSuggestion } from "@/domains/workspace/todoSuggestions";

const HOUR = 60 * 60 * 1000;

/** Tasks due within this window get a card on Home. */
export const DEADLINE_WINDOW_MS = 72 * HOUR;

/** A task overdue by more than this has slipped; it stays in To-dos only. */
const OVERDUE_GRACE_MS = 24 * HOUR;

/** ADR 027 item 4: up to three items open on the tap that asked; more wait for one more tap. */
export function asksBeforeOpening(itemCount: number): boolean {
    return itemCount > 3;
}

/**
 * The candidate set that belongs to this evidence: the one sharing the most
 * memories with it. A set sharing none is another thread, so none is picked
 * rather than a guess.
 */
export function pickSet(resolution: WorkSetResolution, evidence: string[]): WorkSet | null {
    const candidates = resolution.kind === "best" ? [resolution.value] : resolution.kind === "ambiguous" ? resolution.value : [];
    const wanted = new Set(evidence);
    let best: WorkSet | null = null;
    let bestShared = 0;
    for (const candidate of candidates) {
        const shared = candidate.items.filter((item) => wanted.has(item.memoryId)).length;
        if (shared > bestShared) {
            best = candidate;
            bestShared = shared;
        }
    }
    return best;
}

export type OutcomeStatus = "opened" | "moved" | "needs_permission" | "failed";

/** Read from FNDR's typed outcome, never from a report. */
export function outcomeStatus(outcome: WorkItemOutcome): OutcomeStatus {
    if (outcome.ok) return outcome.outcome?.kind === "opened_moved" ? "moved" : "opened";
    if (!outcome.outcome && /actions are turned off|is private right now|permission|accessibility/i.test(outcome.detail)) {
        return "needs_permission";
    }
    return "failed";
}

export const OUTCOME_LABEL: Record<OutcomeStatus, string> = {
    opened: "Opened",
    moved: "Moved, opened from its new place",
    needs_permission: "Needs permission",
    failed: "Not opened",
};

/** "Arrange side by side" picks the layout that fits the number of windows. */
export function layoutFor(itemCount: number): WorkSetLayout {
    if (itemCount <= 1) return "maximize";
    if (itemCount === 2) return "left_right_split";
    if (itemCount === 3) return "thirds";
    return "grid2x2";
}

/** The memories a task came from, its own first. */
export function taskMemoryIds(task: Task): string[] {
    return [...new Set([task.source_memory_id, ...task.linked_memory_ids].filter((id): id is string => Boolean(id)))];
}

/** Open tasks the person took on, due within 72 hours (or just missed), soonest first. */
export function upcomingDeadlines(tasks: Task[], nowMs: number): Task[] {
    return tasks
        .filter((task) => !task.is_completed && !task.is_dismissed && !isSuggestion(task))
        .filter((task): task is Task & { due_date: number } => (
            task.due_date !== null && task.due_date >= nowMs - OVERDUE_GRACE_MS && task.due_date <= nowMs + DEADLINE_WINDOW_MS
        ))
        .sort((a, b) => a.due_date - b.due_date);
}

function startOfDay(ms: number): number {
    const day = new Date(ms);
    day.setHours(0, 0, 0, 0);
    return day.getTime();
}

function daysBetween(fromMs: number, toMs: number): number {
    return Math.round((startOfDay(toMs) - startOfDay(fromMs)) / (24 * HOUR));
}

export function dueLabel(dueMs: number, nowMs: number): string {
    if (dueMs < nowMs) return "overdue";
    const days = daysBetween(nowMs, dueMs);
    if (days === 0) return "due today";
    if (days === 1) return "due tomorrow";
    return `due ${new Date(dueMs).toLocaleDateString("en-US", { weekday: "long" })}`;
}

function count(n: number, one: string, many: string): string {
    return `${n} ${n === 1 ? one : many}`;
}

/** "Lab 4 due tomorrow, 3 items ready to reopen". */
export function deadlineHeadline(task: Task, readyItems: number, nowMs: number): string {
    const due = `${task.title} ${dueLabel(task.due_date ?? nowMs, nowMs)}`;
    return readyItems > 0 ? `${due}, ${count(readyItems, "item", "items")} ready to reopen` : due;
}

function sinceLabel(digest: ThreadDigest, nowMs: number): string {
    if (digest.first_view) return "in the last day";
    const days = daysBetween(digest.since_ms, nowMs);
    if (days === 0) {
        return `since ${new Date(digest.since_ms).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" })}`;
    }
    if (days === 1) return "since yesterday";
    return `since ${new Date(digest.since_ms).toLocaleDateString("en-US", { month: "short", day: "numeric" })}`;
}

/** "3 new pages, 1 task since yesterday"; null when nothing is new. */
export function changeLine(digest: ThreadDigest, nowMs: number): string | null {
    const parts = [
        digest.page_count > 0 && count(digest.page_count, "page", "pages"),
        digest.file_count > 0 && count(digest.file_count, "file", "files"),
        digest.task_count > 0 && count(digest.task_count, "task", "tasks"),
        digest.commit_count > 0 && count(digest.commit_count, "commit", "commits"),
    ].filter((part): part is string => Boolean(part));
    if (parts.length === 0) {
        if (digest.new_memories === 0) return null;
        parts.push(count(digest.new_memories, "capture", "captures"));
    }
    parts[0] = parts[0].replace(/^(\d+) /, "$1 new ");
    return `${parts.join(", ")} ${sinceLabel(digest, nowMs)}`;
}
