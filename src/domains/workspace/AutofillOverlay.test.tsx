import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const eventMocks = vi.hoisted(() => ({
    listen: vi.fn(),
}));

const ipcMocks = vi.hoisted(() => ({
    dismissAutofill: vi.fn(),
    injectText: vi.fn(),
    resolveAutofill: vi.fn(),
    setAutofillOverlayReady: vi.fn(),
    takePendingAutofillPayload: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => eventMocks);
vi.mock("@/shared/ipc/tauri", () => ipcMocks);

import type { AutofillOverlayEvent, FieldContext } from "@/shared/ipc/tauri";
import { AutofillOverlay } from "./AutofillOverlay";

const fieldContext: FieldContext = {
    label: "Policy number",
    placeholder: "",
    app_name: "Safari",
    bundle_id: "com.apple.Safari",
    window_title: "Application form",
    current_value: "",
    screen_context: "Policy number",
    inferred_label: "",
};

const resolution = {
    query: "Policy number",
    query_source: "field_label",
    context_hint: "Application form",
    candidates: [{
        value: "PN-123",
        confidence: 0.9,
        match_reason: "Matching field",
        source_snippet: "Policy number PN-123",
        source_app: "Safari",
        source_window_title: "Account",
        timestamp: Date.now(),
        memory_id: "memory-1",
    }],
    auto_inject_threshold: 0.95,
    requires_confirmation: true,
    used_ocr_fallback: false,
};

describe("AutofillOverlay request ownership", () => {
    let trigger: ((event: { payload: AutofillOverlayEvent }) => void) | null;

    beforeEach(() => {
        trigger = null;
        for (const mock of Object.values(ipcMocks)) mock.mockReset();
        eventMocks.listen.mockReset().mockImplementation(async (_name, handler) => {
            trigger = handler;
            return vi.fn();
        });
        ipcMocks.setAutofillOverlayReady.mockResolvedValue(null);
        ipcMocks.takePendingAutofillPayload.mockResolvedValue(null);
        ipcMocks.resolveAutofill.mockResolvedValue(resolution);
        ipcMocks.injectText.mockResolvedValue(undefined);
        ipcMocks.dismissAutofill.mockResolvedValue(true);
    });

    afterEach(() => cleanup());

    async function showPreview(requestId = 1) {
        render(<AutofillOverlay />);
        await waitFor(() => expect(trigger).not.toBeNull());
        await act(async () => {
            trigger?.({ payload: { requestId, payload: fieldContext } });
        });
        expect(await screen.findByRole("button", { name: /Insert Selected/i })).toBeInTheDocument();
    }

    it("replaces an older preview immediately when a newer request starts scanning", async () => {
        await showPreview(1);

        act(() => {
            trigger?.({
                payload: {
                    requestId: 2,
                    payload: { scanning: true, message: "Reading the focused field" },
                },
            });
        });

        expect(screen.queryByRole("button", { name: /Insert Selected/i })).not.toBeInTheDocument();
        expect(screen.getByText("Reading the focused field")).toBeInTheDocument();
        expect(screen.queryByText("PN-123")).not.toBeInTheDocument();
    });

    it("ignores an older injection completion after a newer request takes ownership", async () => {
        let finishInjection: (() => void) | null = null;
        ipcMocks.injectText.mockReturnValue(new Promise<void>((resolve) => {
            finishInjection = resolve;
        }));
        await showPreview(10);

        fireEvent.click(screen.getByRole("button", { name: /Insert Selected/i }));
        await waitFor(() => expect(ipcMocks.injectText).toHaveBeenCalledWith("PN-123", 10));

        act(() => {
            trigger?.({
                payload: {
                    requestId: 11,
                    payload: { scanning: true, message: "Reading the new field" },
                },
            });
        });
        await act(async () => {
            finishInjection?.();
            await Promise.resolve();
        });

        expect(screen.getByText("Reading the new field")).toBeInTheDocument();
        expect(screen.queryByText("Filled field")).not.toBeInTheDocument();
        expect(ipcMocks.dismissAutofill).not.toHaveBeenCalled();
    });
});
