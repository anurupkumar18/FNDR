import { useEffect, useState } from "react";
import { getWallpaperAuroraColors, isPaletteKey, type PaletteKey, type PaletteMode, resolveStoredPalette } from "@/shared/theme/cinematic-palettes";
import { STORAGE_KEYS } from "@/shared/utils/config";

/** Active cinematic palette + aurora triple (bg/mid/acc) for wallpaper shaders. */
export function useActiveCinematicPalette() {
    const [paletteKey, setPaletteKey] = useState<PaletteKey>(() => {
        const stored = localStorage.getItem(STORAGE_KEYS.palette);
        return resolveStoredPalette(stored);
    });
    const [mode, setMode] = useState<PaletteMode>(() =>
        localStorage.getItem(STORAGE_KEYS.theme) === "light" ? "light" : "dark"
    );

    useEffect(() => {
        const handler = (e: Event) => {
            const detail = (e as CustomEvent<{ palette?: PaletteKey; mode?: PaletteMode }>).detail;
            if (detail?.palette && isPaletteKey(detail.palette)) setPaletteKey(detail.palette);
            if (detail?.mode === "light" || detail?.mode === "dark") setMode(detail.mode);
        };
        window.addEventListener("fndr-appearance-changed", handler);
        return () => window.removeEventListener("fndr-appearance-changed", handler);
    }, []);

    return { paletteKey, mode, aurora: getWallpaperAuroraColors(paletteKey, mode) };
}
