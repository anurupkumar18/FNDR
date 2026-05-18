import { useState, useEffect, useRef } from "react";
import { IMMERSIVE_SECTIONS } from "@/shared/motion";
import { useAppShell } from "./AppShell";
import { ChapterRail } from "@/domains/immersive/components/ChapterRail";
import { ScrollProgressIndicator } from "@/domains/immersive/components/ScrollProgressIndicator";
import { HeroSection } from "@/domains/immersive/sections/HeroSection";
import { CaptureSection } from "@/domains/immersive/sections/CaptureSection";
import { PlaceholderSection } from "@/domains/immersive/sections/PlaceholderSection";
import { SearchSection } from "@/domains/immersive/sections/SearchSection";
import { GraphSection } from "@/domains/immersive/sections/GraphSection";
import { AgentSection } from "@/domains/immersive/sections/AgentSection";
import { PrivacySection } from "@/domains/immersive/sections/PrivacySection";
import { WorkspaceSection } from "@/domains/immersive/sections/WorkspaceSection";
import { ImmersiveWallpaperContext } from "@/shared/hooks/useImmersiveWallpaper";
import type { AuroraPageId } from "@/shared/components/AuroraWallpaper";
import { AuroraWallpaper } from "@/shared/components/AuroraWallpaper";
import { getOnboardingState } from "@/shared/ipc/onboarding";
import { getFunGreeting } from "@/shared/ipc/tauri";
import { formatHomeDate } from "@/shared/utils/dateFormat";
import { getAuroraColors, isPaletteKey, type PaletteKey } from "@/shared/theme/cinematic-palettes";
import { STORAGE_KEYS } from "@/shared/utils/config";
import { CursorInverter } from "@/shared/components/CursorInverter";
import "@/domains/immersive/ScrollModeShell.css";

/**
 * Immersive scroll experience. Hosts a stack of full-viewport sections
 * navigated via mouse-wheel, keyboard (1–8 to jump), or the ChapterRail.
 *
 * Sections are listed in IMMERSIVE_SECTIONS (see scrollConfig.ts). Each
 * registers a stable DOM id so ChapterRail's IntersectionObserver can
 * track the currently visible section.
 *
 * The AuroraWallpaper is driven by the currently-visible section via
 * ImmersiveWallpaperContext — sections call useSetWallpaperPage to declare
 * which preset they want when in view.
 */
export function ScrollModeShell() {
    const { setMode } = useAppShell();
    const containerRef = useRef<HTMLDivElement>(null);
    const [wallpaperPage, setWallpaperPage] = useState<AuroraPageId>("home");

    // Active palette + mode (from localStorage, updated via custom event)
    const [activePalette, setActivePalette] = useState<PaletteKey>(() => {
        const stored = localStorage.getItem(STORAGE_KEYS.palette);
        return isPaletteKey(stored) ? stored : "film";
    });
    const [activeMode, setActiveMode] = useState<"dark" | "light">(() => {
        const stored = localStorage.getItem(STORAGE_KEYS.theme);
        return stored === "light" ? "light" : "dark";
    });

    useEffect(() => {
        const handler = (e: Event) => {
            const detail = (e as CustomEvent<{ palette: PaletteKey; mode: "dark" | "light" }>).detail;
            if (isPaletteKey(detail?.palette)) setActivePalette(detail.palette);
            if (detail?.mode === "light" || detail?.mode === "dark") setActiveMode(detail.mode);
        };
        window.addEventListener("fndr-appearance-changed", handler);
        return () => window.removeEventListener("fndr-appearance-changed", handler);
    }, []);

    // Aurora colors derived from active palette
    const auroraColors = getAuroraColors(activePalette, activeMode);

    // Greeting + date
    const [greeting, setGreeting] = useState<string>("");
    const [dateLabel, setDateLabel] = useState<string>(() => formatHomeDate(new Date()));

    useEffect(() => {
        let mounted = true;
        getOnboardingState()
            .then((state) => getFunGreeting(state.display_name))
            .then((g) => { if (mounted) setGreeting(g); })
            .catch(() => { if (mounted) setGreeting("Welcome back to FNDR."); });
        // Refresh date at midnight
        const msToMidnight = (() => {
            const now = new Date();
            const midnight = new Date(now);
            midnight.setHours(24, 0, 0, 0);
            return midnight.getTime() - now.getTime();
        })();
        const timer = setTimeout(() => setDateLabel(formatHomeDate(new Date())), msToMidnight);
        return () => { mounted = false; clearTimeout(timer); };
    }, []);

    // 1–8 jump to section by index. Skip when typing.
    useEffect(() => {
        const handler = (e: KeyboardEvent) => {
            const target = e.target as HTMLElement | null;
            if (target) {
                const tag = target.tagName;
                if (
                    tag === "INPUT" ||
                    tag === "TEXTAREA" ||
                    tag === "SELECT" ||
                    target.isContentEditable
                ) {
                    return;
                }
            }
            if (e.metaKey || e.ctrlKey || e.altKey) return;

            const digit = parseInt(e.key, 10);
            if (Number.isNaN(digit) || digit < 1 || digit > IMMERSIVE_SECTIONS.length) {
                return;
            }
            e.preventDefault();
            const target_section = IMMERSIVE_SECTIONS[digit - 1];
            const el = document.getElementById(`fndr-section-${target_section.id}`);
            el?.scrollIntoView({ behavior: "smooth", block: "start" });
        };
        window.addEventListener("keydown", handler);
        return () => window.removeEventListener("keydown", handler);
    }, []);

    return (
        <ImmersiveWallpaperContext.Provider value={setWallpaperPage}>
            <div className="fndr-immersive-root" data-theme={activeMode}>
                <CursorInverter />
                {/* Per-section wallpaper — morphs smoothly on page change */}
                <AuroraWallpaper
                    page={wallpaperPage}
                    className="fndr-immersive-wallpaper"
                    auroraBg={auroraColors.bg}
                    aurMid={auroraColors.mid}
                    aurAcc={auroraColors.acc}
                />
                <ScrollProgressIndicator targetRef={containerRef} />
                <ChapterRail onEnterWorkMode={() => setMode("work")} />
                <div className="fndr-immersive-scroll" ref={containerRef}>
                    <HeroSection
                        onEnterReel={() => {
                            const el = document.getElementById("fndr-section-capture");
                            el?.scrollIntoView({ behavior: "smooth", block: "start" });
                        }}
                        onEnterWorkMode={() => setMode("work")}
                        onScrollToSearch={(query) => {
                            const el = document.getElementById("fndr-section-search");
                            el?.scrollIntoView({ behavior: "smooth", block: "start" });
                            // Dispatch a custom event so SearchSection can pre-fill the query
                            window.dispatchEvent(new CustomEvent("fndr-hero-search", { detail: { query } }));
                        }}
                        greeting={greeting}
                        dateLabel={dateLabel}
                    />
                    <CaptureSection />
                    <PlaceholderSection
                        id="timeline"
                        label="Today's reel"
                        subtitle="Timeline of memories — coming in slice 4."
                    />
                    <SearchSection />
                    <GraphSection />
                    <AgentSection />
                    <PrivacySection />
                    <WorkspaceSection />
                </div>
            </div>
        </ImmersiveWallpaperContext.Provider>
    );
}

export default ScrollModeShell;
