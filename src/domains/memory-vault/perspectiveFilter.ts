import type { MemoryCard } from "@/shared/ipc/tauri";

export type Perspective = "web" | "coding" | "meetings" | "communication" | "docs";

const CODING_ACTIVITIES = new Set(["coding", "debugging", "testing_workflow"]);

/** Whether a card belongs to a Vault perspective.
 *
 *  The structured `activity_type` only ever confirms a match. The backend
 *  labels (coding, debugging, testing_workflow, planning, researching) do not
 *  cover every perspective, so any other label falls through to text signals
 *  instead of excluding the card. */
export function matchesPerspective(card: MemoryCard, perspective: Perspective): boolean {
    const activity = card.activity_type ?? "";
    if (perspective === "coding" && CODING_ACTIVITIES.has(activity)) return true;
    if (perspective === "web" && activity === "researching") return true;
    if (perspective === "communication" && activity === "communication") return true;

    const text = [
        card.window_title,
        ...(card.context ?? []),
        card.summary,
        card.display_summary,
        card.internal_context,
    ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();
    const hasAny = (terms: string[]) => terms.some((term) => text.includes(term));

    switch (perspective) {
        case "web":
            return Boolean(card.url);
        case "coding":
            return hasAny(["code", "debug", "build", "compile", "branch", "commit", "pull request", "repo"]);
        case "meetings":
            return hasAny(["meeting", "agenda", "call", "transcript", "attendee", "follow-up"]);
        case "communication":
            return hasAny(["message", "email", "chat", "inbox", "reply", "thread"]);
        case "docs":
            return hasAny(["doc", "document", "summary", "outline", "spec", "readme", "note", "draft", "pdf"]);
    }
}
