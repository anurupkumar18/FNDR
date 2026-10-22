import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, expect, it } from "vitest";
import { getVerbatimMemoryEnabled, setVerbatimMemoryEnabled } from "./tauri";

afterEach(clearMocks);

it("reads and writes the full text switch through the boolean IPC contract", async () => {
    let enabled = false;
    mockIPC((command, payload) => {
        if (command === "get_verbatim_memory_enabled") return enabled;
        if (command === "set_verbatim_memory_enabled") {
            expect(payload).toEqual({ enabled: !enabled });
            enabled = (payload as { enabled: boolean }).enabled;
            return undefined;
        }
        throw new Error(`Unexpected command: ${command}`);
    });

    expect(await getVerbatimMemoryEnabled()).toBe(false);
    await setVerbatimMemoryEnabled(true);
    expect(await getVerbatimMemoryEnabled()).toBe(true);
    await setVerbatimMemoryEnabled(false);
    expect(await getVerbatimMemoryEnabled()).toBe(false);
});
