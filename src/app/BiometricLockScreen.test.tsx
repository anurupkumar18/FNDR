import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { BiometricLockScreen } from "./BiometricLockScreen";

const requestBiometricAuthMock = vi.fn();

vi.mock("@/shared/ipc/onboarding", () => ({
    requestBiometricAuth: (...args: unknown[]) => requestBiometricAuthMock(...args),
}));

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("BiometricLockScreen", () => {
    it("shows the real macOS authentication request while it is pending", async () => {
        requestBiometricAuthMock.mockReturnValue(new Promise(() => {}));

        render(<BiometricLockScreen onUnlock={() => {}} />);

        const trace = await screen.findByRole("region", { name: "Unlock activity" });
        expect(within(trace).getByRole("status")).toHaveTextContent(
            "Waiting for macOS authentication",
        );
        expect(within(trace).getByRole("status")).toHaveTextContent("Running");
    });

    it("stays locked after a cancelled or failed authentication", async () => {
        requestBiometricAuthMock.mockResolvedValue(false);
        const onUnlock = vi.fn();

        render(<BiometricLockScreen onUnlock={onUnlock} />);

        expect(await screen.findByRole("alert")).toHaveTextContent(/authentication was not completed/i);
        expect(onUnlock).not.toHaveBeenCalled();
        expect(
            screen.queryByRole("button", { name: /continue without biometric lock/i }),
        ).not.toBeInTheDocument();
        expect(screen.getByRole("button", { name: /try again/i })).toBeInTheDocument();

        const trace = screen.getByRole("region", { name: "Unlock activity" });
        expect(within(trace).getByText("Authentication was not completed")).toBeInTheDocument();
        expect(within(trace).getByText("Failed")).toBeInTheDocument();

        fireEvent.click(within(trace).getByRole("button", { name: "Show Unlock activity details" }));
        expect(trace).toHaveTextContent("Request boundary");
        expect(trace).toHaveTextContent("Verified result");
        expect(within(trace).queryByText("Running")).not.toBeInTheDocument();
    });

    it("unlocks only after macOS confirms authentication", async () => {
        requestBiometricAuthMock
            .mockResolvedValueOnce(false)
            .mockResolvedValueOnce(true);
        const onUnlock = vi.fn();

        render(<BiometricLockScreen onUnlock={onUnlock} />);
        await screen.findByRole("button", { name: /try again/i });

        fireEvent.click(screen.getByRole("button", { name: /try again/i }));

        await waitFor(() => expect(onUnlock).toHaveBeenCalledTimes(1));
        const trace = screen.getByRole("region", { name: "Unlock activity" });
        expect(within(trace).getByText("Authentication confirmed")).toBeInTheDocument();
        expect(within(trace).getByText("Completed")).toBeInTheDocument();
    });

    it("shows a bounded failure without rendering the native error", async () => {
        requestBiometricAuthMock.mockRejectedValue(
            new Error("/Users/person/private.db prompt=https://private.example"),
        );

        render(<BiometricLockScreen onUnlock={() => {}} />);

        expect(await screen.findByRole("alert")).toHaveTextContent(
            "Authentication is unavailable right now",
        );
        const trace = screen.getByRole("region", { name: "Unlock activity" });
        expect(within(trace).getByText("macOS authentication is unavailable")).toBeInTheDocument();
        expect(within(trace).getByText("Failed")).toBeInTheDocument();
        expect(screen.queryByText(/private\.db|private\.example/)).not.toBeInTheDocument();
    });
});
