import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { installPressFeedback, pressScaleFor } from "./pressFeedback";

describe("pressScaleFor", () => {
    it("dips a small control further than a wide card", () => {
        expect(pressScaleFor(32, 32)).toBe(0.94);
        expect(pressScaleFor(160, 40)).toBeCloseTo(0.969, 3);
        expect(pressScaleFor(900, 120)).toBe(0.99);
    });
});

describe("installPressFeedback", () => {
    let uninstall: () => void = () => undefined;
    const animate = vi.fn((_keyframes: Keyframe[], _options?: KeyframeAnimationOptions) => ({ cancel: vi.fn() }) as unknown as Animation);

    beforeEach(() => {
        Object.defineProperty(Element.prototype, "animate", { configurable: true, value: animate });
        animate.mockClear();
        uninstall = installPressFeedback(document);
    });

    afterEach(() => {
        uninstall();
        document.body.innerHTML = "";
        delete document.documentElement.dataset.motion;
    });

    // jsdom has no PointerEvent; a MouseEvent carrying isPrimary stands in.
    const pointer = (type: string, init: MouseEventInit = {}) =>
        Object.assign(new MouseEvent(type, { bubbles: true, button: 0, ...init }), { isPrimary: true });
    const press = (target: Element, init: MouseEventInit = {}) => target.dispatchEvent(pointer("pointerdown", init));

    it("presses a button and springs it back on release", () => {
        document.body.innerHTML = `<button type="button"><span>Save</span></button>`;
        press(document.querySelector("span")!);
        expect(animate).toHaveBeenCalledTimes(1);
        document.dispatchEvent(pointer("pointerup"));
        expect(animate).toHaveBeenCalledTimes(2);
        expect(animate.mock.calls[1][0]).toContainEqual({ scale: "1" });
    });

    it("leaves text fields, disabled controls and secondary buttons alone", () => {
        document.body.innerHTML = `<label><input id="field"></label><button id="off" disabled>Off</button><button id="b">B</button>`;
        press(document.getElementById("field")!);
        press(document.getElementById("off")!);
        press(document.getElementById("b")!, { button: 2 });
        expect(animate).not.toHaveBeenCalled();
    });

    it("lets go when the press becomes a drag", () => {
        document.body.innerHTML = `<button type="button">Row</button>`;
        press(document.querySelector("button")!, { clientX: 0, clientY: 0 });
        document.dispatchEvent(pointer("pointermove", { clientX: 20, clientY: 0 }));
        expect(animate).toHaveBeenCalledTimes(2);
    });

    it("does nothing while motion is switched off", () => {
        document.documentElement.dataset.motion = "off";
        document.body.innerHTML = `<button type="button">Save</button>`;
        press(document.querySelector("button")!);
        expect(animate).not.toHaveBeenCalled();
    });
});
