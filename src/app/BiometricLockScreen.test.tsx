import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
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
    });
});
