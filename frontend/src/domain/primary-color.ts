import { sanitizePrimaryCorrection } from './edit'

export interface PrimaryCorrections {
  temperature: number
  tint: number
  highlights: number
  shadows: number
}

export type Rgb = readonly [number, number, number]

/** Canonical order shared with fixtures/color-grade/primary-corrections-v1.json. */
export const PRIMARY_COLOR_PIPELINE_ORDER = [
  'decode-transfer',
  'temperature-tint',
  'shadows-highlights',
  'encode-transfer',
  'brightness-contrast-saturation',
  'preset',
  'lut',
  'curves',
] as const

function controls(value: PrimaryCorrections): PrimaryCorrections {
  return {
    temperature: sanitizePrimaryCorrection(value.temperature),
    tint: sanitizePrimaryCorrection(value.tint),
    highlights: sanitizePrimaryCorrection(value.highlights),
    shadows: sanitizePrimaryCorrection(value.shadows),
  }
}

export function primaryCorrectionsActive(value: PrimaryCorrections): boolean {
  const safe = controls(value)
  return safe.temperature !== 0 || safe.tint !== 0 || safe.highlights !== 0 || safe.shadows !== 0
}

function smoothstep(low: number, high: number, value: number): number {
  const amount = Math.max(0, Math.min(1, (value - low) / (high - low)))
  return amount * amount * (3 - 2 * amount)
}

/** Apply the fixture-defined grade to linear-light sRGB values. */
export function applyPrimaryCorrectionsLinearSrgb(rgb: Rgb, value: PrimaryCorrections): [number, number, number] {
  const safe = controls(value)
  const corrected: [number, number, number] = [
    rgb[0] * 2 ** (0.25 * safe.temperature - 0.10 * safe.tint),
    rgb[1] * 2 ** (0.20 * safe.tint),
    rgb[2] * 2 ** (-0.25 * safe.temperature - 0.10 * safe.tint),
  ]
  const luma = 0.2126 * corrected[0] + 0.7152 * corrected[1] + 0.0722 * corrected[2]
  const shadowMask = 1 - smoothstep(0, 0.5, luma)
  const highlightMask = smoothstep(0.5, 1, luma)
  const tonalGain = 2 ** (0.75 * (safe.shadows * shadowMask + safe.highlights * highlightMask))
  return corrected.map(channel => Math.max(0, Math.min(1, channel * tonalGain))) as [number, number, number]
}

export function decodeSrgb(value: number): number {
  const channel = Math.max(0, Math.min(1, value))
  return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4
}

export function encodeSrgb(value: number): number {
  const channel = Math.max(0, Math.min(1, value))
  return channel <= 0.0031308 ? 12.92 * channel : 1.055 * channel ** (1 / 2.4) - 0.055
}

/** Exact CPU preview path for gamma-encoded sRGB input/output. */
export function applyPrimaryCorrectionsSrgb(rgb: Rgb, value: PrimaryCorrections): [number, number, number] {
  return applyPrimaryCorrectionsLinearSrgb(rgb.map(decodeSrgb) as [number, number, number], value).map(encodeSrgb) as [number, number, number]
}

const BYTE_TO_LINEAR = Array.from({ length: 256 }, (_, value) => decodeSrgb(value / 255))

/** Mutates canvas RGBA bytes; alpha is intentionally preserved. */
export function applyPrimaryCorrectionsToImageData(image: ImageData, value: PrimaryCorrections): void {
  const safe = controls(value)
  if (!primaryCorrectionsActive(safe)) return
  const gains = [
    2 ** (0.25 * safe.temperature - 0.10 * safe.tint),
    2 ** (0.20 * safe.tint),
    2 ** (-0.25 * safe.temperature - 0.10 * safe.tint),
  ] as const
  const bytes = image.data
  for (let index = 0; index < bytes.length; index += 4) {
    const red = BYTE_TO_LINEAR[bytes[index]!]! * gains[0]
    const green = BYTE_TO_LINEAR[bytes[index + 1]!]! * gains[1]
    const blue = BYTE_TO_LINEAR[bytes[index + 2]!]! * gains[2]
    const luma = 0.2126 * red + 0.7152 * green + 0.0722 * blue
    const shadowMask = 1 - smoothstep(0, 0.5, luma)
    const highlightMask = smoothstep(0.5, 1, luma)
    const tonalGain = 2 ** (0.75 * (safe.shadows * shadowMask + safe.highlights * highlightMask))
    bytes[index] = Math.round(encodeSrgb(red * tonalGain) * 255)
    bytes[index + 1] = Math.round(encodeSrgb(green * tonalGain) * 255)
    bytes[index + 2] = Math.round(encodeSrgb(blue * tonalGain) * 255)
  }
}
