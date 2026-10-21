import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SmartFillSetting } from "./SmartFillSetting";

const ipc = vi.hoisted(() => ({
    getSmartFillEnabled: vi.fn(),
    setSmartFillEnabled: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ipc);

describe("SmartFillSetting", () => {
    afterEach(cleanup);

    beforeEach(() => {
        ipc.getSmartFillEnabled.mockReset().mockResolvedValue(false);
        ipc.setSmartFillEnabled.mockReset().mockImplementation(async (next: boolean) => next);
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
