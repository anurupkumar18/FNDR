import { type PaletteKey, type PaletteMode, resolveStoredPalette } from "@/shared/theme/cinematic-palettes";

export interface PreviewConfig {
    theme: PaletteMode;
    palette: PaletteKey;
    motion: "on" | "off";
}

/** Deterministic, privacy-safe defaults for browser UI review. */
export function resolvePreviewConfig(search: string): PreviewConfig {
    const params = new URLSearchParams(search);
    const requestedTheme = params.get("theme");
    const requestedPalette = params.get("palette");

    return {
        theme: requestedTheme === "light" ? "light" : "dark",
        palette: resolveStoredPalette(requestedPalette),
        motion: params.get("motion") === "on" ? "on" : "off",
    };
}
