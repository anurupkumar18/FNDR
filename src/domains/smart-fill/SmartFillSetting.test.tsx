import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SmartFillSetting } from "./SmartFillSetting";

const ipc = vi.hoisted(() => ({
    getSmartFillEnabled: vi.fn(),
    setSmartFillEnabled: vi.fn(),
    getSavedDetailsCount: vi.fn(),
    forgetSavedDetails: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ipc);

describe("SmartFillSetting", () => {
    afterEach(cleanup);

    beforeEach(() => {
        ipc.getSmartFillEnabled.mockReset().mockResolvedValue(false);
        ipc.setSmartFillEnabled.mockReset().mockImplementation(async (next: boolean) => next);
        ipc.getSavedDetailsCount.mockReset().mockResolvedValue(0);
        ipc.forgetSavedDetails.mockReset().mockResolvedValue(0);
    });

    it("starts off and says what leaves the Mac", async () => {
        render(<SmartFillSetting />);
        const toggle = await screen.findByRole("switch", { name: "Smart Fill" });
        await waitFor(() => expect(toggle).not.toBeDisabled());
        expect(toggle).not.toBeChecked();
        expect(screen.getByText(/screenshot of your/i)).toBeInTheDocument();
        expect(screen.getByText(/up to four related memories/i)).toBeInTheDocument();
        expect(screen.getByText(/never writes into password fields/i)).toBeInTheDocument();
    });

    it("says it saves details, shows how many, and forgets them on request", async () => {
        ipc.getSavedDetailsCount.mockResolvedValue(10);
        render(<SmartFillSetting />);
        expect(await screen.findByText(/saves labelled details it sees on screen/i)).toBeInTheDocument();
        expect(screen.getByText(/stored page text of those memories/i)).toBeInTheDocument();
        expect(screen.getByText(/names of your recent files \(names only, nothing opened\)/i)).toBeInTheDocument();
        expect(await screen.findByText(/10 personal details saved on this Mac/i)).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: "Forget saved details" }));
        await waitFor(() => expect(ipc.forgetSavedDetails).toHaveBeenCalled());
        await waitFor(() => expect(screen.queryByText(/personal details saved/i)).not.toBeInTheDocument());
    });

    it("saves only the Smart Fill switch", async () => {
        render(<SmartFillSetting />);
        const toggle = await screen.findByRole("switch", { name: "Smart Fill" });
        await waitFor(() => expect(toggle).not.toBeDisabled());
        fireEvent.click(toggle);
        await waitFor(() => expect(ipc.setSmartFillEnabled).toHaveBeenCalledWith(true));
        await waitFor(() => expect(toggle).toBeChecked());
    });

    it("shows the reason when saving fails and stays off", async () => {
        ipc.setSmartFillEnabled.mockRejectedValueOnce("The setting could not be saved.");
        render(<SmartFillSetting />);
        const toggle = await screen.findByRole("switch", { name: "Smart Fill" });
        await waitFor(() => expect(toggle).not.toBeDisabled());
        fireEvent.click(toggle);
        expect(await screen.findByRole("alert")).toHaveTextContent("The setting could not be saved.");
        expect(toggle).not.toBeChecked();
    });
});
