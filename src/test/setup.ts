import "@testing-library/jest-dom/vitest";

// jsdom implements no SVG filter primitives — `feDisplacementMap` comes back as
// a bare SVGElement with no `scale`, so a per-frame write into `.baseVal` throws
// from inside the animation loop and surfaces as an unhandled error. Nothing
// under test asserts on painted frames, so a frame that cannot paint is dropped
// rather than failing the run.
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

// jsdom ships no media-query engine; components that check reduced motion or
// colour scheme (the notch's voice beam and thinking orb) call this on mount.
if (typeof window !== "undefined" && typeof window.matchMedia !== "function") {
    Object.defineProperty(window, "matchMedia", {
        configurable: true,
        value: (query: string): MediaQueryList =>
            ({
                matches: false,
                media: query,
                onchange: null,
                addEventListener: () => undefined,
                removeEventListener: () => undefined,
                addListener: () => undefined,
                removeListener: () => undefined,
                dispatchEvent: () => false,
            }) as unknown as MediaQueryList,
    });
}

if (typeof localStorage.getItem !== "function") {
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
