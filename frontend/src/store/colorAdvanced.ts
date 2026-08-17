// Resolve-lite primary grade: white balance, exposure, lift/gamma/gain wheels
// and per-band HSL. Applied before the look preset, curves and LUT. Foundation
// ships state, serialization and validated restore; the color agent adds the
// wheels UI on top.

import { reactive } from 'vue'
import type { ColorAdvanced, HslAdjustment, HslBand, Rgb } from '../types'
import { clampNumber, enumOr, isRecord, sanitizeList } from './validation'

export const HSL_BANDS: readonly HslBand[] = [
  'red',
  'orange',
  'yellow',
  'green',
  'cyan',
  'blue',
  'magenta',
]

/** Ranges from the contract, section 2. */
const TEMPERATURE_LIMIT = 1
const EXPOSURE_LIMIT = 2
const TONE_LIMIT = 1
const LIFT_LIMIT = 0.5
const GAMMA_MIN = 0.1
const GAMMA_MAX = 4
const GAIN_MAX = 4
/** Not pinned by the contract; chosen to match the FFmpeg `hue`/`colorchannelmixer` domains. */
const HUE_LIMIT = 180
const HSL_SCALE_MAX = 4

export interface ColorAdvancedState {
  temperature: number
  tint: number
  exposure: number
  highlights: number
  shadows: number
  lift: Rgb
  gamma: Rgb
  gain: Rgb
  hsl: HslAdjustment[]
}

function defaults(): ColorAdvancedState {
  return {
    temperature: 0,
    tint: 0,
    exposure: 0,
    highlights: 0,
    shadows: 0,
    lift: { r: 0, g: 0, b: 0 },
    gamma: { r: 1, g: 1, b: 1 },
    gain: { r: 1, g: 1, b: 1 },
    hsl: [],
  }
}

export const colorAdvancedState = reactive<ColorAdvancedState>(defaults())

export function resetColorAdvanced(): void {
  const base = defaults()
  colorAdvancedState.temperature = base.temperature
  colorAdvancedState.tint = base.tint
  colorAdvancedState.exposure = base.exposure
  colorAdvancedState.highlights = base.highlights
  colorAdvancedState.shadows = base.shadows
  colorAdvancedState.lift = base.lift
  colorAdvancedState.gamma = base.gamma
  colorAdvancedState.gain = base.gain
  colorAdvancedState.hsl = []
}

function sanitizeRgb(value: unknown, min: number, max: number, neutral: number): Rgb {
  const source = isRecord(value) ? value : {}
  return {
    r: clampNumber(source.r, min, max, neutral),
    g: clampNumber(source.g, min, max, neutral),
    b: clampNumber(source.b, min, max, neutral),
  }
}

function isNeutralRgb(rgb: Rgb, neutral: number): boolean {
  return rgb.r === neutral && rgb.g === neutral && rgb.b === neutral
}

function sanitizeHslAdjustment(value: unknown): HslAdjustment | null {
  if (!isRecord(value)) return null
  if (typeof value.band !== 'string') return null
  if (!HSL_BANDS.includes(value.band as HslBand)) return null
  return {
    band: enumOr(value.band, HSL_BANDS, 'red'),
    hue: clampNumber(value.hue, -HUE_LIMIT, HUE_LIMIT, 0),
    saturation: clampNumber(value.saturation, 0, HSL_SCALE_MAX, 1),
    luminance: clampNumber(value.luminance, 0, HSL_SCALE_MAX, 1),
  }
}

function isNeutralHsl(adjustment: HslAdjustment): boolean {
  return adjustment.hue === 0 && adjustment.saturation === 1 && adjustment.luminance === 1
}

export function sanitizeColorAdvanced(value: unknown): ColorAdvancedState {
  const source = isRecord(value) ? value : {}
  // One adjustment per band; a later duplicate replaces the earlier one.
  const byBand = new Map<HslBand, HslAdjustment>()
  for (const adjustment of sanitizeList(source.hsl, HSL_BANDS.length * 4, sanitizeHslAdjustment)) {
    byBand.set(adjustment.band, adjustment)
  }
  return {
    temperature: clampNumber(source.temperature, -TEMPERATURE_LIMIT, TEMPERATURE_LIMIT, 0),
    tint: clampNumber(source.tint, -TEMPERATURE_LIMIT, TEMPERATURE_LIMIT, 0),
    exposure: clampNumber(source.exposure, -EXPOSURE_LIMIT, EXPOSURE_LIMIT, 0),
    highlights: clampNumber(source.highlights, -TONE_LIMIT, TONE_LIMIT, 0),
    shadows: clampNumber(source.shadows, -TONE_LIMIT, TONE_LIMIT, 0),
    lift: sanitizeRgb(source.lift, -LIFT_LIMIT, LIFT_LIMIT, 0),
    gamma: sanitizeRgb(source.gamma, GAMMA_MIN, GAMMA_MAX, 1),
    gain: sanitizeRgb(source.gain, 0, GAIN_MAX, 1),
    hsl: [...byBand.values()],
  }
}

export function colorAdvancedPayload(): Record<string, unknown> {
  const state = sanitizeColorAdvanced(colorAdvancedState)
  const grade: ColorAdvanced = {}
  if (state.temperature !== 0) grade.temperature = state.temperature
  if (state.tint !== 0) grade.tint = state.tint
  if (state.exposure !== 0) grade.exposure = state.exposure
  if (state.highlights !== 0) grade.highlights = state.highlights
  if (state.shadows !== 0) grade.shadows = state.shadows
  if (!isNeutralRgb(state.lift, 0)) grade.lift = state.lift
  if (!isNeutralRgb(state.gamma, 1)) grade.gamma = state.gamma
  if (!isNeutralRgb(state.gain, 1)) grade.gain = state.gain
  const hsl = state.hsl.filter((adjustment) => !isNeutralHsl(adjustment))
  if (hsl.length) grade.hsl = hsl
  return Object.keys(grade).length ? { colorAdvanced: grade } : {}
}

export function applyColorAdvancedSnapshot(raw: unknown): void {
  const sanitized = sanitizeColorAdvanced(raw)
  colorAdvancedState.temperature = sanitized.temperature
  colorAdvancedState.tint = sanitized.tint
  colorAdvancedState.exposure = sanitized.exposure
  colorAdvancedState.highlights = sanitized.highlights
  colorAdvancedState.shadows = sanitized.shadows
  colorAdvancedState.lift = sanitized.lift
  colorAdvancedState.gamma = sanitized.gamma
  colorAdvancedState.gain = sanitized.gain
  colorAdvancedState.hsl = sanitized.hsl
}
