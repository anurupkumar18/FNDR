import type { ReopenOutcome } from "@/shared/ipc/tauri";

export function reopenOutcomeMessage(outcome: ReopenOutcome): string {
    switch (outcome.kind) {
        case "opened":
            return "Opened.";
        case "opened_moved":
            return `File was moved. Opened it from ${outcome.new_path}.`;
        case "missing":
            return `This file no longer exists at ${outcome.path}. The memory is still here.`;
        case "drive_not_connected":
            return `Connect the drive ${outcome.volume} to open this file.`;
        case "app_missing":
            return `${outcome.app_name?.trim() || outcome.bundle_id} is no longer installed.`;
        case "app_only":
            return `Opened ${outcome.app_name?.trim() || "the app"}. No specific page or file was saved for this memory.`;
        case "blocked":
            return "FNDR does not open this kind of link.";
        case "no_target":
            return "Nothing to reopen for this memory.";
    }
}
