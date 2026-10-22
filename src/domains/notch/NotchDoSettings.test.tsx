import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

const ipcMocks = vi.hoisted(() => ({
    getNotchDoAutonomy: vi.fn(),
    setNotchDoAutonomy: vi.fn(),
    notchDoDeleteSkill: vi.fn(),
    notchDoRateRun: vi.fn(),
    notchDoRequest: vi.fn(),
    notchDoRuns: vi.fn(),
    notchDoSaveSkill: vi.fn(),
    notchDoSkills: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ipcMocks);

import { NotchDoSettings } from "./NotchDoSettings";

const run = (overrides: Record<string, unknown>) => ({
    runId: "r1",
    at: "2026-10-15T15:00:00Z",
    request: "open Notes",
    plannedBy: "local",
    steps: ["Open Notes"],
    ok: true,
    summary: "Done: Open Notes.",
    rating: null,
    repeatable: true,
    ...overrides,
});

const mondaySetup = {
    id: "s1",
    name: "Monday setup",
    request: "open Notes and Slack",
    steps: ["Open Notes", "Open Slack"],
    savedAt: "2026-10-14T15:00:00Z",
};

describe("NotchDoSettings", () => {
    it("asks only before destructive actions by default and can go back to asking each time", async () => {
        ipcMocks.notchDoSkills.mockResolvedValue([]);
        ipcMocks.notchDoRuns.mockResolvedValue([]);
        render(<NotchDoSettings />);
        const toggle = await screen.findByRole("switch", { name: "Ask before each action" });
        await waitFor(() => expect(toggle).not.toBeDisabled());
        expect(toggle).not.toBeChecked();
        expect(screen.getByText(/could destroy data/i)).toBeInTheDocument();
        expect(screen.getByText(/never pays, enters a password or force quits/i)).toBeInTheDocument();
        fireEvent.click(toggle);
        await waitFor(() => expect(ipcMocks.setNotchDoAutonomy).toHaveBeenCalledWith("ask_each"));
        await waitFor(() => expect(toggle).toBeChecked());
        fireEvent.click(toggle);
        await waitFor(() => expect(ipcMocks.setNotchDoAutonomy).toHaveBeenLastCalledWith("smooth"));
    });

    beforeEach(() => {
        Object.values(ipcMocks).forEach((mock) => mock.mockReset());
        ipcMocks.getNotchDoAutonomy.mockResolvedValue("smooth");
        ipcMocks.setNotchDoAutonomy.mockImplementation(async (value: string) => value);
        ipcMocks.notchDoSkills.mockResolvedValue([mondaySetup]);
        ipcMocks.notchDoRuns.mockResolvedValue([]);
    });
    afterEach(cleanup);

    it("runs a skill by handing its name to Notch Do", async () => {
        render(<NotchDoSettings />);
        const skills = await screen.findByRole("list", { name: "Skills" });
        expect(within(skills).getByText("Open Notes · Open Slack")).toBeTruthy();
        fireEvent.click(within(skills).getByRole("button", { name: "Run" }));
        await waitFor(() => expect(ipcMocks.notchDoRequest).toHaveBeenCalledWith("Monday setup"));
    });

    it("says why a skill did not run", async () => {
        ipcMocks.notchDoRequest.mockRejectedValue('Turn on "Operate my Mac" in Settings for FNDR to do this.');
        render(<NotchDoSettings />);
        fireEvent.click(await screen.findByRole("button", { name: "Run" }));
        expect((await screen.findByRole("alert")).textContent).toContain("Operate my Mac");
    });

    it("deletes a skill only after a second, named tap", async () => {
        ipcMocks.notchDoDeleteSkill.mockResolvedValue(true);
        render(<NotchDoSettings />);
        fireEvent.click(await screen.findByRole("button", { name: "Delete Monday setup" }));
        expect(ipcMocks.notchDoDeleteSkill).not.toHaveBeenCalled();
        fireEvent.click(screen.getByRole("button", { name: "Keep" }));
        fireEvent.click(screen.getByRole("button", { name: "Delete Monday setup" }));
        expect(screen.getByText("Delete it?")).toBeTruthy();
        fireEvent.click(screen.getByRole("button", { name: "Delete" }));
        await waitFor(() => expect(ipcMocks.notchDoDeleteSkill).toHaveBeenCalledWith("s1"));
        expect(await screen.findByText(/No skills yet/)).toBeTruthy();
    });

    it("rates a run after the notch has closed, and saves one that worked as a skill", async () => {
        ipcMocks.notchDoRuns.mockResolvedValue([run({})]);
        ipcMocks.notchDoRateRun.mockResolvedValue(undefined);
        ipcMocks.notchDoSaveSkill.mockResolvedValue("Notes");
        render(<NotchDoSettings />);
        const runs = await screen.findByRole("list", { name: "Recent runs" });
        expect(within(runs).getByText(/Planned on this Mac · Done/)).toBeTruthy();
        expect(within(runs).queryByLabelText(/Skill name/)).toBeNull();

        fireEvent.click(within(runs).getByRole("button", { name: "Worked" }));
        await waitFor(() => expect(ipcMocks.notchDoRateRun).toHaveBeenCalledWith("r1", true));
        const name = await within(runs).findByLabelText("Skill name for “open Notes”");
        fireEvent.change(name, { target: { value: "Notes" } });
        ipcMocks.notchDoSkills.mockResolvedValue([mondaySetup, { ...mondaySetup, id: "s2", name: "Notes" }]);
        fireEvent.click(within(runs).getByRole("button", { name: "Save" }));
        await waitFor(() => expect(ipcMocks.notchDoSaveSkill).toHaveBeenCalledWith("r1", "Notes"));
        expect(await within(runs).findByText("Saved as “Notes”.")).toBeTruthy();
        expect(within(screen.getByRole("list", { name: "Skills" })).getByText("Notes")).toBeTruthy();
    });

    it("keeps a wrong run as a test case and offers no skill for it, a work set or a skill run", async () => {
        ipcMocks.notchDoRuns.mockResolvedValue([
            run({ runId: "wrong" }),
            run({ runId: "set", request: "pull up the essay", repeatable: false, rating: "worked" }),
            run({ runId: "again", request: "Monday setup", plannedBy: "skill", rating: "worked" }),
            run({ runId: "open", request: "open Slack", ok: null }),
        ]);
        ipcMocks.notchDoRateRun.mockResolvedValue(undefined);
        render(<NotchDoSettings />);
        const runs = await screen.findByRole("list", { name: "Recent runs" });
        expect(within(runs).getAllByRole("button", { name: "Worked" })).toHaveLength(1);
        fireEvent.click(within(runs).getByRole("button", { name: "Didn't work" }));
        await waitFor(() => expect(ipcMocks.notchDoRateRun).toHaveBeenCalledWith("wrong", false));
        expect(await within(runs).findByText(/Kept as a test case/)).toBeTruthy();
        expect(within(runs).queryByLabelText(/Skill name/)).toBeNull();
        expect(within(runs).getByText(/Did not end/)).toBeTruthy();
    });
});
