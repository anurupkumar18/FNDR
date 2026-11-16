import React from "react";
import ReactDOM from "react-dom/client";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { applyPalette } from "@/shared/theme/cinematic-palettes";
import { installPressFeedback } from "@/shared/motion/pressFeedback";
import { createPreviewIpcHandler } from "./previewIpc";
import { resolvePreviewConfig } from "./previewConfig";
import "@/app/styles/index.css";

/**
 * Browser preview for FNDR's auxiliary windows (notch, omnibar, Screen Guide
 * overlay, autofill, Smart Fill) on the same privacy-safe mocked IPC as
 * ui-preview.html. `?window=notch|omnibar|guide|autofill|smartfill`.
 */
const params = new URLSearchParams(window.location.search);
const preview = resolvePreviewConfig(window.location.search);
document.documentElement.setAttribute("data-theme", preview.theme);
document.documentElement.dataset.motion = preview.motion;
applyPalette(preview.palette, preview.theme);

mockWindows(params.get("window") ?? "notch");
mockIPC(createPreviewIpcHandler(), { shouldMockEvents: true });
installPressFeedback();
// Lets a review script fire the backend events that drive each window.
(window as unknown as { fndrPreviewEmit: typeof emit }).fndrPreviewEmit = emit;

async function load(): Promise<React.ReactNode> {
    switch (params.get("window")) {
        case "omnibar": {
            await import("@/domains/omnibar/Omnibar.css");
            const { OmnibarApp } = await import("@/domains/omnibar/OmnibarApp");
            return <OmnibarApp />;
        }
        case "guide": {
            const { ScreenGuideOverlay } = await import("@/domains/screen-guide/ScreenGuideOverlay");
            return <ScreenGuideOverlay />;
        }
        case "autofill": {
            const { AutofillOverlay } = await import("@/domains/workspace/AutofillOverlay");
            return <AutofillOverlay />;
        }
        case "smartfill": {
            const { SmartFillGlow } = await import("@/domains/smart-fill/SmartFillGlow");
            return <SmartFillGlow />;
        }
        default: {
            await import("@/domains/notch/Notch.css");
            const { NotchHud } = await import("@/domains/notch/NotchHud");
            return <NotchHud />;
        }
    }
}

void load().then((node) => {
    ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(node);
});
