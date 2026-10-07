import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { VoicePreviewGallery } from "./VoicePreviewGallery";

vi.mock("@/shared/motion/useReducedMotionSafe", () => ({
    useReducedMotionSafe: () => ({ reduced: false }),
}));

afterEach(cleanup);

it("shows every shared voice state and both control modes", () => {
    render(<VoicePreviewGallery />);

    for (const label of [
        "Idle",
        "Requesting permission",
        "Preparing model",
        "Listening",
        "Partial transcript",
        "Final transcript",
        "Error",
        "Unavailable",
    ]) {
        expect(screen.getByRole("heading", { name: label })).toBeInTheDocument();
    }
    expect(screen.getByRole("button", { name: "Start voice input" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Hold to speak" })).toBeInTheDocument();
});
