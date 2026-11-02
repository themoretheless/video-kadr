import type { NativeColorMatch } from '$lib/api'
import type { WireLumetri } from './types'

/** FilmCraft Apply Match clears only solved channels' keys and preserves the rest of the grade. */
export function applyColorMatch(parameters: WireLumetri, match: NativeColorMatch): WireLumetri {
  const coordinates = [...match.shadows, ...match.midtones, ...match.highlights]
  if (coordinates.some(value => !Number.isFinite(value) || Math.abs(value) > 1)
    || match.lightness.some(value => !Number.isFinite(value) || Math.abs(value) > 100)
    || !Number.isFinite(match.saturation) || match.saturation < 0 || match.saturation > 200) {
    throw new Error('Некорректный результат Color Match')
  }
  const basicAnimation = { ...parameters.animation?.basic }
  delete basicAnimation.saturation
  const gradeAnimation = { ...parameters.animation?.grade }
  for (const key of ['shadowsX', 'shadowsY', 'shadowsLightness', 'midtonesX', 'midtonesY', 'midtonesLightness', 'highlightsX', 'highlightsY', 'highlightsLightness'] as const) delete gradeAnimation[key]
  return {
    ...parameters, enabled: true, wheelsOn: true,
    basic: { ...parameters.basic, saturation: match.saturation },
    grade: {
      ...parameters.grade,
      shadows: { x: match.shadows[0], y: match.shadows[1], lightness: match.lightness[0] },
      midtones: { x: match.midtones[0], y: match.midtones[1], lightness: match.lightness[1] },
      highlights: { x: match.highlights[0], y: match.highlights[1], lightness: match.lightness[2] },
    },
    ...(parameters.animation ? { animation: { ...parameters.animation, basic: basicAnimation, grade: gradeAnimation } } : {}),
  }
}
