import {
    applyPalette,
    isPaletteKey,
    type PaletteMode,
} from "@/shared/theme/cinematic-palettes";
import { STORAGE_KEYS } from "@/shared/utils/config";

/**
 * Keep pre-created auxiliary windows visually aligned with the main FNDR shell.
 * Re-syncing on focus covers windows that stayed hidden while appearance changed.
 */
export function syncAuxiliaryAppearance(): void {
    const storedTheme = localStorage.getItem(STORAGE_KEYS.theme) as PaletteMode | null;
    const theme: PaletteMode = storedTheme === "light" ? "light" : "dark";
    const storedPalette = localStorage.getItem(STORAGE_KEYS.palette);

    document.documentElement.setAttribute("data-theme", theme);
    applyPalette(isPaletteKey(storedPalette) ? storedPalette : "matrix", theme);
}

export function installAuxiliaryAppearanceSync(): () => void {
    const sync = () => syncAuxiliaryAppearance();
    sync();
    window.addEventListener("focus", sync);
    window.addEventListener("storage", sync);
    return () => {
        window.removeEventListener("focus", sync);
        window.removeEventListener("storage", sync);
    };
}
