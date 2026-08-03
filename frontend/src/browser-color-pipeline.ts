import {
  colorWheelsActive,
  sanitizePrimaryCorrection,
} from './domain/edit'
import { sanitizeLiftGammaGain } from './domain/color-wheels'

type EditPayload = Record<string, unknown>

export interface BrowserColorFilterPlan {
  /** Linear colour work that must happen before the LUT branch. */
  beforeLut: string[]
  /** A prepared LUT filter, or null when the LUT is neutral/absent. */
  lutFilter: string | null
  /** Canonical 0..1 blend amount. */
  lutIntensity: number
  /** Authored curves deliberately run after the LUT (and after a partial blend). */
  afterLut: string[]
}

interface BrowserVideoFilterArgsOptions {
  prefixFilters?: string[]
  suffixFilters?: string[]
  mapAudio?: boolean
}

function number(value: unknown, fallback = 0): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

function record(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null
}

function curvePoints(value: unknown): string {
  if (!Array.isArray(value)) return '0/0 1/1'
  return value
    .map(record)
    .filter((point): point is Record<string, unknown> => Boolean(point))
    .map((point) => {
      const normalize = (coordinate: unknown) => {
        const value = number(coordinate)
        return Math.max(0, Math.min(1, value > 1 ? value / 255 : value))
      }
      return `${normalize(point.x)}/${normalize(point.y)}`
    })
    .join(' ')
}

function presetFilter(name: string): string | null {
  const presets: Record<string, string> = {
    grayscale: 'hue=s=0',
    sepia: 'colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131',
    warm: 'colorbalance=rs=.08:bs=-.06',
    cold: 'colorbalance=rs=-.06:bs=.08',
    'teal-orange': 'colorbalance=rs=.05:gs=-.02:bs=.05',
    faded: 'eq=contrast=.85:brightness=.04:saturation=.9',
    noir: 'hue=s=0,eq=contrast=1.4',
    vintage: 'curves=vintage',
  }
  return presets[name] ?? null
}

function escapeFilterPath(path: string): string {
  return path.replace(/([\\':,;[\]])/g, '\\$1')
}

/** Primary + Lift/Gamma/Gain in one linear-sRGB decode/encode pass. */
export function linearColorCorrectionFfmpegFilter(payload: EditPayload): string | null {
  const temperature = sanitizePrimaryCorrection(payload.temperature)
  const tint = sanitizePrimaryCorrection(payload.tint)
  const highlights = sanitizePrimaryCorrection(payload.highlights)
  const shadows = sanitizePrimaryCorrection(payload.shadows)
  const primaryActive = temperature !== 0 || tint !== 0 || highlights !== 0 || shadows !== 0
  const colorWheels = record(payload.colorWheels)
  const wheels = sanitizeLiftGammaGain({
    lift: record(colorWheels?.lift) ?? undefined,
    gamma: record(colorWheels?.gamma) ?? undefined,
    gain: record(colorWheels?.gain) ?? undefined,
  })
  const wheelsActive = colorWheelsActive(wheels)
  if (!primaryActive && !wheelsActive) return null
  const gains = [
    2 ** (0.25 * temperature - 0.10 * tint),
    2 ** (0.20 * tint),
    2 ** (-0.25 * temperature - 0.10 * tint),
  ]
  const linear = (channel: string, gain: number) => {
    const normalized = `(${channel}(X,Y)/65535)`
    return `(if(lte(${normalized},0.04045),${normalized}/12.92,pow((${normalized}+0.055)/1.055,2.4))*${gain.toFixed(12)})`
  }
  const red = linear('r', gains[0]!)
  const green = linear('g', gains[1]!)
  const blue = linear('b', gains[2]!)
  const luma = `(0.2126*${red}+0.7152*${green}+0.0722*${blue})`
  const smooth = (value: string, low: number, high: number) => {
    const amount = `clip((${value}-${low})/(${high}-${low}),0,1)`
    return `(${amount}*${amount}*(3-2*${amount}))`
  }
  const shadowMask = `(1-${smooth(luma, 0, 0.5)})`
  const highlightMask = smooth(luma, 0.5, 1)
  const tonalGain = `pow(2,0.75*(${shadows.toFixed(12)}*${shadowMask}+${highlights.toFixed(12)}*${highlightMask}))`
  const primaryChannels = [red, green, blue].map(channel =>
    primaryActive ? `clip((${channel})*${tonalGain},0,1)` : channel,
  )
  const channelNames = ['red', 'green', 'blue'] as const
  const correctedChannels = primaryChannels.map((channel, index) => {
    const name = channelNames[index]!
    const lift = wheels.lift.master + wheels.lift[name]
    const gamma = wheels.gamma.master + wheels.gamma[name]
    const gain = wheels.gain.master + wheels.gain[name]
    const lifted = `max(0,(${channel})+0.25*${lift.toFixed(12)})`
    const gammaCorrected = `pow(${lifted},pow(2,${(-gamma).toFixed(12)}))`
    return `clip((${gammaCorrected})*pow(2,${gain.toFixed(12)}),0,1)`
  })
  const encode = (corrected: string) => {
    return `65535*if(lte(${corrected},0.0031308),12.92*${corrected},1.055*pow(${corrected},0.416666666666667)-0.055)`
  }
  return `geq=r='${encode(correctedChannels[0]!)}':g='${encode(correctedChannels[1]!)}':b='${encode(correctedChannels[2]!)}':a='alpha(X,Y)'`
}

/** Compatibility export for callers/tests that only supply primary controls. */
export function primaryCorrectionFfmpegFilter(payload: EditPayload): string | null {
  return linearColorCorrectionFfmpegFilter({ ...payload, colorWheels: undefined })
}

/** Color-only filter plan. Ordering is primary → EQ → preset → LUT → curves. */
export function browserColorFilterPlan(payload: EditPayload, lutFilename?: string): BrowserColorFilterPlan {
  const beforeLut: string[] = []
  const afterLut: string[] = []
  const linearCorrection = linearColorCorrectionFfmpegFilter(payload)
  const lut = record(payload.lut)
  const lutIntensity = lut
    ? Math.max(0, Math.min(1, number(lut.intensity, 1)))
    : 0
  const lutActive = Boolean(lut) && lutIntensity > 1e-9
  const curves = record(payload.curves)
  if (linearCorrection || lutActive || curves) beforeLut.push('format=gbrap16le')
  if (linearCorrection) beforeLut.push(linearCorrection)

  const brightness = number(payload.brightness)
  const contrast = number(payload.contrast, 1)
  const saturation = number(payload.saturation, 1)
  if (brightness || contrast !== 1 || saturation !== 1) {
    beforeLut.push(`eq=brightness=${brightness}:contrast=${contrast}:saturation=${saturation}`)
  }
  const preset = presetFilter(String(payload.filter || ''))
  if (preset) beforeLut.push(preset)
  let lutFilter: string | null = null
  if (lutActive) {
    if (!lutFilename) throw new Error('LUT filter requires a prepared browser asset')
    lutFilter = `lut3d=file='${escapeFilterPath(lutFilename)}':interp=tetrahedral`
  }
  if (curves) {
    afterLut.push(
      `curves=interp=pchip:master='${curvePoints(curves.master)}':r='${curvePoints(curves.red)}':g='${curvePoints(curves.green)}':b='${curvePoints(curves.blue)}'`,
    )
  }
  return { beforeLut, lutFilter, lutIntensity, afterLut }
}

/**
 * Compile the actual FFmpeg video-filter argv. Partial LUT intensity needs a
 * split/lut3d/blend graph; a simple `-vf` chain cannot express that topology.
 */
export function browserVideoFilterArgs(
  color: BrowserColorFilterPlan,
  options: BrowserVideoFilterArgsOptions = {},
): string[] {
  const beforeLut = [...(options.prefixFilters ?? []), ...color.beforeLut]
  const afterLut = [...color.afterLut, ...(options.suffixFilters ?? [])]
  const partialLut = color.lutFilter !== null && color.lutIntensity < 1 - 1e-9

  if (!partialLut) {
    const filters = [
      ...beforeLut,
      ...(color.lutFilter ? [color.lutFilter] : []),
      ...afterLut,
    ]
    return filters.length ? ['-vf', filters.join(',')] : []
  }

  const before = beforeLut.length ? `${beforeLut.join(',')},` : ''
  const after = afterLut.length ? `,${afterLut.join(',')}` : ''
  const intensity = color.lutIntensity.toFixed(6)
  const graph = [
    `[0:v]${before}split=2[browser_lut_base][browser_lut_input]`,
    `[browser_lut_input]${color.lutFilter}[browser_lut_applied]`,
    `[browser_lut_base][browser_lut_applied]blend=all_expr='A*(1-${intensity})+B*${intensity}'${after}[browser_vout]`,
  ].join(';')
  const args = ['-filter_complex', graph, '-map', '[browser_vout]']
  if (options.mapAudio) args.push('-map', '0:a?')
  return args
}
