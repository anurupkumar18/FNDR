import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AppShell } from "@/app/AppShell";
import { createPreviewIpcHandler } from "@/dev/previewIpc";

vi.mock("@/shared/components/MotionWallpaper", () => ({ MotionWallpaper: () => null }));

class StaticIntersectionObserver {
    observe() {}
    disconnect() {}
    unobserve() {}
}

function renderPreview(onboarding?: string) {
    window.history.replaceState(null, "", onboarding ? `/ui-preview.html?onboarding=${onboarding}` : "/ui-preview.html");
    const ipc = createPreviewIpcHandler();
    mockIPC(ipc, { shouldMockEvents: true });
    render(<AppShell />);
    return ipc;
}

beforeEach(() => {
    localStorage.clear();
    vi.stubGlobal("IntersectionObserver", StaticIntersectionObserver);
});

afterEach(async () => {
    cleanup();
    // Let event-listener cleanup finish before removing the Tauri event mock.
    await act(async () => { await Promise.resolve(); });
    clearMocks();
    vi.unstubAllGlobals();
    Reflect.deleteProperty(navigator, "clipboard");
    window.history.replaceState(null, "", "/");
});

describe("journeys through the real app shell", () => {
    it("a person can complete onboarding and reach their workspace", async () => {
        renderPreview("welcome");
        fireEvent.click(await screen.findByRole("button", { name: "Get Started" }));
        expect(await screen.findByRole("heading", { name: "How FNDR handles your data" })).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Continue" }));
        expect(await screen.findByRole("heading", { name: "Lock FNDR with Touch ID" })).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Skip for now" }));
        expect(await screen.findByRole("heading", { name: "Select your local AI model" })).toBeInTheDocument();
        fireEvent.click(await screen.findByRole("button", { name: /^Skip for now/ }));
        expect(await screen.findByRole("heading", { name: "Grant a few permissions" })).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Skip for now" }));
        fireEvent.click(await screen.findByRole("button", { name: "Skip imports" }));
        expect(await screen.findByRole("textbox", { name: "Search your memories" })).toBeInTheDocument();
    });

    it("a person can find a memory and open it", async () => {
        renderPreview();
        const input = await screen.findByRole("textbox", { name: "Search your memories" });
        fireEvent.change(input, { target: { value: "FNDR home experience" } });
        fireEvent.keyDown(input, { key: "Enter", code: "Enter" });
        expect(await screen.findByText("Reviewed the FNDR home experience")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Selected memory: Reviewed the FNDR home experience" })).toHaveAttribute("aria-pressed", "true");
        fireEvent.click(screen.getByRole("button", { name: "Open sidebar" }));
        fireEvent.click(screen.getByRole("button", { name: "Memory Vault" }));
        fireEvent.click(await screen.findByRole("button", { name: "Open memory: Reviewed the FNDR home experience" }));
        expect(await screen.findByRole("button", { name: "Close memory details" })).toBeInTheDocument();
    });

    it("a person can pause capture and see it stay paused", async () => {
        const ipc = renderPreview();
        fireEvent.click(await screen.findByRole("button", { name: "Open settings" }));
        fireEvent.click(screen.getByRole("tab", { name: "Capture" }));
        fireEvent.click(await screen.findByRole("button", { name: "Pause capture" }));
        expect(await screen.findByText(/Capture paused. It will remain paused after relaunch./)).toBeInTheDocument();
        expect(await ipc("get_status")).toMatchObject({ is_paused: true });
        fireEvent.click(screen.getByRole("button", { name: "Close settings" }));
        fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
        fireEvent.click(screen.getByRole("tab", { name: "Capture" }));
        expect(screen.getByRole("button", { name: "Resume capture" })).toBeInTheDocument();
    });

    it("a person can enable agent notes and copy connection setup", async () => {
        const writeText = vi.fn().mockResolvedValue(undefined);
        Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
        renderPreview();
        fireEvent.click(await screen.findByRole("button", { name: "Open settings" }));
        fireEvent.click(screen.getByRole("tab", { name: "AI" }));
        const notes = await screen.findByRole("switch", { name: "Let assistants add notes" });
        fireEvent.click(notes);
        await waitFor(() => expect(notes).toBeChecked());
        const client = await screen.findByRole("listitem", { name: "Claude Desktop" });
        fireEvent.click(within(client).getByRole("button", { name: "Show setup" }));
        fireEvent.click(await screen.findByRole("button", { name: "Copy setup" }));
        await waitFor(() => expect(writeText).toHaveBeenCalledWith(expect.stringContaining("mcpServers")));
        expect(await screen.findByText("Setup copied.")).toBeInTheDocument();
    });

    it("a person sees listening, partial, and final voice states before searching", async () => {
        renderPreview();
        fireEvent.click(await screen.findByRole("button", { name: "Start voice input" }));
        expect(await screen.findByRole("button", { name: "Stop voice input" })).toBeInTheDocument();
        await act(async () => {
            await emit("voice://state", { version: 1, sessionId: "preview-voice-1", surface: "home_search", state: { kind: "listening", level: 0.4 } });
        });
        expect(screen.getByRole("progressbar", { name: "Microphone input level" })).toHaveAttribute("aria-valuenow", "40");
        await act(async () => {
            await emit("voice://state", { version: 1, sessionId: "preview-voice-1", surface: "home_search", state: { kind: "partial", text: "atlas planning" } });
        });
        expect(screen.getByRole("textbox", { name: "Search your memories" })).toHaveValue("atlas planning");
        fireEvent.click(screen.getByRole("button", { name: "Stop voice input" }));
        await act(async () => {
            await emit("voice://state", { version: 1, sessionId: "preview-voice-1", surface: "home_search", state: { kind: "final", text: "atlas planning notes" } });
        });
        expect(screen.getByRole("textbox", { name: "Search your memories" })).toHaveValue("atlas planning notes");
        expect(screen.getByText("Transcript ready: atlas planning notes")).toBeInTheDocument();
    });

    it("a person can open the Daily Brief from To-dos", async () => {
        renderPreview();
        fireEvent.click(await screen.findByRole("button", { name: "Open sidebar" }));
        fireEvent.click(screen.getByRole("button", { name: "To-dos" }));
        expect(await screen.findByText("Today's Briefing")).toBeInTheDocument();
        expect(await screen.findByText(/finish the keyboard walkthrough/i)).toBeInTheDocument();
    });
});
