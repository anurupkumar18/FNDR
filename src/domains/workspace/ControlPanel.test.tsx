import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import {
    type CaptureStatus,
    getAgentNotesEnabled,
    getBlocklist,
    getPrivacyAlerts,
    pauseCapture,
    resumeCapture,
    setAgentNotesEnabled,
    setBlocklist,
} from "@/shared/ipc/tauri";
import { ControlPanel } from "./ControlPanel";
import { downloadModel, getAppleModelStatus, listAvailableModels, setAppleModelEnabled } from "@/shared/ipc/onboarding";

const checkUpdateMock = vi.fn();

vi.mock("@/shared/voice/VoiceOutputSection", () => ({
    VoiceOutputSection: () => <div data-testid="voice-output-section" />,
}));

vi.mock("@tauri-apps/plugin-updater", () => ({
    check: (...args: unknown[]) => checkUpdateMock(...args),
}));

vi.mock("@tauri-apps/plugin-process", () => ({
    relaunch: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ({
    getActionSettings: vi.fn().mockResolvedValue({ actions_kill_switch: false, confirm_unnamed_apps: false }),
    setActionSettings: vi.fn().mockResolvedValue({ actions_kill_switch: false, confirm_unnamed_apps: false }),
    listActionApprovals: vi.fn().mockResolvedValue([]),
    resolveActionApproval: vi.fn().mockResolvedValue(undefined),
    PRIVACY_ALERTS_EVENT: "privacy://alerts",
    CAPTURE_STATUS_EVENT: "capture://status",
    deleteAllData: vi.fn(),
    deleteOlderThan: vi.fn(),
    addSiteToBlocklist: vi.fn(),
    dismissPrivacyAlert: vi.fn(),
    fndrQualityStatus: vi.fn().mockResolvedValue({
        stored_count: 0,
        dropped_count: 0,
        flagged_count: 0,
    }),
    getBlocklist: vi.fn().mockResolvedValue([]),
    getAgentNotesEnabled: vi.fn().mockResolvedValue(false),
    getVerbatimMemoryEnabled: vi.fn().mockResolvedValue(false),
    setVerbatimMemoryEnabled: vi.fn().mockResolvedValue(undefined),
    getAutofillSettings: vi.fn().mockResolvedValue({
        enabled: true,
        shortcut: "Alt+F",
        lookback_days: 90,
        auto_inject_threshold: 0.9,
        prefer_typed_injection: true,
        max_candidates: 4,
    }),
    getContextRuntimeStatus: vi.fn().mockResolvedValue({
        status: "healthy",
        active_project: null,
        current_context_pack_id: null,
        latest_pack_summary: "",
        tokens_used: 0,
        last_generated_at: null,
        recent_pack_count: 0,
        activity_event_count: 0,
        decision_count: 0,
        runtime_tables_ready: true,
    }),
    getMemoryRepairProgress: vi.fn(),
    getStorageHealth: vi.fn().mockResolvedValue({
        memory_db_bytes: 1024,
        frames_bytes: 0,
        models_bytes: 2048,
        dev_build_cache_bytes: 0,
        runtime_total_bytes: 3072,
        measured_at_ms: 0,
    }),
    getStorageReclaimProgress: vi.fn(),
    getMcpServerStatus: vi.fn().mockResolvedValue({
        running: false,
        host: "127.0.0.1",
        port: 8799,
        endpoint: "http://127.0.0.1:8799/mcp",
        require_auth: false,
        auth_mode: "disabled for localhost",
        last_error: null,
    }),
    getPrivacyAlerts: vi.fn().mockResolvedValue([]),
    getRetentionDays: vi.fn().mockResolvedValue(7),
    notchDoSkills: vi.fn().mockResolvedValue([]),
    getNotchDoAutonomy: vi.fn().mockResolvedValue("smooth"),
    setNotchDoAutonomy: vi.fn().mockResolvedValue("smooth"),
    notchDoRuns: vi.fn().mockResolvedValue([]),
    pauseCapture: vi.fn(),
    resumeCapture: vi.fn(),
    reclaimMemoryStorage: vi.fn(),
    runMemoryRepairBackfill: vi.fn(),
    setBlocklist: vi.fn(),
    setAgentNotesEnabled: vi.fn().mockResolvedValue(undefined),
    setAutofillSettings: vi.fn(),
    setRetentionDays: vi.fn(),
    startMcpServer: vi.fn(),
    stopMcpServer: vi.fn(),
}));

vi.mock("@/shared/ipc/onboarding", () => ({
    getAppleModelStatus: vi.fn().mockResolvedValue({ availability: "unsupported_os", enabled: false }),
    setAppleModelEnabled: vi.fn(),
    deleteAiModel: vi.fn(),
    downloadModel: vi.fn(),
    getModelDownloadStatus: vi.fn().mockResolvedValue({
        state: "idle",
        model_id: null,
        filename: null,
        download_url: null,
        destination_path: null,
        temp_path: null,
        bytes_downloaded: 0,
        total_bytes: 0,
        percent: 0,
        done: false,
        error: null,
        logs: [],
        updated_at_ms: 0,
    }),
    getOnboardingState: vi.fn().mockResolvedValue({
        step: "complete",
        model_downloaded: true,
        display_name: null,
        biometric_enabled: false,
    }),
    listAvailableModels: vi.fn().mockResolvedValue([]),
    onDownloadStatus: vi.fn().mockResolvedValue(() => {}),
    refreshAiModels: vi.fn(),
    saveOnboardingState: vi.fn(),
}));

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    vi.mocked(getPrivacyAlerts).mockResolvedValue([]);
    vi.mocked(getAgentNotesEnabled).mockResolvedValue(false);
    localStorage.clear();
    document.getElementById("cinematic-palette-vars")?.remove();
});

function openTab(name: string) {
    fireEvent.click(screen.getByRole("tab", { name: new RegExp(`^${name}`, "i") }));
}

describe("ControlPanel", () => {
    it("shows the Apple text model switch and saves an opt out", async () => {
        vi.mocked(getAppleModelStatus).mockResolvedValueOnce({ availability: "available", enabled: true });
        vi.mocked(setAppleModelEnabled).mockResolvedValueOnce({ availability: "available", enabled: false });
        render(<ControlPanel status={null} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        openTab("AI");

        const toggle = await screen.findByRole("switch", { name: "Use Apple on-device model" });
        await waitFor(() => expect(toggle).toBeChecked());
        fireEvent.click(toggle);
        await waitFor(() => expect(setAppleModelEnabled).toHaveBeenCalledWith(false));
        await waitFor(() => expect(toggle).not.toBeChecked());
    });

    it("names the backend that handled the last text call", () => {
        render(<ControlPanel status={{ ...statusWithEmbedder("real", false), last_text_backend: "apple_foundation" }} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        openTab("AI");
        expect(screen.getByText(/Last text backend: Apple on-device model/)).toBeInTheDocument();
    });

    it("ignores an earlier settings load after reopening and saving consent", async () => {
        let finishFirstLoad!: (apps: string[]) => void;
        vi.mocked(getBlocklist).mockImplementationOnce(() => new Promise<string[]>((resolve) => {
            finishFirstLoad = resolve;
        }));
        render(<ControlPanel status={null} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        openTab("AI");
        await waitFor(() => expect(getAgentNotesEnabled).toHaveBeenCalledTimes(1));
        fireEvent.click(screen.getByRole("button", { name: /close settings/i }));
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));

        const toggle = screen.getByRole("switch", { name: "Let assistants add notes" });
        await waitFor(() => expect(toggle).toBeEnabled());
        fireEvent.click(toggle);
        await waitFor(() => expect(toggle).toBeChecked());

        finishFirstLoad([]);
        await waitFor(() => expect(getAgentNotesEnabled).toHaveBeenCalledTimes(2));
        fireEvent.click(toggle);
        await waitFor(() => expect(setAgentNotesEnabled).toHaveBeenLastCalledWith(false));
        expect(toggle).not.toBeChecked();
    });

    it("changes assistant note consent only after the setting is saved", async () => {
        let finishSave!: () => void;
        vi.mocked(setAgentNotesEnabled).mockImplementationOnce(() => new Promise<void>((resolve) => {
            finishSave = resolve;
        }));
        render(<ControlPanel status={null} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        openTab("AI");

        const toggle = screen.getByRole("switch", { name: "Let assistants add notes" });
        expect(toggle).toBeDisabled();
        await waitFor(() => expect(toggle).toBeEnabled());
        expect(toggle).not.toBeChecked();

        fireEvent.click(toggle);
        expect(setAgentNotesEnabled).toHaveBeenCalledWith(true);
        expect(toggle).toBeDisabled();
        expect(toggle).not.toBeChecked();
        finishSave();
        await waitFor(() => expect(toggle).toBeChecked());

        fireEvent.click(toggle);
        await waitFor(() => expect(toggle).not.toBeChecked());
        expect(setAgentNotesEnabled).toHaveBeenLastCalledWith(false);
    });

    it("keeps the saved assistant note setting when a save fails", async () => {
        vi.mocked(getAgentNotesEnabled).mockResolvedValue(true);
        vi.mocked(setAgentNotesEnabled).mockRejectedValueOnce(new Error("config is read-only"));
        render(<ControlPanel status={null} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        openTab("AI");
        const toggle = screen.getByRole("switch", { name: "Let assistants add notes" });
        await waitFor(() => expect(toggle).toBeChecked());

        fireEvent.click(toggle);

        expect(await screen.findByRole("alert", { name: /assistant notes update failed/i }))
            .toHaveTextContent("config is read-only");
        expect(toggle).toBeChecked();
        expect(toggle).toBeEnabled();
    });

    it("keeps assistant note consent disabled when its current setting cannot load", async () => {
        vi.mocked(getAgentNotesEnabled).mockRejectedValueOnce(new Error("IPC unavailable"));
        render(<ControlPanel status={null} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        openTab("AI");

        expect(await screen.findByText(/Some settings could not be loaded: assistant notes/))
            .toBeInTheDocument();
        expect(screen.getByRole("switch", { name: "Let assistants add notes" })).toBeDisabled();
        expect(setAgentNotesEnabled).not.toHaveBeenCalled();
    });

    it("keeps the closed settings sheet out of the accessibility tree", () => {
        render(<ControlPanel status={null} compact={true} />);

        const settingsButton = screen.getByRole("button", { name: /open settings/i });

        expect(settingsButton).toHaveAttribute("aria-expanded", "false");
        expect(document.getElementById("fndr-settings-panel")).toBeNull();
        expect(screen.queryByRole("heading", { name: /profile/i })).not.toBeInTheDocument();

        fireEvent.click(settingsButton);

        const settingsPanel = document.getElementById("fndr-settings-panel");

        expect(settingsButton).toHaveAttribute("aria-expanded", "true");
        expect(screen.getByRole("dialog", { name: /^settings$/i })).toBe(settingsPanel);
        expect(screen.getByRole("heading", { name: /about/i })).toBeInTheDocument();
    });

    it("uses the alert badge without forcing settings open over the current task", async () => {
        vi.mocked(getPrivacyAlerts).mockResolvedValue([
            { id: "privacy-alert-1", domain_or_title: "bank.example", detected_at: 1 },
        ]);

        render(<ControlPanel status={statusWithEmbedder("real", false)} compact={true} />);

        expect(
            await screen.findByRole("button", { name: /open settings, 1 privacy alert/i }),
        ).toHaveAttribute("aria-expanded", "false");
        expect(screen.queryByRole("dialog", { name: /^settings$/i })).toBeNull();
    });

    it("moves focus into the modal drawer and restores it after Escape", async () => {
        render(<ControlPanel status={statusWithEmbedder("real", false)} compact={true} />);

        const settingsButton = screen.getByRole("button", { name: /open settings/i });
        fireEvent.click(settingsButton);

        const closeButton = await screen.findByRole("button", { name: /close settings/i });
        await waitFor(() => expect(closeButton).toHaveFocus());

        fireEvent.keyDown(document, { key: "Escape" });

        await waitFor(() => expect(settingsButton).toHaveFocus());
        expect(settingsButton).toHaveAttribute("aria-expanded", "false");
    });

    it("reports capture and blocklist action failures in the drawer", async () => {
        vi.mocked(pauseCapture).mockRejectedValueOnce(new Error("capture unavailable"));
        vi.mocked(setBlocklist).mockRejectedValueOnce(new Error("settings unavailable"));

        render(<ControlPanel status={statusWithEmbedder("real", false)} compact={true} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        openTab("Capture");

        fireEvent.click(await screen.findByRole("button", { name: /pause capture/i }));
        expect(await screen.findByRole("alert", { name: /capture action failed/i })).toHaveTextContent(
            /capture unavailable/i,
        );

        openTab("Privacy");
        fireEvent.change(screen.getByRole("textbox", { name: /app or website/i }), {
            target: { value: "Private Browser" },
        });
        fireEvent.click(screen.getByRole("button", { name: /^add$/i }));
        expect(await screen.findByRole("alert", { name: /blocklist update failed/i })).toHaveTextContent(
            /settings unavailable/i,
        );
    });

    it("offers a desktop exit when private mode is active", async () => {
        const status = statusWithEmbedder("real", false);
        status.is_incognito = true;

        render(<ControlPanel status={status} compact={true} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        openTab("Capture");
        fireEvent.click(await screen.findByRole("button", { name: /exit private mode/i }));

        await waitFor(() => expect(resumeCapture).toHaveBeenCalledTimes(1));
        expect(screen.getByText(/private mode ended/i)).toBeInTheDocument();
    });

    it("uses the Film and Paper pair by default and switches between it with the theme", () => {
        localStorage.setItem("fndr-theme", "dark");
        render(<ControlPanel status={null} compact={true} />);

        expect(document.getElementById("cinematic-palette-vars")?.textContent).toContain(
            '--cp-active-palette: "film"',
        );

        fireEvent.click(screen.getByRole("button", { name: /switch to light mode/i }));

        expect(document.documentElement).toHaveAttribute("data-theme", "light");
        expect(document.getElementById("cinematic-palette-vars")?.textContent).toContain(
            '--cp-active-palette: "paper"',
        );
        expect(document.getElementById("cinematic-palette-vars")?.textContent).toContain(
            '--cp-active-mode: "light"',
        );
    });

    it("keeps cinematic palettes behind optional customization in Settings", () => {
        render(<ControlPanel status={null} compact={true} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));

        const defaults = screen.getByRole("radiogroup", { name: "Film & Paper" });
        expect(within(defaults).getByRole("radio", { name: "Film" })).toBeInTheDocument();
        expect(within(defaults).getByRole("radio", { name: "Paper" })).toBeInTheDocument();

        const optionalSummary = screen.getByText("Optional cinematic palettes");
        const optionalPalettes = optionalSummary.closest("details");
        expect(optionalPalettes).not.toHaveAttribute("open");

        fireEvent.click(optionalSummary);
        expect(optionalPalettes).toHaveAttribute("open");
        expect(screen.getByRole("radio", { name: "The Matrix" })).toBeInTheDocument();
    });

    it("lets the user change the home background and tells the wallpaper layer", () => {
        localStorage.setItem("fndr-wallpaper", "warpGrid");
        const changes: unknown[] = [];
        const listener = (event: Event) => changes.push((event as CustomEvent).detail);
        window.addEventListener("fndr-appearance-changed", listener);
        render(<ControlPanel status={null} compact={true} />);

        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        const backgrounds = screen.getByRole("radiogroup", { name: "Background" });
        expect(within(backgrounds).getByRole("radio", { name: "Warp Grid" })).toHaveAttribute("aria-checked", "true");

        fireEvent.click(within(backgrounds).getByRole("radio", { name: "Nebula Drift" }));

        expect(within(backgrounds).getByRole("radio", { name: "Nebula Drift" })).toHaveAttribute("aria-checked", "true");
        expect(localStorage.getItem("fndr-wallpaper")).toBe("nebula");
        expect(changes[changes.length - 1]).toMatchObject({ wallpaper: "nebula" });
        window.removeEventListener("fndr-appearance-changed", listener);
    });

    it("shows one demo-safe settings sheet without destructive or model-management controls", async () => {
        render(<ControlPanel status={null} compact={true} />);

        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));

        expect(await screen.findByRole("heading", { name: /about/i })).toBeInTheDocument();
        openTab("Capture");
        expect(screen.getByRole("heading", { name: /^capture$/i })).toBeInTheDocument();
        openTab("Privacy");
        expect(screen.getByRole("heading", { name: /privacy alerts/i })).toBeInTheDocument();
        expect(screen.getByRole("heading", { name: /blocked apps & sites/i })).toBeInTheDocument();
        expect(await screen.findByText(/no recent apps or sites need your review/i)).toBeInTheDocument();
        openTab("AI");
        expect(screen.getByRole("heading", { name: /^models$/i })).toBeInTheDocument();
        expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
        expect(screen.queryByText(/danger zone/i)).toBeNull();
        expect(screen.queryByRole("button", { name: /delete/i })).toBeNull();
        expect(screen.queryByRole("button", { name: /download/i })).toBeNull();
        expect(screen.queryByText(/auto-fill/i)).toBeNull();
        expect(screen.queryByText(/start mcp server|stop mcp server|mcp server status/i)).toBeNull();
    });

    it("groups settings into five tabs, each holding its own people-first sections", async () => {
        render(<ControlPanel status={null} compact={true} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));

        const tabs = [
            ["General", ["Home", "About"]],
            ["Capture", ["Capture"]],
            ["Privacy", ["Privacy"]],
            ["Voice", ["Voice"]],
            ["AI", ["Agent access", "Models", "Updates"]],
        ] as const;
        expect(screen.getAllByRole("tab").map((tab) => tab.textContent?.replace(/\d+$/, ""))).toEqual(
            tabs.map(([name]) => name),
        );

        for (const [name, headings] of tabs) {
            openTab(name);
            const panel = screen.getByRole("tabpanel");
            for (const heading of headings) {
                expect(within(panel).getByRole("heading", { name: heading, level: 3 })).toBeInTheDocument();
            }
        }

        const sections = [
            ["Capture", "settings-section--capture", /control when fndr may process/i],
            ["Privacy", "settings-section--privacy", /review sensitive activity/i],
            ["Voice", "settings-section--voice", /microphone controls/i],
            ["Agent access", "settings-section--agent-access", /choose whether connected assistants/i],
            ["Models", "settings-section--models", /intelligence is available locally/i],
            ["Updates", "settings-section--updates", /components ready to use/i],
            ["About", "settings-section--about", /personalize fndr/i],
        ] as const;
        for (const [name, className, description] of sections) {
            const heading = screen.getAllByRole("heading", { name, level: 3, hidden: true })[0];
            const section = heading.closest("section");
            expect(section).toHaveClass(className);
            expect(within(section as HTMLElement).getByText(description)).toBeInTheDocument();
        }
    });

    it("shows one tab page at a time and moves with the arrow keys", async () => {
        render(<ControlPanel status={null} compact={true} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));

        const general = screen.getByRole("tab", { name: "General" });
        expect(general).toHaveAttribute("aria-selected", "true");
        expect(general).toHaveAttribute("tabindex", "0");
        expect(screen.getByRole("tab", { name: "Capture" })).toHaveAttribute("tabindex", "-1");
        expect(screen.getAllByRole("tabpanel")).toHaveLength(1);

        fireEvent.keyDown(general, { key: "ArrowRight" });
        const capture = screen.getByRole("tab", { name: "Capture" });
        expect(capture).toHaveAttribute("aria-selected", "true");
        expect(capture).toHaveFocus();
        expect(screen.getByRole("tabpanel")).toHaveAttribute("data-direction", "forward");

        fireEvent.keyDown(capture, { key: "ArrowLeft" });
        expect(screen.getByRole("tab", { name: "General" })).toHaveAttribute("aria-selected", "true");
        expect(screen.getByRole("tabpanel")).toHaveAttribute("data-direction", "back");

        fireEvent.keyDown(screen.getByRole("tab", { name: "General" }), { key: "End" });
        expect(screen.getByRole("tab", { name: "AI" })).toHaveAttribute("aria-selected", "true");
        fireEvent.keyDown(screen.getByRole("tab", { name: "AI" }), { key: "ArrowRight" });
        expect(screen.getByRole("tab", { name: "General" })).toHaveAttribute("aria-selected", "true");
    });

    it("lets the person choose which sections Home shows, from the General tab", () => {
        render(<ControlPanel status={null} compact={true} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));

        const switches = ["Routine offers", "Due soon", "Pick up where you left off", "Your sets"];
        for (const name of switches) {
            expect(screen.getByRole("switch", { name })).toBeChecked();
        }
        fireEvent.click(screen.getByRole("switch", { name: "Due soon" }));
        expect(screen.getByRole("switch", { name: "Due soon" })).not.toBeChecked();
        expect(JSON.parse(localStorage.getItem("fndr-home-sections") ?? "{}")).toMatchObject({ deadlines: false });
    });

    it("keeps a half-typed field when the person visits another tab", async () => {
        render(<ControlPanel status={null} compact={true} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));

        openTab("Privacy");
        fireEvent.change(screen.getByRole("textbox", { name: /app or website/i }), {
            target: { value: "bank.example" },
        });
        openTab("Capture");
        openTab("Privacy");
        expect(screen.getByRole("textbox", { name: /app or website/i })).toHaveValue("bank.example");
    });

    it("lets people download an optional model from Settings", async () => {
        vi.mocked(listAvailableModels).mockResolvedValueOnce([{
            id: "qwen3-vl-2b",
            name: "Qwen3-VL 2B",
            description: "Adds richer visual understanding to search.",
            size_bytes: 1_600_000_000,
            size_label: "1.6 GB",
            quality_label: "Higher visual detail",
            speed_label: "Fast",
            ram_gb: 4,
            recommended: false,
            required: false,
            filename: "qwen3-vl-2b.gguf",
            download_url: "https://models.example.test/qwen3-vl-2b.gguf",
        }]);
        render(<ControlPanel status={null} compact={true} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        openTab("AI");

        const model = await screen.findByText("Qwen3-VL 2B");
        const option = model.closest(".model-download-option");
        expect(option).not.toBeNull();
        fireEvent.click(within(option as HTMLElement).getByRole("button", { name: "Get it" }));

        await waitFor(() => expect(downloadModel).toHaveBeenCalledWith(
            "qwen3-vl-2b",
            "https://models.example.test/qwen3-vl-2b.gguf",
            "qwen3-vl-2b.gguf",
        ));
        expect(await screen.findByRole("status")).toHaveTextContent(/is downloading/i);
    });

    it("shows a read-only warning when capture cannot use its embedding model", async () => {
        render(<ControlPanel status={statusWithEmbedder("unavailable", true)} compact={true} />);

        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));

        expect(
            await screen.findByText(/capture is paused until the embedding model/i),
        ).toBeInTheDocument();
    });

    it("shows the capture pipeline and model state from the live backend status", async () => {
        const status = statusWithEmbedder("real", false);
        status.is_capturing = true;
        status.frames_captured = 4;
        status.ai_model_loaded = true;
        status.loaded_model_id = "qwen3-vl-2b";
        status.pipeline.stored_total = 3;
        status.pipeline.skipped_total = 1;
        render(<ControlPanel status={status} compact={true} />);

        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        openTab("Capture");

        const trace = await screen.findByLabelText("Capture pipeline activity");
        expect(trace).toHaveTextContent("Capture enabled for the next sample");
        expect(trace).toHaveTextContent("Capture policy");
        fireEvent.click(within(trace).getByRole("button", { name: "Show Capture pipeline activity details" }));
        expect(trace).toHaveTextContent("3 stored · 1 skipped");
        expect(trace).toHaveTextContent("all-MiniLM-L6-v2");
        expect(trace).toHaveTextContent("qwen3-vl-2b");
        expect(trace).toHaveTextContent("Backend status");
    });

    it("surfaces degraded embeddings instead of presenting capture as healthy", async () => {
        const status = statusWithEmbedder("hash_fallback", true);
        status.is_capturing = true;
        render(<ControlPanel status={status} compact={true} />);

        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));

        const trace = await screen.findByLabelText("Capture pipeline activity");
        expect(trace).toHaveTextContent("Capture enabled with degraded embeddings");
        expect(trace).toHaveAttribute("data-status", "degraded");
    });

    it("does not show the capture-paused warning when the embedder is healthy", async () => {
        render(<ControlPanel status={statusWithEmbedder("real", false)} compact={true} />);

        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        openTab("AI");

        expect(await screen.findByRole("heading", { name: /^models$/i })).toBeInTheDocument();
        expect(screen.queryByText(/capture is paused until the embedding model/i)).toBeNull();
    });
});

function statusWithEmbedder(backend: string, degraded: boolean): CaptureStatus {
    return {
        is_capturing: false,
        is_paused: false,
        is_incognito: false,
        frames_captured: 0,
        frames_dropped: 0,
        last_capture_time: 0,
        ai_model_available: true,
        ai_model_loaded: false,
        loaded_model_id: null,
        embedding_backend: backend,
        embedding_degraded: degraded,
        embedding_detail: "",
        embedding_model_name: "all-MiniLM-L6-v2",
        embedding_dimension: 384,
        pipeline: {
            evaluated: 0,
            stored_ocr_path: 0,
            stored_visual_path: 0,
            stored_url_only: 0,
            stored_total: 0,
            skipped_blocklist: 0,
            skipped_self_app: 0,
            skipped_surface_policy: 0,
            skipped_perceptual_dup: 0,
            skipped_semantic_dup: 0,
            skipped_ocr_failed: 0,
            skipped_low_signal_text: 0,
            skipped_noise: 0,
            skipped_grounding: 0,
            skipped_stacked_extraction: 0,
            skipped_visual_small: 0,
            skipped_visual_novelty: 0,
            skipped_visual_compose_failed: 0,
            skipped_screen_capture_failed: 0,
            skipped_embedder_unavailable: 0,
            skipped_total: 0,
            last_skip_reason: null,
            last_skip_app: null,
            last_skip_timestamp_ms: null,
        },
    };
}
