// Audio lane peaks. Decoding lives in `domain/waveform`, which already caches
// one decode per URL; this adapter pins the bucket count the lane draws at and
// adds the range lookup the lane needs when a clip is trimmed or reordered.

import { loadWaveform, type WaveformPeaks } from '../../domain/waveform'

/** Peak resolution. Finer than any realistic lane width, coarse enough to cache. */
const BUCKETS = 2048

const cache = new Map<string, WaveformPeaks | null>()

/** Cached peaks, `null` when the source carries no decodable audio. */
export function cachedPeaks(key: string): WaveformPeaks | null | undefined {
  return cache.get(key)
}

export function loadPeaks(key: string, url: string): Promise<WaveformPeaks | null> {
  if (cache.has(key)) return Promise.resolve(cache.get(key) ?? null)
  return loadWaveform(url, BUCKETS).then((peaks) => {
    cache.set(key, peaks)
    return peaks
  })
}

/**
 * Loudest sample over a normalized slice of the source, 0..1. `from` and `to`
 * are fractions of the whole source, which is what the lane can compute from a
 * clip's in-point without knowing the bucket layout.
 */
export function peakBetween(peaks: WaveformPeaks, from: number, to: number): number {
  const count = peaks.buckets
  if (count <= 0) return 0
  const first = Math.max(0, Math.min(count - 1, Math.floor(from * count)))
  const last = Math.max(first, Math.min(count - 1, Math.ceil(to * count) - 1))
  let peak = 0
  for (let index = first; index <= last; index += 1) {
    const high = Math.abs(peaks.max[index] ?? 0)
    const low = Math.abs(peaks.min[index] ?? 0)
    const value = high > low ? high : low
    if (value > peak) peak = value
  }
  return peak
}
