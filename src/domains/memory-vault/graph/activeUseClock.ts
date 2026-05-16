const HOUR_MS = 60 * 60 * 1000;

let activeMillis = 0;
let lastTick = typeof performance !== "undefined" ? performance.now() : Date.now();
let started = false;

function now(): number {
    return typeof performance !== "undefined" ? performance.now() : Date.now();
}

function isForeground(): boolean {
    if (typeof document === "undefined") return true;
    return document.visibilityState === "visible";
}

function tick(): void {
    const t = now();
    if (isForeground()) {
        activeMillis += t - lastTick;
    }
    lastTick = t;
}

function start(): void {
    if (started) return;
    started = true;
    if (typeof document !== "undefined") {
        document.addEventListener("visibilitychange", tick);
    }
    if (typeof window !== "undefined" && typeof window.setInterval === "function") {
        // Coarse cadence — the cache only cares about hour boundaries.
        window.setInterval(tick, 30_000);
    }
}

export function getActiveMillis(): number {
    if (!started) start();
    tick();
    return activeMillis;
}

export const ACTIVE_USE_HOUR_MS = HOUR_MS;

/** Test-only: force the active-use accumulator (does not auto-start the ticker). */
export function __setActiveMillis(ms: number): void {
    activeMillis = ms;
    lastTick = now();
}

/** Test-only: reset state. */
export function __resetActiveClock(): void {
    activeMillis = 0;
    lastTick = now();
}
