import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

const ipc = vi.hoisted(() => ({
    setupComponents: vi.fn(),
    installComponent: vi.fn(),
    codexLoginStart: vi.fn(),
}));
const updater = vi.hoisted(() => ({ check: vi.fn() }));

vi.mock("@/shared/ipc/tauri", () => ipc);
vi.mock("@tauri-apps/plugin-updater", () => updater);
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@/shared/utils/openExternalUrl", () => ({ openExternalUrl: vi.fn().mockResolvedValue(undefined) }));
vi.mock("@/domains/screen-guide/OperatorPermissions", () => ({ OperatorPermissions: () => <div>permissions</div> }));
vi.mock("./CalendarMeetingPrep", () => ({ CalendarMeetingPrep: () => <div>calendar</div> }));

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

describe("SetupCenter", () => {
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
