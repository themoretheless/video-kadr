import type { ColorWheelChannels } from '../types'
import { colorWheelsActive, sanitizeColorWheel } from './edit'
import {
  applyPrimaryCorrectionsLinearSrgb,
  decodeSrgb,
  encodeSrgb,
  primaryCorrectionsActive,
  type PrimaryCorrections,
  type Rgb,
} from './primary-color'

export interface LiftGammaGain {
  lift: ColorWheelChannels
  gamma: ColorWheelChannels
  gain: ColorWheelChannels
}

export const LIFT_GAMMA_GAIN_PIPELINE_ORDER = [
  'decode-transfer',
  'primary-corrections',
  'lift',
  'gamma',
  'gain',
  'encode-transfer',
  'brightness-contrast-saturation',
  'preset',
  'lut',
  'curves',
] as const

export function sanitizeLiftGammaGain(value: {
  lift?: unknown
  gamma?: unknown
  gain?: unknown
} | null | undefined): LiftGammaGain {
  return {
    lift: sanitizeColorWheel(value?.lift),
    gamma: sanitizeColorWheel(value?.gamma),
    gain: sanitizeColorWheel(value?.gain),
  }
}

function wheelControl(wheel: ColorWheelChannels, channel: 'red' | 'green' | 'blue'): number {
  return wheel.master + wheel[channel]
}

/** Apply the shared fixture's Lift → Gamma → Gain stage to linear-light sRGB. */
export function applyLiftGammaGainLinearSrgb(rgb: Rgb, value: LiftGammaGain): [number, number, number] {
  const safe = sanitizeLiftGammaGain(value)
  return (['red', 'green', 'blue'] as const).map((channel, index) => {
    const lifted = Math.max(0, rgb[index]! + 0.25 * wheelControl(safe.lift, channel))
    const gammaExponent = 2 ** -wheelControl(safe.gamma, channel)
    const gained = lifted ** gammaExponent * 2 ** wheelControl(safe.gain, channel)
    return Math.max(0, Math.min(1, gained))
  }) as [number, number, number]
}

/** Combined exact stage used by the paused browser preview. */
export function applyPrimaryAndWheelsSrgb(
  rgb: Rgb,
  primary: PrimaryCorrections,
  wheels: LiftGammaGain,
): [number, number, number] {
  const linear = rgb.map(decodeSrgb) as [number, number, number]
  const primaryCorrected = applyPrimaryCorrectionsLinearSrgb(linear, primary)
  return applyLiftGammaGainLinearSrgb(primaryCorrected, wheels).map(encodeSrgb) as [number, number, number]
}

const BYTE_TO_LINEAR = Array.from({ length: 256 }, (_, value) => decodeSrgb(value / 255))

/** One decode/encode pass; alpha is preserved and no intermediate byte quantisation is introduced. */
export function applyPrimaryAndWheelsToImageData(
  image: ImageData,
  primary: PrimaryCorrections,
  wheels: LiftGammaGain,
): void {
  const safeWheels = sanitizeLiftGammaGain(wheels)
  const primaryActive = primaryCorrectionsActive(primary)
  if (!primaryActive && !colorWheelsActive(safeWheels)) return
  const bytes = image.data
  for (let index = 0; index < bytes.length; index += 4) {
    const input: [number, number, number] = [
      BYTE_TO_LINEAR[bytes[index]!]!,
      BYTE_TO_LINEAR[bytes[index + 1]!]!,
      BYTE_TO_LINEAR[bytes[index + 2]!]!,
    ]
    const corrected = applyLiftGammaGainLinearSrgb(
      applyPrimaryCorrectionsLinearSrgb(input, primary),
      safeWheels,
    )
    bytes[index] = Math.round(encodeSrgb(corrected[0]) * 255)
    bytes[index + 1] = Math.round(encodeSrgb(corrected[1]) * 255)
    bytes[index + 2] = Math.round(encodeSrgb(corrected[2]) * 255)
  }
}
