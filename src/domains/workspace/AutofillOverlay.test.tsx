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

    it("names the focused context and exposes candidate selection semantics", async () => {
        ipcMocks.resolveAutofill.mockResolvedValue({
            ...resolution,
            candidates: [
                resolution.candidates[0],
                {
                    ...resolution.candidates[0],
                    value: "PN-456",
                    confidence: 0.82,
                    memory_id: "memory-2",
                },
            ],
        });
        await showPreview();

        expect(screen.getByRole("searchbox", { name: "Memory search for this field" })).toHaveValue(
            "Policy number",
        );
        expect(screen.getByLabelText("Focused app")).toHaveTextContent("Safari");
        expect(screen.getByLabelText("Focused window")).toHaveTextContent("Application form");
        expect(screen.getAllByRole("option")[0]).toHaveAttribute("aria-selected", "true");
        expect(screen.getAllByRole("option")[1]).toHaveAttribute("aria-selected", "false");
    });

    it("discloses when visible text was needed to identify the field", async () => {
        ipcMocks.resolveAutofill.mockResolvedValue({
            ...resolution,
            used_ocr_fallback: true,
        });
        await showPreview();

        expect(screen.getByText(
            "FNDR used nearby visible text because the field label was not available.",
        )).toBeInTheDocument();
    });

    it("focuses manual search when field context is not specific enough", async () => {
        render(<AutofillOverlay />);
        await waitFor(() => expect(trigger).not.toBeNull());
        await act(async () => {
            trigger?.({
                payload: {
                    requestId: 4,
                    payload: {
                        ...fieldContext,
                        label: "",
                        screen_context: "",
                    },
                },
            });
        });

        const search = screen.getByRole("searchbox", { name: "Memory search for this field" });
        await waitFor(() => expect(search).toHaveFocus());
        expect(screen.getByText("Search memory for this field")).toBeInTheDocument();
    });

    it("does not pretend to search when no focused-field request is available", async () => {
        render(<AutofillOverlay />);
        await waitFor(() => expect(eventMocks.listen).toHaveBeenCalled());

        fireEvent.focus(window);

        expect(await screen.findByText("No field context available")).toBeInTheDocument();
        expect(screen.getByText("Focus a text field, then run Autofill again.")).toBeInTheDocument();
        expect(screen.queryByText("Searching memories")).not.toBeInTheDocument();
    });

    it("turns an accessibility denial into an actionable permission state", async () => {
        render(<AutofillOverlay />);
        await waitFor(() => expect(trigger).not.toBeNull());
        act(() => {
            trigger?.({
                payload: {
                    requestId: 7,
                    payload: { error: "Accessibility permission denied" },
                },
            });
        });

        expect(await screen.findByRole("alert")).toHaveTextContent("Autofill needs Accessibility permission");
        expect(screen.getByRole("alert")).toHaveTextContent(
            "Allow FNDR in System Settings, then focus the field and try again.",
        );
    });

    it("explains when the target field changes before insertion", async () => {
        ipcMocks.injectText.mockRejectedValue(new Error("No autofill target stored"));
        await showPreview(8);

        fireEvent.click(screen.getByRole("button", { name: /Insert Selected/i }));

        expect(await screen.findByRole("alert")).toHaveTextContent("The focused field changed");
        expect(screen.getByRole("alert")).toHaveTextContent(
            "Focus the destination field and run Autofill again.",
        );
    });

    it("re-runs an edited query and inserts the candidate the user selects", async () => {
        const twoCandidates = {
            ...resolution,
            candidates: [
                resolution.candidates[0],
                {
                    ...resolution.candidates[0],
                    value: "PN-456",
                    confidence: 0.82,
                    memory_id: "memory-2",
                },
            ],
        };
        ipcMocks.resolveAutofill.mockResolvedValue(twoCandidates);
        await showPreview(15);

        const search = screen.getByRole("searchbox", { name: "Memory search for this field" });
        fireEvent.change(search, { target: { value: "Member ID" } });
        fireEvent.click(screen.getByRole("button", { name: "Search" }));
        await waitFor(() => expect(ipcMocks.resolveAutofill).toHaveBeenLastCalledWith(
            fieldContext,
            "Member ID",
        ));

        const options = await screen.findAllByRole("option");
        fireEvent.click(options[1]);
        fireEvent.click(screen.getByRole("button", { name: /Insert Selected/i }));
        await waitFor(() => expect(ipcMocks.injectText).toHaveBeenCalledWith("PN-456", 15));
    });

    it("dismisses the active request from both the button and Escape", async () => {
        await showPreview(20);
        fireEvent.click(screen.getByRole("button", { name: "Dismiss Autofill" }));
        await waitFor(() => expect(ipcMocks.dismissAutofill).toHaveBeenCalledWith(20));

        await act(async () => {
            trigger?.({ payload: { requestId: 21, payload: fieldContext } });
        });
        expect(await screen.findByRole("button", { name: /Insert Selected/i })).toBeInTheDocument();
        fireEvent.keyDown(window, { key: "Escape" });
        await waitFor(() => expect(ipcMocks.dismissAutofill).toHaveBeenCalledWith(21));
    });
});
