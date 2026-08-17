// Client-side video scopes: histogram, waveform (luma and RGB parade) and
// vectorscope, plus the transfer tables that let the panel show those scopes
// through the primary grade.
//
// Everything here is pure math over a reduced-resolution RGBA sample of one
// frame. Grabbing the frame, throttling the loop and painting the canvases is
// the components' job; this file never touches the DOM.

import type { Rgb } from '../types'

/** One decoded frame, downscaled. `data` is RGBA, four bytes per pixel. */
export interface FrameSample {
  width: number
  height: number
  data: Uint8ClampedArray
}

/** Rec.709 luma weights, matching how the export path treats HD sources. */
const LUMA_R = 0.2126
const LUMA_G = 0.7152
const LUMA_B = 0.0722

/** BT.709 chroma projection used by the vectorscope. */
const U_R = -0.1146
const U_G = -0.3854
const U_B = 0.5
const V_R = 0.5
const V_G = -0.4542
const V_B = -0.0458

export function lumaOf(r: number, g: number, b: number): number {
  return LUMA_R * r + LUMA_G * g + LUMA_B * b
}

/**
 * Sample size for a source frame: fit inside `maxWidth` keeping the aspect
 * ratio, never smaller than one pixel. Scopes are statistical, so a 320px wide
 * sample is plenty and keeps the throttled loop off the main thread's back.
 */
export function sampleDimensions(
  sourceWidth: number,
  sourceHeight: number,
  maxWidth: number,
): { width: number; height: number } {
  const safeWidth = Number.isFinite(sourceWidth) && sourceWidth > 0 ? sourceWidth : maxWidth
  const safeHeight = Number.isFinite(sourceHeight) && sourceHeight > 0 ? sourceHeight : maxWidth
  const width = Math.max(1, Math.min(Math.round(maxWidth), Math.round(safeWidth)))
  const height = Math.max(1, Math.round((width * safeHeight) / safeWidth))
  return { width, height }
}

/** Number of whole RGBA pixels actually addressable in a sample. */
function pixelCount(frame: FrameSample): number {
  return Math.max(0, Math.min(frame.width * frame.height, frame.data.length >> 2))
}

export interface Histogram {
  bins: number
  red: Uint32Array
  green: Uint32Array
  blue: Uint32Array
  luma: Uint32Array
  /** Largest count in any channel, so a renderer can normalise. */
  peak: number
}

export function computeHistogram(frame: FrameSample, bins = 256): Histogram {
  const count = Math.max(2, Math.min(256, Math.round(bins)))
  const red = new Uint32Array(count)
  const green = new Uint32Array(count)
  const blue = new Uint32Array(count)
  const luma = new Uint32Array(count)
  const scale = (count - 1) / 255
  const pixels = pixelCount(frame)
  for (let index = 0; index < pixels; index++) {
    const at = index * 4
    const r = frame.data[at]
    const g = frame.data[at + 1]
    const b = frame.data[at + 2]
    red[Math.round(r * scale)]++
    green[Math.round(g * scale)]++
    blue[Math.round(b * scale)]++
    luma[Math.round(lumaOf(r, g, b) * scale)]++
  }
  let peak = 0
  for (let bin = 0; bin < count; bin++) {
    peak = Math.max(peak, red[bin], green[bin], blue[bin], luma[bin])
  }
  return { bins: count, red, green, blue, luma, peak }
}

export interface Waveform {
  columns: number
  levels: number
  /** Column-major counts: index = column * levels + level, level 0 is black. */
  luma: Uint32Array
  red: Uint32Array
  green: Uint32Array
  blue: Uint32Array
  peak: number
}

export function computeWaveform(frame: FrameSample, columns = 256, levels = 128): Waveform {
  const cols = Math.max(1, Math.round(columns))
  const rows = Math.max(2, Math.round(levels))
  const size = cols * rows
  const luma = new Uint32Array(size)
  const red = new Uint32Array(size)
  const green = new Uint32Array(size)
  const blue = new Uint32Array(size)
  const levelScale = (rows - 1) / 255
  const pixels = pixelCount(frame)
  const width = Math.max(1, frame.width)
  let peak = 0
  for (let index = 0; index < pixels; index++) {
    const at = index * 4
    const column = Math.min(cols - 1, Math.floor(((index % width) * cols) / width))
    const base = column * rows
    const r = frame.data[at]
    const g = frame.data[at + 1]
    const b = frame.data[at + 2]
    const y = base + Math.round(lumaOf(r, g, b) * levelScale)
    luma[y]++
    red[base + Math.round(r * levelScale)]++
    green[base + Math.round(g * levelScale)]++
    blue[base + Math.round(b * levelScale)]++
    if (luma[y] > peak) peak = luma[y]
  }
  return { columns: cols, levels: rows, luma, red, green, blue, peak }
}

export interface Vectorscope {
  size: number
  /** Row-major counts: index = row * size + column, row 0 is the top (+V). */
  bins: Uint32Array
  peak: number
}

/**
 * Chroma distribution on the U/V plane. The centre of the square is neutral,
 * the distance from it is saturation and the angle is hue, exactly like the
 * hardware scope a colourist expects.
 */
export function computeVectorscope(frame: FrameSample, size = 128): Vectorscope {
  const side = Math.max(2, Math.round(size))
  const bins = new Uint32Array(side * side)
  const pixels = pixelCount(frame)
  let peak = 0
  for (let index = 0; index < pixels; index++) {
    const at = index * 4
    const r = frame.data[at] / 255
    const g = frame.data[at + 1] / 255
    const b = frame.data[at + 2] / 255
    const u = U_R * r + U_G * g + U_B * b
    const v = V_R * r + V_G * g + V_B * b
    const column = Math.max(0, Math.min(side - 1, Math.round((u + 0.5) * (side - 1))))
    const row = Math.max(0, Math.min(side - 1, Math.round((0.5 - v) * (side - 1))))
    const bin = row * side + column
    bins[bin]++
    if (bins[bin] > peak) peak = bins[bin]
  }
  return { size: side, bins, peak }
}

// --- grade preview -------------------------------------------------------
//
// The export grade is emitted by `backend/src/render/graph/color.rs`. Every
// stage it emits before the HSL secondaries is pointwise per channel, so the
// whole thing collapses into three 1D transfer functions that both the scopes
// and the before/after wipe can share.
//
// Deliberate differences from the export, all of them approximations that are
// labelled as such in the UI:
//   - temperature and tint become channel gains and a midtone push, because
//     `colortemperature` and `colorbalance` have no closed form here;
//   - the highlight/shadow curve is interpolated linearly where the export uses
//     pchip through the same five control points;
//   - the HSL secondaries are not previewed at all.

/** The subset of `colorAdvanced` this preview can reproduce. */
export interface ScopeGrade {
  temperature: number
  tint: number
  exposure: number
  highlights: number
  shadows: number
  lift: Rgb
  gamma: Rgb
  gain: Rgb
}

/** How far one unit of temperature pushes the red/blue gains apart. */
const TEMPERATURE_GAIN = 0.25
/** How far one unit of tint pushes the midtones along green/magenta. */
const TINT_MIDTONE_SCALE = 0.25
/** Same control-point travel the export uses for highlight/shadow recovery. */
const TONE_RECOVERY_SCALE = 0.2

const TRANSFER_STEPS = 256

function finite(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback
}

function clamp01(value: number): number {
  return value < 0 ? 0 : value > 1 ? 1 : value
}

/** Piecewise-linear lookup through the highlight/shadow control points. */
function toneCurve(x: number, low: number, high: number): number {
  if (x <= 0.25) return (x / 0.25) * low
  if (x <= 0.5) return low + ((x - 0.25) / 0.25) * (0.5 - low)
  if (x <= 0.75) return 0.5 + ((x - 0.5) / 0.25) * (high - 0.5)
  return high + ((x - 0.75) / 0.25) * (1 - high)
}

/**
 * The three per-channel transfer functions, sampled at `steps` points over
 * 0..1. Stage order matches the export: white balance, exposure, highlight and
 * shadow recovery, then the lift/gamma/gain wheels.
 */
export function gradeTransfer(grade: ScopeGrade, steps = TRANSFER_STEPS): Float64Array[] {
  const count = Math.max(2, Math.round(steps))
  const temperature = Math.max(-1, Math.min(1, finite(grade.temperature, 0)))
  const tint = Math.max(-1, Math.min(1, finite(grade.tint, 0)))
  const exposure = 2 ** Math.max(-2, Math.min(2, finite(grade.exposure, 0)))
  const highlights = Math.max(-1, Math.min(1, finite(grade.highlights, 0)))
  const shadows = Math.max(-1, Math.min(1, finite(grade.shadows, 0)))
  const low = Math.max(0.02, Math.min(0.48, 0.25 + shadows * TONE_RECOVERY_SCALE))
  const high = Math.max(0.52, Math.min(0.98, 0.75 + highlights * TONE_RECOVERY_SCALE))

  const balance = [
    1 + temperature * TEMPERATURE_GAIN,
    1,
    1 - temperature * TEMPERATURE_GAIN,
  ]
  const tintPush = [
    tint * TINT_MIDTONE_SCALE,
    -tint * TINT_MIDTONE_SCALE,
    tint * TINT_MIDTONE_SCALE,
  ]
  const lift = [
    Math.max(-0.5, Math.min(0.5, finite(grade.lift.r, 0))),
    Math.max(-0.5, Math.min(0.5, finite(grade.lift.g, 0))),
    Math.max(-0.5, Math.min(0.5, finite(grade.lift.b, 0))),
  ]
  const gamma = [
    Math.max(0.1, Math.min(4, finite(grade.gamma.r, 1))),
    Math.max(0.1, Math.min(4, finite(grade.gamma.g, 1))),
    Math.max(0.1, Math.min(4, finite(grade.gamma.b, 1))),
  ]
  const gain = [
    Math.max(0, Math.min(4, finite(grade.gain.r, 1))),
    Math.max(0, Math.min(4, finite(grade.gain.g, 1))),
    Math.max(0, Math.min(4, finite(grade.gain.b, 1))),
  ]

  return [0, 1, 2].map((channel) => {
    const table = new Float64Array(count)
    for (let step = 0; step < count; step++) {
      const x = step / (count - 1)
      let value = clamp01(x * balance[channel])
      // The midtone weight peaks at 0.5 and vanishes at both ends, which is how
      // `colorbalance` distributes its midtone offset.
      value = clamp01(value + tintPush[channel] * (1 - (2 * value - 1) ** 2))
      value = clamp01(value * exposure)
      value = clamp01(toneCurve(value, low, high))
      value = clamp01(value * gain[channel] + lift[channel]) ** (1 / gamma[channel])
      table[step] = clamp01(value)
    }
    return table
  })
}

/** True when the grade would leave every pixel exactly where it was. */
export function isNeutralGrade(grade: ScopeGrade): boolean {
  return (
    finite(grade.temperature, 0) === 0 &&
    finite(grade.tint, 0) === 0 &&
    finite(grade.exposure, 0) === 0 &&
    finite(grade.highlights, 0) === 0 &&
    finite(grade.shadows, 0) === 0 &&
    grade.lift.r === 0 &&
    grade.lift.g === 0 &&
    grade.lift.b === 0 &&
    grade.gamma.r === 1 &&
    grade.gamma.g === 1 &&
    grade.gamma.b === 1 &&
    grade.gain.r === 1 &&
    grade.gain.g === 1 &&
    grade.gain.b === 1
  )
}

/** A graded copy of the sample. The input buffer is never mutated. */
export function gradeFrame(frame: FrameSample, grade: ScopeGrade): FrameSample {
  if (isNeutralGrade(grade)) return frame
  const [red, green, blue] = gradeTransfer(grade, 256)
  const data = new Uint8ClampedArray(frame.data.length)
  const pixels = pixelCount(frame)
  for (let index = 0; index < pixels; index++) {
    const at = index * 4
    data[at] = red[frame.data[at]] * 255
    data[at + 1] = green[frame.data[at + 1]] * 255
    data[at + 2] = blue[frame.data[at + 2]] * 255
    data[at + 3] = frame.data[at + 3]
  }
  return { width: frame.width, height: frame.height, data }
}

/**
 * The same transfer functions as `tableValues` for an SVG `feComponentTransfer`,
 * which interpolates linearly between the entries. 33 stops track the curve
 * closely enough for a preview while keeping the markup small.
 */
export function gradeTransferTables(grade: ScopeGrade, steps = 33): string[] {
  return gradeTransfer(grade, steps).map((table) =>
    Array.from(table, (value) => value.toFixed(4)).join(' '),
  )
}
