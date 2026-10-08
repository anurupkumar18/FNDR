import React from "react";
import ReactDOM from "react-dom/client";
import { mockIPC } from "@tauri-apps/api/mocks";
import { AppShell } from "@/app/AppShell";
import { createPreviewIpcHandler } from "./previewIpc";
import { applyPalette } from "@/shared/theme/cinematic-palettes";
import { STORAGE_KEYS } from "@/shared/utils/config";
import { resolvePreviewConfig } from "./previewConfig";
import { VoicePreviewGallery } from "./VoicePreviewGallery";
import "@/app/styles/index.css";

const preview = resolvePreviewConfig(window.location.search);
const showVoiceStates = new URLSearchParams(window.location.search).get("voice") === "states";
document.documentElement.setAttribute("data-theme", preview.theme);
document.documentElement.dataset.motion = preview.motion;
localStorage.setItem(STORAGE_KEYS.theme, preview.theme);
localStorage.setItem(STORAGE_KEYS.palette, preview.palette);
applyPalette(preview.palette, preview.theme);

mockIPC(createPreviewIpcHandler(), { shouldMockEvents: true });

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
        {showVoiceStates ? <VoicePreviewGallery /> : <AppShell />}
    </React.StrictMode>,
);
