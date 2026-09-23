/**
 * Centralised motion values.
 *
 * Durations are in ms (Framer Motion expects seconds — divide by 1000 at the
 * call site or use the helper `s(ms)` below). Eases mirror the CSS custom
 * properties in `film-paper.css` (`--film-ease-*`) so CSS and JS motion stay
 * visually aligned.
 *
 * Springs use Apple's two designer parameters instead of mass/stiffness/
 * damping: `bounce` (0 = critically damped, damping ratio 1.0) and
 * `visualDuration` (Apple's "response" — how fast the value reaches the
 * target, not a fixed duration). Springs animate from the live value and
 * carry velocity through a re-target, so anything a user can touch or
 * interrupt should use one of these rather than a tween.
 */

export const motion = {
    ease: {
        // Primary easing for almost all UI transitions
        shutter: [0.32, 0.72, 0, 1] as const,
        // Reveals, page-in
        develop: [0.4, 0, 0.2, 1] as const,
        // Used sparingly — small overshoot
        iris: [0.34, 1.28, 0.64, 1] as const,
    },
    dur: {
        fast: 120,
        base: 220,
        slow: 320,
        cinema: 420,
    },
    spring: {
        // Damping 1.0 / response 0.3 — presses, toggles, chip state.
        snappy: { type: "spring", bounce: 0, visualDuration: 0.3 } as const,
        // Damping 1.0 / response 0.4 — reposition, panels, cards (PiP value).
        gentle: { type: "spring", bounce: 0, visualDuration: 0.4 } as const,
        // Damping 1.0 / response 0.5 — deliberate entrances.
        reveal: { type: "spring", bounce: 0, visualDuration: 0.5 } as const,
        // Damping ~0.8 / response 0.3 — drawers and sheets released by a
        // gesture. Only use when a drag or flick preceded the motion.
        momentum: { type: "spring", bounce: 0.2, visualDuration: 0.3 } as const,
    },
} as const;

/** Convert ms to seconds for Framer Motion `transition.duration`. */
export const s = (ms: number): number => ms / 1000;

export type MotionEase = keyof typeof motion.ease;
export type MotionDur = keyof typeof motion.dur;
export type MotionSpring = keyof typeof motion.spring;
