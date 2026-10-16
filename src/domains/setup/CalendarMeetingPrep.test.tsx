import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const ipc = vi.hoisted(() => ({
    calendarMeetingPrepState: vi.fn(),
    setCalendarMeetingPrep: vi.fn(),
    openCalendarPrivacySettings: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ipc);

import { CalendarMeetingPrep } from "./CalendarMeetingPrep";

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

const toggle = () => screen.getByRole("switch", { name: "Meeting prep from your calendar" });

describe("CalendarMeetingPrep", () => {
    it("is off by default and turning it on asks for access", async () => {
        ipc.calendarMeetingPrepState.mockResolvedValue({ enabled: false, status: "not_determined" });
        ipc.setCalendarMeetingPrep.mockResolvedValue({ enabled: true, status: "granted" });
        render(<CalendarMeetingPrep />);

        await waitFor(() => expect(toggle()).not.toBeDisabled());
        expect(toggle()).not.toBeChecked();
        expect(screen.getByText(/asks macOS for calendar access/)).toBeInTheDocument();

        fireEvent.click(toggle());
        await waitFor(() => expect(ipc.setCalendarMeetingPrep).toHaveBeenCalledWith(true));
        await waitFor(() => expect(toggle()).toBeChecked());
        expect(screen.getByText(/never stored or sent anywhere/)).toBeInTheDocument();
    });

    it("stays off when access is denied and offers System Settings", async () => {
        ipc.calendarMeetingPrepState.mockResolvedValue({ enabled: false, status: "not_determined" });
        ipc.setCalendarMeetingPrep.mockResolvedValue({ enabled: false, status: "denied" });
        render(<CalendarMeetingPrep />);

        await waitFor(() => expect(toggle()).not.toBeDisabled());
        fireEvent.click(toggle());
        const open = await screen.findByRole("button", { name: "Open System Settings" });
        expect(toggle()).not.toBeChecked();
        fireEvent.click(open);
        expect(ipc.openCalendarPrivacySettings).toHaveBeenCalled();
    });

    it("turns off without asking again and shows a failure", async () => {
        ipc.calendarMeetingPrepState.mockResolvedValue({ enabled: true, status: "granted" });
        ipc.setCalendarMeetingPrep.mockRejectedValue(new Error("Could not save the setting"));
        render(<CalendarMeetingPrep />);

        await waitFor(() => expect(toggle()).toBeChecked());
        fireEvent.click(toggle());
        await waitFor(() => expect(ipc.setCalendarMeetingPrep).toHaveBeenCalledWith(false));
        expect(await screen.findByRole("alert")).toHaveTextContent("Could not save the setting");
        expect(toggle()).toBeChecked();
    });
});
