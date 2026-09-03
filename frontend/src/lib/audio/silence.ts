import type { TimelineSegment } from '../types'
import { MAX_TIMELINE_SEGMENTS, TIMELINE_MIN_SEGMENT_DURATION } from '../domain/timeline'
import type { WaveformSummary } from './waveform'

export interface SilenceRemovalOptions {
  readonly thresholdDb: number
  readonly minimumSilenceSeconds: number
  readonly paddingSeconds: number
}

export interface SourceRange {
  readonly start: number
  readonly end: number
}

export const DEFAULT_SILENCE_REMOVAL: SilenceRemovalOptions = {
  thresholdDb: -42,
  minimumSilenceSeconds: 0.45,
  paddingSeconds: 0.08,
}

/**
 * Find source ranges worth retaining using only bounded waveform RMS buckets.
 * Silence padding is retained around audible material to avoid clipped words.
 */
export function detectAudibleRanges(
  summary: WaveformSummary,
  options: SilenceRemovalOptions = DEFAULT_SILENCE_REMOVAL,
): SourceRange[] {
  const { buckets, durationSeconds } = summary
  if (!Number.isFinite(durationSeconds) || durationSeconds <= 0 || buckets.length < 3 || buckets.length > 8_192) {
    throw new Error('Некорректная форма волны')
  }
  const inRange = (value: number, min: number, max: number) => Number.isFinite(value) && value >= min && value <= max
  if (
    !inRange(options.thresholdDb, -80, -6) ||
    !inRange(options.minimumSilenceSeconds, 0.1, 10) ||
    !inRange(options.paddingSeconds, 0, 2)
  ) {
    throw new Error('Некорректные настройки тишины')
  }

  const threshold = 10 ** (options.thresholdDb / 20)
  const bucketSeconds = durationSeconds / buckets.length
  const keep: SourceRange[] = []
  let runStart = -1
  let cursor = 0
  for (let index = 0; index <= buckets.length; index += 1) {
    const rms = index < buckets.length ? buckets[index]!.rms : Number.POSITIVE_INFINITY
    const silent = Number.isFinite(rms) && Math.max(0, rms) <= threshold
    if (silent && runStart < 0) runStart = index
    if ((!silent || index === buckets.length) && runStart >= 0) {
      const start = runStart * bucketSeconds
      const end = Math.min(durationSeconds, index * bucketSeconds)
      if (end - start >= options.minimumSilenceSeconds) {
        const cutStart = start === 0 ? 0 : start + options.paddingSeconds
        const cutEnd = end === durationSeconds ? durationSeconds : end - options.paddingSeconds
        if (cutEnd - cutStart >= TIMELINE_MIN_SEGMENT_DURATION) {
          if (cutStart - cursor >= TIMELINE_MIN_SEGMENT_DURATION) keep.push({ start: cursor, end: cutStart })
          cursor = cutEnd
        }
      }
      runStart = -1
    }
  }
  if (durationSeconds - cursor >= TIMELINE_MIN_SEGMENT_DURATION) keep.push({ start: cursor, end: durationSeconds })
  return keep
}

/** Preserve authored playback order and duplicates while cutting silent source intervals. */
export function retainAudibleTimelineSegments(
  segments: readonly TimelineSegment[],
  audibleRanges: readonly SourceRange[],
): TimelineSegment[] {
  const result: TimelineSegment[] = []
  for (const segment of segments) {
    for (const audible of audibleRanges) {
      const start = Math.max(segment.start, audible.start)
      const end = Math.min(segment.end, audible.end)
      if (end - start < TIMELINE_MIN_SEGMENT_DURATION) continue
      if (result.length >= MAX_TIMELINE_SEGMENTS) throw new Error(`Лимит: ${MAX_TIMELINE_SEGMENTS} фрагментов`)
      result.push({ id: `segment-${result.length + 1}`, start, end })
    }
  }
  if (!result.length) throw new Error('Весь фрагмент распознан как тишина')
  return result
}
