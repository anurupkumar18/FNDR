import { afterEach, describe, expect, it } from "vitest";
import {
    applyPalette,
    getWallpaperAuroraColors,
    getWallpaperInkColors,
    PALETTES,
    removePalette,
    rgbToHex, resolveStoredPalette } from "../cinematic-palettes";

describe("applyPalette", () => {
    afterEach(() => {
        removePalette();
    });

    it("injects --cp-bg from the selected palette onto :root", () => {
        applyPalette("bladeRunner2049", "dark");

        const style = document.getElementById("cinematic-palette-vars");
        expect(style?.textContent).toContain('--cp-bg: #000000');
        expect(style?.textContent).toContain('--cp-active-palette: "bladeRunner2049"');
    });

    it("updates tokens when switching palettes", () => {
        applyPalette("system", "dark");
        applyPalette("matrix", "dark");

        const style = document.getElementById("cinematic-palette-vars");
        expect(style?.textContent).toContain('--cp-bg: #000000');
        expect(style?.textContent).toContain('--cp-accent: #00ff41');
    });

    it("injects wall ink from the active theme tokens", () => {
        applyPalette("matrix", "dark");

        const style = document.getElementById("cinematic-palette-vars");
        const ink = getWallpaperInkColors("matrix", "dark");
        expect(ink.primary).toBe(PALETTES.matrix.dark.textPrimary);
        expect(style?.textContent).toContain(`--cp-wall-text-primary: ${ink.primary}`);
    });

    it("injects exact palette-derived wall swatch hex for CSS fallback", () => {
        applyPalette("bladeRunner2049", "dark");

        const style = document.getElementById("cinematic-palette-vars");
        const aurora = getWallpaperAuroraColors("bladeRunner2049", "dark");
        expect(style?.textContent).toContain(`--cp-wall-bg: ${rgbToHex(aurora.bg)}`);
        expect(style?.textContent).toContain(`--cp-wall-acc: ${rgbToHex(aurora.acc)}`);
    });
});

describe("getWallpaperInkColors", () => {
    it("uses dark-mode text on black wallpaper void", () => {
        const ink = getWallpaperInkColors("bladeRunner2049", "dark");
        expect(ink.primary).toBe(PALETTES.bladeRunner2049.dark.textPrimary);
    });

    it("uses light-mode text on paper wallpaper void", () => {
        const ink = getWallpaperInkColors("grandBudapestHotel", "light");
        expect(ink.primary).toBe(PALETTES.grandBudapestHotel.light.textPrimary);
    });

    it("uses light-mode UI ink when theme is light", () => {
        const ink = getWallpaperInkColors("system", "light");
        expect(ink.primary).toBe(PALETTES.system.light.textPrimary);
    });

    it("defaults to System and retires the amber Old Film palette", () => {
        expect(resolveStoredPalette(null)).toBe("system");
        expect(resolveStoredPalette("film")).toBe("system");
        expect(resolveStoredPalette("matrix")).toBe("matrix");
    });
});
