import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { getAgentNotesEnabled, setAgentNotesEnabled } from "./tauri";

afterEach(clearMocks);

it("reads and writes assistant note consent through the boolean IPC contract", async () => {
    let enabled = false;
    mockIPC((command, payload) => {
        if (command === "get_agent_notes_enabled") return enabled;
        if (command === "set_agent_notes_enabled") {
            expect(payload).toEqual({ enabled: !enabled });
            enabled = (payload as { enabled: boolean }).enabled;
            return undefined;
        }
        throw new Error(`Unexpected command: ${command}`);
    });

    expect(await getAgentNotesEnabled()).toBe(false);
    await setAgentNotesEnabled(true);
    expect(await getAgentNotesEnabled()).toBe(true);
    await setAgentNotesEnabled(false);
    expect(await getAgentNotesEnabled()).toBe(false);
});
