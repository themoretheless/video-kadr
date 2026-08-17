// HSL secondaries list editing. The wire carries one entry per band and only
// for the bands the user actually touched, so a band back at its defaults has
// to leave the list rather than sit in it neutral.

import type { HslAdjustment, HslBand } from '../../types'

/**
 * Bands the export implements. `backend/src/render/graph/color.rs` drives
 * `huesaturation`, which has no orange range, so orange is deliberately absent:
 * offering it would be a control that silently does nothing.
 */
export const HSL_UI_BANDS: { band: HslBand; label: string }[] = [
  { band: 'red', label: 'Красный' },
  { band: 'yellow', label: 'Жёлтый' },
  { band: 'green', label: 'Зелёный' },
  { band: 'cyan', label: 'Голубой' },
  { band: 'blue', label: 'Синий' },
  { band: 'magenta', label: 'Пурпурный' },
]

/**
 * The export turns the 0..4 multiplier into the filter's -1..1 shift, so
 * anything above 2 is clipped there. The sliders stop at 2 for that reason.
 */
export const HSL_SCALE_MAX = 2
export const HSL_HUE_LIMIT = 180

export type HslField = 'hue' | 'saturation' | 'luminance'

export function neutralAdjustment(band: HslBand): HslAdjustment {
  return { band, hue: 0, saturation: 1, luminance: 1 }
}

export function isNeutralAdjustment(value: HslAdjustment): boolean {
  return value.hue === 0 && value.saturation === 1 && value.luminance === 1
}

/** The stored adjustment for a band, or a neutral one when it is untouched. */
export function adjustmentOf(list: readonly HslAdjustment[], band: HslBand): HslAdjustment {
  return list.find((entry) => entry.band === band) ?? neutralAdjustment(band)
}

export function withoutBand(list: readonly HslAdjustment[], band: HslBand): HslAdjustment[] {
  return list.filter((entry) => entry.band !== band)
}

/**
 * A new list with one field of one band changed. Non-finite input is ignored,
 * everything else is clamped, and a band that lands back on its defaults is
 * dropped so the payload stays minimal.
 */
export function withBandField(
  list: readonly HslAdjustment[],
  band: HslBand,
  field: HslField,
  raw: number,
): HslAdjustment[] {
  if (!Number.isFinite(raw)) return [...list]
  const limit = field === 'hue' ? HSL_HUE_LIMIT : HSL_SCALE_MAX
  const min = field === 'hue' ? -HSL_HUE_LIMIT : 0
  const next: HslAdjustment = {
    ...adjustmentOf(list, band),
    [field]: Math.max(min, Math.min(limit, raw)),
  }
  const rest = withoutBand(list, band)
  return isNeutralAdjustment(next) ? rest : [...rest, next]
}
