/**
 * Gesture math from Apple's "Designing Fluid Interfaces" (WWDC 2018).
 */

/**
 * Distance a flick travels before coming to rest, using the same
 * exponential-decay model as scroll deceleration. `decelerationRate` 0.998
 * matches normal scroll feel; 0.99 lands sooner.
 */
export function project(velocityPxPerSec: number, decelerationRate = 0.998): number {
    return ((velocityPxPerSec / 1000) * decelerationRate) / (1 - decelerationRate);
}

/**
 * Progressive resistance past a boundary: the further the overshoot, the
 * less the element follows, approaching but never reaching `dimension`.
 */
export function rubberband(overshoot: number, dimension: number, constant = 0.55): number {
    const damped = (Math.abs(overshoot) * dimension * constant) / (dimension + constant * Math.abs(overshoot));
    return Math.sign(overshoot) * damped;
}

/** The snap point nearest to where the gesture is heading, not where it let go. */
export function nearestSnapPoint(projected: number, snapPoints: readonly number[]): number {
    return snapPoints.reduce((best, point) =>
        Math.abs(point - projected) < Math.abs(best - projected) ? point : best,
    );
}
