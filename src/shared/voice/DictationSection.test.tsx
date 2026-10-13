import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DictationSection } from "./DictationSection";

const mocks = vi.hoisted(() => ({ get: vi.fn(), set: vi.fn() }));

vi.mock("@/shared/ipc/tauri", () => ({
    getDictationSettings: mocks.get,
    setDictationSettings: mocks.set,
}));

const OFF = { enabled: false, shortcut: "Control+Alt+D", learned_words: [] as string[] };

describe("DictationSection", () => {
    beforeEach(() => {
        mocks.get.mockResolvedValue(OFF);
        mocks.set.mockImplementation((settings) => Promise.resolve(settings));
    });
    afterEach(() => {
        cleanup();
        vi.clearAllMocks();
    });

    it("loads the saved settings and stays off until asked", async () => {
        render(<DictationSection />);
        const toggle = await screen.findByRole("switch", { name: /dictate into any app/i });
        await waitFor(() => expect(toggle).toBeEnabled());
        expect(toggle).not.toBeChecked();
        expect(screen.getByLabelText(/shortcut/i)).toHaveValue("Control+Alt+D");
    });

    it("turns dictation on with the saved shortcut and words intact", async () => {
        mocks.get.mockResolvedValue({ ...OFF, learned_words: ["Kunj"] });
        render(<DictationSection />);
        const toggle = await screen.findByRole("switch");
        await waitFor(() => expect(toggle).toBeEnabled());
        fireEvent.click(toggle);
        await waitFor(() =>
            expect(mocks.set).toHaveBeenCalledWith({ enabled: true, shortcut: "Control+Alt+D", learned_words: ["Kunj"] }),
        );
        await waitFor(() => expect(toggle).toBeChecked());
    });

    it("saves a changed shortcut when the field loses focus", async () => {
        render(<DictationSection />);
        const field = await screen.findByLabelText(/shortcut/i);
        await waitFor(() => expect(field).toBeEnabled());
        fireEvent.change(field, { target: { value: "Control+Alt+J" } });
        expect(mocks.set).not.toHaveBeenCalled();
        fireEvent.blur(field);
        await waitFor(() => expect(mocks.set).toHaveBeenCalledWith({ ...OFF, shortcut: "Control+Alt+J" }));
    });

    it("saves words one per line, without blanks", async () => {
        render(<DictationSection />);
        const field = await screen.findByLabelText(/words to spell/i);
        await waitFor(() => expect(field).toBeEnabled());
        fireEvent.change(field, { target: { value: "Priyanka\n\n  LanceDB  \n" } });
        fireEvent.blur(field);
        await waitFor(() => expect(mocks.set).toHaveBeenCalledWith({ ...OFF, learned_words: ["Priyanka", "LanceDB"] }));
    });

    it("shows the refusal and restores the shortcut when the backend rejects it", async () => {
        mocks.set.mockRejectedValue(new Error("Dictation shortcut 'Alt+Space' conflicts with Omnibar."));
        render(<DictationSection />);
        const field = await screen.findByLabelText(/shortcut/i);
        await waitFor(() => expect(field).toBeEnabled());
        fireEvent.change(field, { target: { value: "Alt+Space" } });
        fireEvent.blur(field);
        expect(await screen.findByRole("alert")).toHaveTextContent("conflicts with Omnibar");
        expect(field).toHaveValue("Control+Alt+D");
    });
});
