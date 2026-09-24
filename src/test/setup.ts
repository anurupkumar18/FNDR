import "@testing-library/jest-dom/vitest";

if (typeof localStorage === "undefined" || typeof localStorage.getItem !== "function") {
    const storage = new Map<string, string>();
    Object.defineProperty(globalThis, "localStorage", {
        configurable: true,
        value: {
            clear: () => storage.clear(),
            getItem: (key: string) => storage.get(key) ?? null,
            key: (index: number) => Array.from(storage.keys())[index] ?? null,
            removeItem: (key: string) => storage.delete(key),
            setItem: (key: string, value: string) => storage.set(key, value),
            get length() {
                return storage.size;
            },
        },
    });
}

// jsdom has no canvas; canvas-drawn decoration (ThinkingOrb) renders nothing.
Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
    configurable: true,
    value: () => null,
});

// jsdom has no matchMedia; components that read media queries at render
// (BorderBeam, reduced-motion checks) see "no match".
if (typeof window !== "undefined" && typeof window.matchMedia !== "function") {
    Object.defineProperty(window, "matchMedia", {
        configurable: true,
        value: (query: string) => ({
            matches: false,
            media: query,
            onchange: null,
            addListener: () => {},
            removeListener: () => {},
            addEventListener: () => {},
            removeEventListener: () => {},
            dispatchEvent: () => false,
        }),
    });
}

// jsdom has no ResizeObserver; layout-tracking effects (liquid-gooey) observe nothing.
if (typeof globalThis.ResizeObserver === "undefined") {
    globalThis.ResizeObserver = class {
        observe() {}
        unobserve() {}
        disconnect() {}
    } as unknown as typeof ResizeObserver;
}

// jsdom implements no SVG filter primitives, so a per-frame write into
// feDisplacementMap's `scale` throws inside animation loops (the notch's voice
// beam). Nothing asserts on painted frames; unpaintable frames are dropped.
if (typeof window !== "undefined") {
    const scheduleFrame = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (callback: FrameRequestCallback): number =>
        scheduleFrame((time) => {
            try {
                callback(time);
            } catch {
                /* unpaintable frame under jsdom */
            }
        });
}

// jsdom has no layout, so no scrollIntoView; scrolling is a no-op in tests.
if (typeof Element !== "undefined" && typeof Element.prototype.scrollIntoView !== "function") {
    Element.prototype.scrollIntoView = () => {};
}
