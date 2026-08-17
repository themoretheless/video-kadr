// Geometry of a Resolve-style colour wheel: the disc maps a point to an RGB
// offset (lift) or multiplier (gamma, gain), the ring moves all three channels
// together. Kept out of the component so the mapping can be unit tested.

import type { Rgb } from '../../types'

export type WheelMode = 'lift' | 'gamma' | 'gain'

export interface WheelConfig {
  /** Value that means "no change" for this wheel. */
  neutral: number
  /** Hard limits from the feature contract, section 2. */
  min: number
  max: number
  /** How far the rim of the disc, and the end of the ring, travel. */
  range: number
  label: string
}

export const WHEEL_CONFIG: Record<WheelMode, WheelConfig> = {
  lift: { neutral: 0, min: -0.5, max: 0.5, range: 0.5, label: 'Тени (lift)' },
  gamma: { neutral: 1, min: 0.1, max: 4, range: 0.5, label: 'Полутона (gamma)' },
  gain: { neutral: 1, min: 0, max: 4, range: 0.5, label: 'Света (gain)' },
}

export const WHEEL_MODES: readonly WheelMode[] = ['lift', 'gamma', 'gain']

/**
 * Where each channel sits on the disc, in radians measured counter-clockwise
 * from the +x axis: red up, green lower left, blue lower right.
 */
const AXES = [Math.PI / 2, (7 * Math.PI) / 6, (11 * Math.PI) / 6]

/** Wheel output resolution, decimals. Keeps float noise out of the payload and the render-cache key. */
const DECIMALS = 4

function clamp(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min
  return value < min ? min : value > max ? max : value
}

function quantize(value: number): number {
  return Number(value.toFixed(DECIMALS))
}

export function neutralRgb(mode: WheelMode): Rgb {
  const { neutral } = WHEEL_CONFIG[mode]
  return { r: neutral, g: neutral, b: neutral }
}

export function isNeutralWheel(value: Rgb, mode: WheelMode): boolean {
  const { neutral } = WHEEL_CONFIG[mode]
  return value.r === neutral && value.g === neutral && value.b === neutral
}

/**
 * Disc point to channel values. `x` and `y` are in -1..1 with `y` pointing up;
 * anything outside the unit circle is pulled back onto the rim. A pull towards
 * one primary also pushes the other two down by half as much, which is what
 * makes the wheel a hue offset rather than three independent gains.
 */
export function rgbFromPoint(x: number, y: number, mode: WheelMode): Rgb {
  const { neutral, min, max, range } = WHEEL_CONFIG[mode]
  const safeX = Number.isFinite(x) ? x : 0
  const safeY = Number.isFinite(y) ? y : 0
  const radius = Math.min(1, Math.hypot(safeX, safeY))
  const angle = Math.atan2(safeY, safeX)
  const channel = (axis: number) =>
    clamp(quantize(neutral + radius * range * Math.cos(angle - axis)), min, max)
  return { r: channel(AXES[0]), g: channel(AXES[1]), b: channel(AXES[2]) }
}

/** The inverse of `rgbFromPoint`, for drawing the puck. */
export function pointFromRgb(value: Rgb, mode: WheelMode): { x: number; y: number } {
  const { neutral, range } = WHEEL_CONFIG[mode]
  const offsets = [
    (clampChannel(value.r, mode) - neutral) / range,
    (clampChannel(value.g, mode) - neutral) / range,
    (clampChannel(value.b, mode) - neutral) / range,
  ]
  // The forward map is a projection onto three unit vectors 120 degrees apart,
  // whose pseudo-inverse is the same projection scaled by 2/3.
  let x = 0
  let y = 0
  for (let index = 0; index < 3; index++) {
    x += (offsets[index] * Math.cos(AXES[index]) * 2) / 3
    y += (offsets[index] * Math.sin(AXES[index]) * 2) / 3
  }
  const radius = Math.hypot(x, y)
  if (radius > 1) {
    x /= radius
    y /= radius
  }
  return { x, y }
}

/** A single channel value pulled into range; anything unusable falls back to neutral. */
export function clampChannel(value: number, mode: WheelMode): number {
  const { neutral, min, max } = WHEEL_CONFIG[mode]
  return Number.isFinite(value) ? clamp(value, min, max) : neutral
}

/** Move the puck by a delta in disc coordinates, e.g. from an arrow key. */
export function nudgeWheel(value: Rgb, mode: WheelMode, dx: number, dy: number): Rgb {
  const point = pointFromRgb(value, mode)
  return rgbFromPoint(point.x + dx, point.y + dy, mode)
}

/** Master (luminance) offset of a wheel: the average distance from neutral. */
export function masterOf(value: Rgb, mode: WheelMode): number {
  const { neutral, range } = WHEEL_CONFIG[mode]
  const average =
    (clampChannel(value.r, mode) + clampChannel(value.g, mode) + clampChannel(value.b, mode)) / 3
  return clamp((average - neutral) / range, -1, 1)
}

/** Set the master offset while keeping the hue the disc currently holds. */
export function withMaster(value: Rgb, mode: WheelMode, master: number): Rgb {
  const { min, max, range } = WHEEL_CONFIG[mode]
  const target = clamp(master, -1, 1) * range
  const current = masterOf(value, mode) * range
  const shift = target - current
  return {
    r: clamp(quantize(clampChannel(value.r, mode) + shift), min, max),
    g: clamp(quantize(clampChannel(value.g, mode) + shift), min, max),
    b: clamp(quantize(clampChannel(value.b, mode) + shift), min, max),
  }
}

/** Display text for one channel field: lift needs more digits than gain. */
export function formatChannel(value: number, mode: WheelMode): string {
  return clampChannel(value, mode).toFixed(mode === 'lift' ? 3 : 2)
}
