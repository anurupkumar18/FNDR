import { afterEach, describe, expect, it } from "vitest";
import { removePalette } from "@/shared/theme/cinematic-palettes";
import { syncAuxiliaryAppearance } from "./auxiliaryAppearance";

describe("syncAuxiliaryAppearance", () => {
    afterEach(() => {
        localStorage.clear();
        removePalette();
        document.documentElement.removeAttribute("data-theme");
    });

    it("applies the stored light palette to an auxiliary window", () => {
        localStorage.setItem("fndr-theme", "light");
        localStorage.setItem("fndr-palette", "her");

        syncAuxiliaryAppearance();

        expect(document.documentElement).toHaveAttribute("data-theme", "light");
        expect(document.getElementById("cinematic-palette-vars")).toHaveTextContent(
            '--cp-active-palette: "her"',
        );
        expect(document.getElementById("cinematic-palette-vars")).toHaveTextContent(
            '--cp-active-mode: "light"',
        );
    });

    it("matches the main window fallback when stored appearance is invalid", () => {
        localStorage.setItem("fndr-theme", "sepia");
        localStorage.setItem("fndr-palette", "unknown");

        syncAuxiliaryAppearance();

        expect(document.documentElement).toHaveAttribute("data-theme", "dark");
        expect(document.getElementById("cinematic-palette-vars")).toHaveTextContent(
            '--cp-active-palette: "system"',
        );
    });
});
