import React from "react";
import ReactDOM from "react-dom/client";
import { mockIPC } from "@tauri-apps/api/mocks";
import { AppShell } from "@/app/AppShell";
import { createPreviewIpcHandler } from "./previewIpc";
import { applyPalette } from "@/shared/theme/cinematic-palettes";
import { STORAGE_KEYS } from "@/shared/utils/config";
import { resolvePreviewConfig } from "./previewConfig";
import { installPressFeedback } from "@/shared/motion/pressFeedback";
import { VoicePreviewGallery } from "./VoicePreviewGallery";
import { AskPanel } from "@/domains/ask/AskPanel";
import { MemoryCardsPanel } from "@/domains/memory-vault/MemoryCardsPanel";
import "@/app/styles/index.css";

const preview = resolvePreviewConfig(window.location.search);
const showVoiceStates = new URLSearchParams(window.location.search).get("voice") === "states";
const showAsk = new URLSearchParams(window.location.search).get("panel") === "ask";
document.documentElement.setAttribute("data-theme", preview.theme);
document.documentElement.dataset.motion = preview.motion;
localStorage.setItem(STORAGE_KEYS.theme, preview.theme);
localStorage.setItem(STORAGE_KEYS.palette, preview.palette);
applyPalette(preview.palette, preview.theme);

mockIPC(createPreviewIpcHandler(), { shouldMockEvents: true });
installPressFeedback();

/** Direct entry for reviewing the real conversation panel with synthetic IPC. */
function AskPreview() {
    const [memoryId, setMemoryId] = React.useState<string | null>(null);
    return <>
        <AskPanel isVisible={memoryId === null} onClose={() => {
            const url = new URL(window.location.href);
            url.searchParams.delete("panel");
            window.location.assign(url);
        }} onOpenMemoryById={setMemoryId} />
        <MemoryCardsPanel isVisible={memoryId !== null} onClose={() => setMemoryId(null)}
            appNames={[]} feature="vault" focusMemoryId={memoryId} onOpenMemoryById={setMemoryId} />
    </>;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
        {showVoiceStates ? <VoicePreviewGallery /> : showAsk ? <AskPreview /> : <AppShell />}
    </React.StrictMode>,
);
