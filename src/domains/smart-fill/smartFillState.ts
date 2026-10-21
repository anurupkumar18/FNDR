/** Sent by the backend to the glow window (`smart_fill/overlay.rs`). */
export const SMART_FILL_STATE_EVENT = "smart-fill://state";

export type SmartFillPhase = "idle" | "thinking" | "writing" | "done" | "stopped" | "notice" | "error";

export interface SmartFillStateEvent {
    phase: Exclude<SmartFillPhase, "idle">;
    message: string | null;
}

/** The short label beside the caret. Notices and errors carry their own sentence. */
export function smartFillLabel(phase: SmartFillPhase, message: string | null): string {
    switch (phase) {
        case "thinking":
            return "Reading the screen";
        case "writing":
            return "Writing";
        case "done":
            return "Done";
        case "stopped":
            return "Stopped";
        case "notice":
        case "error":
            return message ?? "Smart Fill could not write here.";
        default:
            return "";
    }
}

/** Whether the line is a sentence for the person rather than a status word. */
export function isSmartFillMessage(phase: SmartFillPhase): boolean {
    return phase === "notice" || phase === "error";
}
