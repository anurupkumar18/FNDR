/**
 * App-wide press feedback. Anything a person can press gives way under the
 * pointer and springs back on release, without each component opting in.
 *
 * It animates the individual `scale` property through the Web Animations
 * API, so it composes with a component's own `transform` and never touches
 * its CSS transitions. A press that turns into a drag or scroll lets go.
 */

/** Bounce 0.18, response 0.35 s, sampled for CSS `linear()`. */
export const SPRING_SOFT_EASING =
    "linear(0, 0.0697, 0.2211, 0.3947, 0.5578, 0.6951, 0.8024, 0.881, 0.9355, 0.971, 0.9924, 1.0042, 1.0096, 1.0111, 1.0105, 1.0088, 1.0069, 1.0051, 1.0035, 1.0023, 1.0014, 1.0008, 1.0004, 1.0001, 1)";
export const SPRING_SOFT_MS = 560;

const PRESS_IN_MS = 90;
const PRESS_IN_EASING = "cubic-bezier(0.32, 0.72, 0, 1)";
const MOVE_CANCEL_PX = 8;

const PRESSABLE = [
    "button",
    '[role="button"]',
    '[role="tab"]',
    '[role="menuitem"]',
    '[role="option"]',
    "a[href]",
    "summary",
    "[data-press]",
].join(", ");

/** Presses that begin on these belong to text entry or their own gesture. */
const NOT_PRESSABLE = 'input, textarea, select, [contenteditable="true"], [role="slider"], [data-no-press]';

/**
 * How far an element gives way. The shrink is roughly constant in pixels, so
 * a small icon button visibly dips and a wide card barely moves.
 */
export function pressScaleFor(width: number, height: number): number {
    const longest = Math.max(width, height, 1);
    return Math.min(0.99, Math.max(0.94, 1 - 5 / longest));
}

function findPressable(target: EventTarget | null): HTMLElement | null {
    if (!(target instanceof Element)) return null;
    if (target.closest(NOT_PRESSABLE)) return null;
    const element = target.closest<HTMLElement>(PRESSABLE);
    if (!element) return null;
    if (element.matches(":disabled") || element.getAttribute("aria-disabled") === "true") return null;
    return element;
}

function motionAllowed(): boolean {
    if (document.documentElement.dataset.motion === "off") return false;
    return !window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
}

interface Press {
    element: HTMLElement;
    animation: Animation;
    scale: number;
    startedAt: number;
    x: number;
    y: number;
}

/** Installs the listeners once per document; returns an uninstaller. */
export function installPressFeedback(doc: Document = document): () => void {
    const view = doc.defaultView;
    if (!view || typeof view.Element.prototype.animate !== "function") return () => undefined;

    let press: Press | null = null;

    const release = () => {
        const current = press;
        press = null;
        if (!current) return;
        const { element, animation, scale, startedAt } = current;
        const live = Number.parseFloat(view.getComputedStyle(element).scale);
        const from = Number.isFinite(live) ? live : scale;
        animation.cancel();
        const quickTap = performance.now() - startedAt < PRESS_IN_MS;
        const keyframes = quickTap
            ? [
                { scale: String(from), easing: PRESS_IN_EASING },
                { scale: String(scale), offset: 0.14, easing: SPRING_SOFT_EASING },
                { scale: "1" },
            ]
            : [{ scale: String(from), easing: SPRING_SOFT_EASING }, { scale: "1" }];
        element.animate(keyframes, { duration: SPRING_SOFT_MS });
    };

    const onPointerDown = (event: PointerEvent) => {
        if (event.button !== 0 || !event.isPrimary) return;
        if (!motionAllowed()) return;
        const element = findPressable(event.target);
        if (!element) return;
        if (view.getComputedStyle(element).display === "inline") return;
        release();
        const rect = element.getBoundingClientRect();
        const scale = pressScaleFor(rect.width, rect.height);
        const animation = element.animate([{ scale: "1" }, { scale: String(scale) }], {
            duration: PRESS_IN_MS,
            easing: PRESS_IN_EASING,
            fill: "forwards",
        });
        press = { element, animation, scale, startedAt: performance.now(), x: event.clientX, y: event.clientY };
    };

    const onPointerMove = (event: PointerEvent) => {
        if (!press) return;
        if (Math.hypot(event.clientX - press.x, event.clientY - press.y) > MOVE_CANCEL_PX) release();
    };

    doc.addEventListener("pointerdown", onPointerDown, true);
    doc.addEventListener("pointermove", onPointerMove, true);
    doc.addEventListener("pointerup", release, true);
    doc.addEventListener("pointercancel", release, true);
    doc.addEventListener("scroll", release, true);
    view.addEventListener("blur", release);

    return () => {
        release();
        doc.removeEventListener("pointerdown", onPointerDown, true);
        doc.removeEventListener("pointermove", onPointerMove, true);
        doc.removeEventListener("pointerup", release, true);
        doc.removeEventListener("pointercancel", release, true);
        doc.removeEventListener("scroll", release, true);
        view.removeEventListener("blur", release);
    };
}
