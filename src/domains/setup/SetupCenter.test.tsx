import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

const ipc = vi.hoisted(() => ({
    setupComponents: vi.fn(),
    installComponent: vi.fn(),
    codexLoginStart: vi.fn(),
    mcpConnectStatus: vi.fn(),
    mcpConnectApply: vi.fn(),
    mcpConnectDisconnect: vi.fn(),
    mcpConnectPreview: vi.fn(),
    mcpConnectTest: vi.fn(),
    mcpConnectRotateToken: vi.fn(),
    mcpConnectRevoke: vi.fn(),
    mcpConnectSetKeepRunning: vi.fn(),
    mcpConnectSetHideMore: vi.fn(),
}));
const updater = vi.hoisted(() => ({ check: vi.fn() }));

vi.mock("@/shared/ipc/tauri", () => ipc);
vi.mock("@tauri-apps/plugin-updater", () => updater);
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@/shared/utils/openExternalUrl", () => ({ openExternalUrl: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@/domains/screen-guide/OperatorPermissions", () => ({ OperatorPermissions: () => <div>permissions</div> }));
vi.mock("./CalendarMeetingPrep", () => ({ CalendarMeetingPrep: () => <div>calendar</div> }));
vi.mock("./ImportSources", () => ({ ImportSources: () => <div>imports</div> }));

import { SetupCenter } from "./SetupCenter";

const components = [
    { id: "codex_cli", name: "Codex", purpose: "Signs in", required: true, state: "ready", version: "codex-cli 0.153.4", detail: null, action: null, url: null },
    { id: "chatgpt", name: "ChatGPT sign-in", purpose: "One sign-in", required: true, state: "missing", version: null, detail: null, action: "sign_in", url: null },
    { id: "computer_use", name: "Computer Use", purpose: "Clicks", required: false, state: "missing", version: null, detail: null, action: "install", url: null },
    { id: "hermes", name: "Hermes agent", purpose: "Agent", required: false, state: "ready", version: "v2026.7.7.2", detail: null, action: null, url: null },
];

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

const mcpClients = [
    { id: "claude_code", name: "Claude Code", state: "not_connected", detected: true, configPath: "/tmp/.claude.json", detail: null, missingTool: null },
    { id: "claude_desktop", name: "Claude Desktop", state: "not_connected", detected: true, configPath: "/tmp/claude_desktop_config.json", detail: null, missingTool: null },
    { id: "cursor", name: "Cursor", state: "not_connected", detected: true, configPath: "/tmp/mcp.json", detail: null, missingTool: null },
];

beforeEach(() => {
    ipc.mcpConnectStatus.mockResolvedValue({ running: false, endpoint: null, actionsBlocked: false, keepRunning: true, hideMore: false, clients: mcpClients });
});

describe("SetupCenter", () => {
    it("connects each MCP client from Setup with one click and refreshes its state", async () => {
        ipc.setupComponents.mockResolvedValue(components);
        ipc.mcpConnectStatus
            .mockResolvedValueOnce({ running: false, endpoint: null, actionsBlocked: false, keepRunning: true, clients: mcpClients })
            .mockResolvedValue({ running: true, endpoint: "http://127.0.0.1:4000/mcp", actionsBlocked: false, keepRunning: true, clients: [
                { ...mcpClients[0], state: "connected" }, ...mcpClients.slice(1),
            ] });
        ipc.mcpConnectApply.mockResolvedValue({ message: "Claude Code is connected. Start a new session to use it.", changed: true });
        updater.check.mockResolvedValue(null);
        render(<SetupCenter />);

        expect(await screen.findByText("Connect your AI tools")).toBeInTheDocument();
        for (const client of mcpClients) {
            expect(screen.getByRole("listitem", { name: client.name })).toBeInTheDocument();
        }
        fireEvent.click(within(screen.getByRole("listitem", { name: "Claude Code" })).getByRole("button", { name: "Connect" }));
        await waitFor(() => expect(ipc.mcpConnectApply).toHaveBeenCalledWith("claude_code"));
        await waitFor(() => expect(within(screen.getByRole("listitem", { name: "Claude Code" })).getByText("Connected")).toBeInTheDocument());
    });

    it("saves the choice to keep MCP available after launch", async () => {
        ipc.setupComponents.mockResolvedValue(components);
        ipc.mcpConnectSetKeepRunning.mockResolvedValue({ running: false, endpoint: null, actionsBlocked: false, keepRunning: false, clients: mcpClients });
        updater.check.mockResolvedValue(null);
        render(<SetupCenter />);

        const choice = await screen.findByRole("checkbox", { name: "Keep the MCP server running" });
        expect(choice).toBeChecked();
        fireEvent.click(choice);
        await waitFor(() => expect(ipc.mcpConnectSetKeepRunning).toHaveBeenCalledWith(false));
    });

    it("offers a stricter privacy choice for AI-tool results", async () => {
        ipc.setupComponents.mockResolvedValue(components);
        ipc.mcpConnectSetHideMore.mockResolvedValue(undefined);
        updater.check.mockResolvedValue(null);
        render(<SetupCenter />);

        const choice = await screen.findByRole("checkbox", { name: "Hide more in AI-tool results" });
        expect(choice).not.toBeChecked();
        fireEvent.click(choice);
        await waitFor(() => expect(ipc.mcpConnectSetHideMore).toHaveBeenCalledWith(true));
    });
    it("lists every component with its state and installs what FNDR can install", async () => {
        ipc.setupComponents.mockResolvedValue(components);
        ipc.installComponent.mockResolvedValue(undefined);
        updater.check.mockResolvedValue(null);
        render(<SetupCenter />);

        const row = await screen.findByRole("listitem", { name: "Computer Use" });
        expect(within(row).getByText("Not installed")).toBeInTheDocument();
        fireEvent.click(within(row).getByRole("button", { name: "Install" }));
        await waitFor(() => expect(ipc.installComponent).toHaveBeenCalledWith("computer_use"));
        await waitFor(() => expect(ipc.setupComponents).toHaveBeenCalledTimes(2));

        const codex = screen.getByRole("listitem", { name: "Codex" });
        expect(within(codex).getByText("codex-cli 0.153.4")).toBeInTheDocument();
        fireEvent.click(within(screen.getByRole("listitem", { name: "ChatGPT sign-in" })).getByRole("button", { name: "Sign in" }));
        await waitFor(() => expect(ipc.codexLoginStart).toHaveBeenCalled());
    });

    it("offers no way to move Hermes off the version FNDR was checked against", async () => {
        ipc.setupComponents.mockResolvedValue(components);
        updater.check.mockResolvedValue(null);
        render(<SetupCenter />);

        const row = await screen.findByRole("listitem", { name: "Hermes agent" });
        expect(within(row).queryByRole("button", { name: /update/i })).not.toBeInTheDocument();
    });

    it("offers an FNDR update when one is published", async () => {
        ipc.setupComponents.mockResolvedValue(components);
        const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
        updater.check.mockResolvedValue({ version: "0.3.1", downloadAndInstall });
        render(<SetupCenter />);

        fireEvent.click(await screen.findByRole("button", { name: "Update FNDR to 0.3.1" }));
        await waitFor(() => expect(downloadAndInstall).toHaveBeenCalled());
    });
});
