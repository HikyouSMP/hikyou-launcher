// M3 fast-spatial stiffness, critically damped to keep clipped text from bouncing.
// Sample the normalized unit-step response once; CSS runs the animation itself.
const durationMs = 250;
const omega = Math.sqrt(1400);
const response = (seconds: number) => 1 - (1 + omega * seconds) * Math.exp(-omega * seconds);
const end = response(durationMs / 1000);
const stops = Array.from({ length: 65 }, (_, index) =>
    (response(index / 64 * durationMs / 1000) / end).toFixed(5));

export const CHIP_REVEAL_MOTION = {
    durationMs,
    easing: `linear(${stops.join(", ")})`,
} as const;
