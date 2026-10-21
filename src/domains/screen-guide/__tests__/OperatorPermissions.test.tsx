import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";

const mocks = vi.hoisted(() => ({
    computerUsePermissions: vi.fn(),
    openSystemSettings: vi.fn(),
}));

vi.mock("@/shared/ipc/tauri", () => ({ computerUsePermissions: mocks.computerUsePermissions }));
vi.mock("@/shared/ipc/onboarding", () => ({ openSystemSettings: mocks.openSystemSettings }));

import { OperatorPermissions } from "../OperatorPermissions";

afterEach(cleanup);

describe("OperatorPermissions", () => {
    it("names each permission's state in words and labels each settings button by permission", async () => {
        mocks.computerUsePermissions.mockResolvedValue({
            accessibility: true,
            screenRecording: false,
            automationMedia: null,
            backend: null,
            backendReady: null,
            backendDetail: null,
        });
        render(<OperatorPermissions />);

        const accessibility = (await screen.findByText("Accessibility")).closest("li") as HTMLElement;
        expect(within(accessibility).getByText("Allowed")).toBeInTheDocument();
        expect(within(accessibility).queryByRole("button")).toBeNull();

        const recording = screen.getByText("Screen Recording").closest("li") as HTMLElement;
        expect(within(recording).getByText("Not allowed")).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Open Screen Recording settings" })).toBeInTheDocument();
        expect(screen.getByRole("button", { name: "Open Automation settings" })).toBeInTheDocument();
    });

    it("reports a missing helper as information, not an alert", async () => {
        mocks.computerUsePermissions.mockResolvedValue({
            accessibility: true,
            screenRecording: true,
            automationMedia: true,
            backend: null,
            backendReady: null,
            backendDetail: null,
        });
        render(<OperatorPermissions />);

        expect(await screen.findByRole("status")).toHaveTextContent("No computer-use helper is installed.");
        expect(screen.queryByRole("alert")).toBeNull();
    });
});
