import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { CodexAccountStatus, CodexLoginCompleted } from "@/shared/ipc/tauri";

const eventHandlers = new Map<string, (payload: unknown) => void>();

vi.mock("@/shared/hooks/useTauriEvent", () => ({
    useTauriEvent: (event: string, handler: (payload: unknown) => void) => {
        eventHandlers.set(event, handler);
    },
}));
vi.mock("@/shared/utils/openExternalUrl", () => ({ openExternalUrl: vi.fn() }));
vi.mock("@/shared/ipc/tauri", () => ({
    CODEX_LOGIN_COMPLETED_EVENT: "codex-login-completed",
    codexAccountStatus: vi.fn(),
    codexLoginStart: vi.fn(),
    codexLoginCancel: vi.fn(),
    codexLogout: vi.fn(),
}));

import { codexAccountStatus, codexLoginCancel, codexLoginStart, codexLogout } from "@/shared/ipc/tauri";
import { openExternalUrl } from "@/shared/utils/openExternalUrl";
import { CodexAccountCard } from "./CodexAccountCard";

const signedOut: CodexAccountStatus = {
    cliState: "ready",
    cliPath: "/opt/homebrew/bin/codex",
    cliError: null,
    account: null,
    usableForHermes: false,
    primaryWindow: null,
    secondaryWindow: null,
    models: [],
};

const signedIn: CodexAccountStatus = {
    ...signedOut,
    account: { kind: "chatgpt", email: "ada@example.com", planType: "plus" },
    usableForHermes: true,
    primaryWindow: { usedPercent: 25, windowMinutes: 300, resetsAt: null },
    secondaryWindow: { usedPercent: 60, windowMinutes: 10080, resetsAt: null },
    models: [{ id: "gpt-6-sol", displayName: "GPT-6 Sol", isDefault: true }],
};

function completeLogin(payload: CodexLoginCompleted) {
    act(() => eventHandlers.get("codex-login-completed")?.(payload));
}

beforeEach(() => {
    eventHandlers.clear();
    vi.mocked(codexAccountStatus).mockReset();
    vi.mocked(codexLoginStart).mockReset();
    vi.mocked(codexLoginCancel).mockReset();
    vi.mocked(codexLogout).mockReset();
    vi.mocked(openExternalUrl).mockReset();
});

afterEach(cleanup);

describe("CodexAccountCard", () => {
    it("signs in through the browser and reports the account's models", async () => {
        vi.mocked(codexAccountStatus).mockResolvedValueOnce(signedOut).mockResolvedValueOnce(signedIn);
        vi.mocked(codexLoginStart).mockResolvedValue({ loginId: "login-1", authUrl: "https://auth.openai.com/x" });
        const onStatusChange = vi.fn();
        render(<CodexAccountCard onStatusChange={onStatusChange} />);

        fireEvent.click(await screen.findByRole("button", { name: "Sign in with ChatGPT" }));

        expect(await screen.findByRole("status")).toHaveTextContent("Finish signing in in your browser");
        expect(openExternalUrl).toHaveBeenCalledWith("https://auth.openai.com/x");

        completeLogin({ loginId: "login-1", success: true, error: null });

        expect(await screen.findByText("ChatGPT Plus")).toBeInTheDocument();
        expect(screen.getByRole("meter", { name: "5-hour window usage" })).toHaveAttribute("aria-valuenow", "25");
        expect(screen.getByRole("meter", { name: "This week usage" })).toHaveAttribute("aria-valuenow", "60");
        expect(onStatusChange).toHaveBeenLastCalledWith(signedIn);
    });

    it("cancels a pending sign-in and surfaces the failure", async () => {
        vi.mocked(codexAccountStatus).mockResolvedValue(signedOut);
        vi.mocked(codexLoginStart).mockResolvedValue({ loginId: "login-2", authUrl: "https://auth.openai.com/y" });
        render(<CodexAccountCard onStatusChange={vi.fn()} />);

        fireEvent.click(await screen.findByRole("button", { name: "Sign in with ChatGPT" }));
        fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));

        await waitFor(() => expect(codexLoginCancel).toHaveBeenCalledWith("login-2"));
        completeLogin({ loginId: "login-2", success: false, error: "Sign-in cancelled." });
        expect(await screen.findByRole("alert")).toHaveTextContent("Sign-in cancelled.");
        expect(screen.getByRole("button", { name: "Sign in with ChatGPT" })).toBeEnabled();
    });

    it("explains a broken Codex install instead of offering sign-in", async () => {
        vi.mocked(codexAccountStatus).mockResolvedValue({ ...signedOut, cliState: "broken", cliError: "exited" });
        render(<CodexAccountCard onStatusChange={vi.fn()} />);

        expect(await screen.findByText("Codex won't start")).toBeInTheDocument();
        expect(screen.getByText("npm install -g @openai/codex")).toBeInTheDocument();
        expect(screen.queryByRole("button", { name: "Sign in with ChatGPT" })).not.toBeInTheDocument();
    });

    it("signs out", async () => {
        vi.mocked(codexAccountStatus).mockResolvedValue(signedIn);
        vi.mocked(codexLogout).mockResolvedValue(signedOut);
        render(<CodexAccountCard onStatusChange={vi.fn()} />);

        fireEvent.click(await screen.findByRole("button", { name: "Sign out" }));

        expect(await screen.findByRole("button", { name: "Sign in with ChatGPT" })).toBeInTheDocument();
    });
});
