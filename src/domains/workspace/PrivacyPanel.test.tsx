import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { PrivacyPanel } from "./PrivacyPanel";

const getPrivacyAlerts = vi.fn().mockResolvedValue([]);

vi.mock("@/shared/ipc/tauri", () => ({
    PRIVACY_ALERTS_EVENT: "privacy://alerts",
    getPrivacyAlerts: (...args: unknown[]) => getPrivacyAlerts(...args),
    getBlocklist: vi.fn().mockResolvedValue([]),
    addSiteToBlocklist: vi.fn(),
    dismissPrivacyAlert: vi.fn(),
}));

vi.mock("@/shared/hooks/useTauriEvent", () => ({
    useTauriEvent: vi.fn(),
}));

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("PrivacyPanel", () => {
    it("describes an empty alert queue without claiming the user's data is secure", async () => {
        render(<PrivacyPanel isVisible onClose={() => {}} embedded />);

        expect(screen.getByText("No recent apps or sites need your review.")).toBeInTheDocument();
        expect(
            screen.getByText(/detection can miss sensitive content\. use blocklists or pause capture/i),
        ).toBeInTheDocument();
        expect(screen.queryByText(/your data is secure/i)).toBeNull();
        await waitFor(() => expect(getPrivacyAlerts).toHaveBeenCalled());
    });
});
