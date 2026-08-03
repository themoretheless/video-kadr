export type Rgb = readonly [number, number, number]
export type Hsl = readonly [number, number, number]

export interface HslSelection {
  centerDegrees: number
  halfWidthDegrees: number
  featherDegrees: number
}

export interface HslAdjustment {
  hueDegrees: number
  saturation: number
  lightness: number
}

export interface HslSelective {
  selection: HslSelection
  adjustment: HslAdjustment
}

export const HSL_SELECTIVE_DEFAULTS: HslSelective = Object.freeze({
  selection: Object.freeze({ centerDegrees: 0, halfWidthDegrees: 30, featherDegrees: 15 }),
  adjustment: Object.freeze({ hueDegrees: 0, saturation: 0, lightness: 0 }),
})

export const HSL_SELECTIVE_PIPELINE_ORDER = [
  'decode-transfer', 'primary-corrections', 'lift', 'gamma', 'gain', 'encode-transfer',
  'hsl-selective-v1', 'brightness-contrast-saturation', 'preset', 'lut', 'curves',
] as const

function finite(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

function clamp(value: number, low = 0, high = 1): number {
  return Math.max(low, Math.min(high, value))
}

export function wrapDegrees(value: number): number {
  const wrapped = value % 360
  return wrapped < 0 ? wrapped + 360 : wrapped
}

export function sanitizeHslSelective(value: {
  selection?: Partial<HslSelection> | null
  adjustment?: Partial<HslAdjustment> | null
} | null | undefined): HslSelective {
  const selection = value?.selection
  const adjustment = value?.adjustment
  const halfWidthDegrees = clamp(finite(selection?.halfWidthDegrees, 30), 0, 180)
  return {
    selection: {
      centerDegrees: wrapDegrees(finite(selection?.centerDegrees, 0)),
      halfWidthDegrees,
      featherDegrees: clamp(finite(selection?.featherDegrees, 15), 0, Math.min(90, 180 - halfWidthDegrees)),
    },
    adjustment: {
      hueDegrees: clamp(finite(adjustment?.hueDegrees, 0), -180, 180),
      saturation: clamp(finite(adjustment?.saturation, 0), -1, 1),
      lightness: clamp(finite(adjustment?.lightness, 0), -1, 1),
    },
  }
}

export function hslSelectiveActive(value: HslSelective): boolean {
  const { adjustment } = sanitizeHslSelective(value)
  return adjustment.hueDegrees !== 0 || adjustment.saturation !== 0 || adjustment.lightness !== 0
}

export function encodedSrgbToHsl(rgb: Rgb): Hsl {
  const [red, green, blue] = rgb.map(value => clamp(value)) as [number, number, number]
  const maximum = Math.max(red, green, blue)
  const minimum = Math.min(red, green, blue)
  const chroma = maximum - minimum
  const lightness = (maximum + minimum) / 2
  if (chroma === 0) return [0, 0, lightness]
  let hue: number
  if (maximum === red) hue = 60 * (((green - blue) / chroma) % 6)
  else if (maximum === green) hue = 60 * ((blue - red) / chroma + 2)
  else hue = 60 * ((red - green) / chroma + 4)
  const saturation = chroma / (1 - Math.abs(2 * lightness - 1))
  return [wrapDegrees(hue), saturation, lightness]
}

export function hslToEncodedSrgb(hsl: Hsl): [number, number, number] {
  const hue = wrapDegrees(hsl[0])
  const saturation = clamp(hsl[1])
  const lightness = clamp(hsl[2])
  const chroma = (1 - Math.abs(2 * lightness - 1)) * saturation
  const x = chroma * (1 - Math.abs((hue / 60) % 2 - 1))
  const sector: [number, number, number] = hue < 60 ? [chroma, x, 0]
    : hue < 120 ? [x, chroma, 0]
      : hue < 180 ? [0, chroma, x]
        : hue < 240 ? [0, x, chroma]
          : hue < 300 ? [x, 0, chroma] : [chroma, 0, x]
  const offset = lightness - chroma / 2
  return sector.map(channel => clamp(channel + offset)) as [number, number, number]
}

function smoothstep(low: number, high: number, value: number): number {
  const amount = clamp((value - low) / (high - low))
  return amount * amount * (3 - 2 * amount)
}

export function hslSelectionMask(hsl: Hsl, selectionValue: HslSelection): number {
  if (hsl[1] === 0) return 0
  const selection = sanitizeHslSelective({ selection: selectionValue }).selection
  const distance = Math.abs(wrapDegrees(hsl[0] - selection.centerDegrees + 180) - 180)
  if (distance <= selection.halfWidthDegrees) return 1
  if (selection.featherDegrees === 0 || distance >= selection.halfWidthDegrees + selection.featherDegrees) return 0
  return 1 - smoothstep(
    selection.halfWidthDegrees,
    selection.halfWidthDegrees + selection.featherDegrees,
    distance,
  )
}

export function applyHslSelectiveEncodedSrgb(rgb: Rgb, value: HslSelective): [number, number, number] {
  const safe = sanitizeHslSelective(value)
  const hsl = encodedSrgbToHsl(rgb)
  const mask = hslSelectionMask(hsl, safe.selection)
  return hslToEncodedSrgb([
    hsl[0] + mask * safe.adjustment.hueDegrees,
    clamp(hsl[1] + mask * safe.adjustment.saturation),
    clamp(hsl[2] + 0.25 * mask * safe.adjustment.lightness),
  ])
}

/** Mutates encoded-sRGB canvas bytes. `mask` is UI-only; alpha is always preserved. */
export function applyHslSelectiveToImageData(
  image: ImageData,
  value: HslSelective,
  mode: 'grade' | 'mask' = 'grade',
): void {
  const safe = sanitizeHslSelective(value)
  for (let index = 0; index < image.data.length; index += 4) {
    const rgb: Rgb = [image.data[index]! / 255, image.data[index + 1]! / 255, image.data[index + 2]! / 255]
    if (mode === 'mask') {
      const byte = Math.round(hslSelectionMask(encodedSrgbToHsl(rgb), safe.selection) * 255)
      image.data[index] = byte
      image.data[index + 1] = byte
      image.data[index + 2] = byte
    } else {
      const output = applyHslSelectiveEncodedSrgb(rgb, safe)
      image.data[index] = Math.round(output[0] * 255)
      image.data[index + 1] = Math.round(output[1] * 255)
      image.data[index + 2] = Math.round(output[2] * 255)
    }
  }
}
