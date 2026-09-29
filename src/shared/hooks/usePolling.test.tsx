import { act, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { usePolling } from "./usePolling";

function Harness({ callback }: { callback: () => Promise<void> }) {
    usePolling(callback, 100, true);
    return null;
}

afterEach(() => {
    vi.useRealTimers();
});

describe("usePolling", () => {
    it("does not overlap refreshes when an earlier poll is still running", async () => {
        vi.useFakeTimers();
        let finishFirst!: () => void;
        const callback = vi.fn()
            .mockImplementationOnce(() => new Promise<void>((resolve) => {
                finishFirst = resolve;
            }))
            .mockResolvedValue(undefined);

        render(<Harness callback={callback} />);
        await act(async () => {});
        expect(callback).toHaveBeenCalledTimes(1);

        await act(async () => {
            await vi.advanceTimersByTimeAsync(300);
        });
        expect(callback).toHaveBeenCalledTimes(1);

        await act(async () => {
            finishFirst();
            await Promise.resolve();
            await vi.advanceTimersByTimeAsync(100);
        });
        expect(callback).toHaveBeenCalledTimes(2);
    });
});
