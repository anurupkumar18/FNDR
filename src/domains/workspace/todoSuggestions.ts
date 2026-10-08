import { invoke } from "@tauri-apps/api/core";
import type { Task } from "@/shared/ipc/tauri";

/** A task FNDR proposed from a capture and the person has not accepted. */
export function isSuggestion(task: Task): boolean {
    return task.source_app.startsWith("Memory:") || task.source_app.toLowerCase() === "auto";
}

/** Where a task came from, in words a person would use. Empty for their own. */
export function sourceLabel(task: Task): string {
    const [kind, ...rest] = task.source_app.split(":");
    const name = rest.join(":").trim();
    if (kind === "Memory" || kind === "Accepted") return name ? `Seen in ${name}` : "";
    if (kind === "Meeting") return name ? `From meeting: ${name}` : "From a meeting";
    return "";
}

/** "just now", "12 min ago", "3 h ago", "yesterday", then the date. */
export function relativeTime(timestampMs: number, nowMs: number = Date.now()): string {
    const minutes = Math.floor(Math.max(0, nowMs - timestampMs) / 60_000);
    if (minutes < 1) return "just now";
    if (minutes < 60) return `${minutes} min ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours} h ago`;
    if (hours < 48) return "yesterday";
    return new Date(timestampMs).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** Make a suggestion the person's own task. Its wording and memory link stay. */
export async function acceptSuggestion(task: Task): Promise<Task> {
    return invoke<Task>("update_todo", {
        taskId: task.id,
        title: task.title,
        taskType: task.task_type,
        accept: true,
    });
}
