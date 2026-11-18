import type { SubtitleKaraokeSpan } from './types'

export function rebaseKaraokeTiming<T extends Pick<SubtitleKaraokeSpan, 'startTicks' | 'endTicks'>>(spans: readonly T[], deltaUs: number): T[] {
  return spans.map(span => {
    const startTicks = span.startTicks + deltaUs
    const endTicks = span.endTicks + deltaUs
    if (!Number.isSafeInteger(startTicks) || !Number.isSafeInteger(endTicks)) {
      throw new Error('Время караоке выходит за допустимый диапазон')
    }
    return { ...span, startTicks, endTicks }
  })
}
