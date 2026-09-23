import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const getOnboardingState = vi.hoisted(() => vi.fn());

vi.mock("./AppPanels", () => ({
    AppPanels: ({
        activePanel,
        onClosePanel,
    }: {
        activePanel: string | null;
        onClosePanel: () => void;
    }) => activePanel
        ? <button type="button" onClick={onClosePanel}>Close mock panel</button>
        : null,
}));
vi.mock("./BiometricLockScreen", () => ({
    BiometricLockScreen: () => <div data-testid="biometric-lock" />,
}));
vi.mock("./HomeHero", () => ({
    HomeHero: ({ onHeroSearch }: { onHeroSearch: (query: string) => void }) => (
        <div data-testid="home">
            <button type="button" onClick={() => onHeroSearch("accessibility")}>Run search</button>
        </div>
    ),
}));
vi.mock("@/domains/search/SearchBar", () => ({ SearchBar: () => null }));
vi.mock("@/domains/timeline/Timeline", () => ({ Timeline: () => null }));
vi.mock("@/domains/workspace/ModelDownloadBanner", () => ({ ModelDownloadBanner: () => null }));
vi.mock("@/domains/workspace/Onboarding", () => ({
    Onboarding: () => <div data-testid="onboarding" />,
}));
vi.mock("@/domains/workspace/SearchHistoryPanel", () => ({
    appendToSearchHistory: vi.fn(),
}));

vi.mock("@/shared/hooks/useSearch", () => ({
    useSearch: () => ({ results: [], isLoading: false, error: null }),
}));
vi.mock("@/shared/hooks/usePolling", () => ({ usePolling: vi.fn() }));
vi.mock("@/shared/hooks/useTauriEvent", () => ({ useTauriEvent: vi.fn() }));

vi.mock("@/shared/ipc/onboarding", () => ({
    getOnboardingState,
    listAvailableModels: vi.fn().mockResolvedValue([]),
    saveOnboardingState: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@/shared/ipc/tauri", () => ({
    CAPTURE_STATUS_EVENT: "capture-status",
    OMNIBAR_OPEN_MEMORY_EVENT: "omnibar-open-memory",
    PRIVACY_ALERTS_EVENT: "privacy-alerts",
    fndrQualityStatus: vi.fn().mockResolvedValue({
        stored_count: 0,
        dropped_count: 0,
        flagged_count: 0,
    }),
    getAppNames: vi.fn().mockResolvedValue([]),
    getBlocklist: vi.fn().mockResolvedValue([]),
    getMeetingStatus: vi.fn().mockResolvedValue(null),
    getPrivacyAlerts: vi.fn().mockResolvedValue([]),
    onMeetingStatus: vi.fn().mockResolvedValue(() => {}),
    onFndrNotification: vi.fn().mockResolvedValue(() => {}),
    onProactiveSuggestion: vi.fn().mockResolvedValue(() => {}),
    pauseCapture: vi.fn().mockResolvedValue(undefined),
    resumeCapture: vi.fn().mockResolvedValue(undefined),
    setBlocklist: vi.fn().mockResolvedValue(undefined),
    getStatus: vi.fn().mockResolvedValue({
        is_capturing: false,
        is_paused: false,
        ai_model_available: true,
        pipeline: {
            stored_total: 0,
            skipped_total: 0,
        },
    }),
    getFunGreeting: vi.fn().mockResolvedValue("Welcome back to FNDR."),
}));

import App from "./App";

beforeEach(() => {
    getOnboardingState.mockReset();
});

afterEach(() => {
    cleanup();
});

describe("App onboarding gate", () => {
    const completedOnboarding = {
        step: "complete",
        biometric_enabled: false,
        screen_permission: true,
        accessibility_permission: false,
        model_downloaded: false,
        model_id: null,
        display_name: "Ada",
    };

    it("opens the workspace after completed onboarding when the optional model was skipped", async () => {
        getOnboardingState.mockResolvedValue(completedOnboarding);

        render(<App />);

        expect(await screen.findByTestId("home")).toBeInTheDocument();
        expect(screen.queryByTestId("onboarding")).not.toBeInTheDocument();
    });

    it("makes Home inert behind a full-screen panel and restores shell focus after close", async () => {
        getOnboardingState.mockResolvedValue(completedOnboarding);
        render(<App />);

        const sidebarButton = await screen.findByRole("button", { name: "Open sidebar" });
        fireEvent.click(sidebarButton);
        fireEvent.click(screen.getByRole("button", { name: "Memory Vault" }));

        const background = document.querySelector(".app-background-layer");
        await waitFor(() => expect(background).toHaveAttribute("inert"));

        fireEvent.click(screen.getByRole("button", { name: "Close mock panel" }));
        await waitFor(() => expect(background).not.toHaveAttribute("inert"));
        await waitFor(() => expect(sidebarButton).toHaveFocus());
    });

    it("marks only the submitted-search layout for narrow-screen chrome clearance", async () => {
        getOnboardingState.mockResolvedValue(completedOnboarding);
        render(<App />);

        await screen.findByTestId("home");
        const main = document.querySelector(".app-main");
        expect(main).not.toHaveClass("has-active-search");

        fireEvent.click(screen.getByRole("button", { name: "Run search" }));

        await waitFor(() => expect(main).toHaveClass("has-active-search"));
    });

    it("makes the workspace inert behind Settings and restores its trigger after Escape", async () => {
        getOnboardingState.mockResolvedValue(completedOnboarding);
        const nativeFocus = HTMLElement.prototype.focus;
        const focusSpy = vi.spyOn(HTMLElement.prototype, "focus").mockImplementation(function (
            this: HTMLElement,
            options?: FocusOptions,
        ) {
            if (this.closest("[inert]")) return;
            nativeFocus.call(this, options);
        });
        render(<App />);

        const settingsButton = await screen.findByRole("button", { name: /open settings/i });
        const background = document.querySelector(".app-background-layer");
        expect(background).not.toHaveAttribute("inert");

        fireEvent.click(settingsButton);

        expect(await screen.findByRole("dialog", { name: /^settings$/i })).toBeInTheDocument();
        await waitFor(() => expect(background).toHaveAttribute("inert"));
        await waitFor(() => expect(screen.getByRole("button", { name: /close settings/i })).toHaveFocus());

        fireEvent.keyDown(document, { key: "Escape" });

        await waitFor(() => expect(screen.queryByRole("dialog", { name: /^settings$/i })).toBeNull());
        await waitFor(() => expect(background).not.toHaveAttribute("inert"));
        await waitFor(() => expect(settingsButton).toHaveFocus());
        focusSpy.mockRestore();
    });
});
