import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MotionWallpaper } from "./MotionWallpaper";

describe("MotionWallpaper", () => {
    afterEach(() => {
        cleanup();
        delete document.documentElement.dataset.motion;
        vi.restoreAllMocks();
    });

    it("uses the static CSS wallpaper when motion is disabled", () => {
        document.documentElement.dataset.motion = "off";
        const getContext = vi.spyOn(HTMLCanvasElement.prototype, "getContext");

        render(<MotionWallpaper />);

        expect(getContext).not.toHaveBeenCalled();
    });
});
