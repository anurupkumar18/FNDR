import { open as shellOpen } from "@tauri-apps/plugin-shell";

/** Opens a URL in the user's default browser, falling back to a new tab
 *  outside Tauri (browser preview). */
export async function openExternalUrl(url: string): Promise<void> {
    try {
        await shellOpen(url);
        return;
    } catch {
        // Not running inside Tauri.
    }
    window.open(url, "_blank", "noopener,noreferrer");
}
