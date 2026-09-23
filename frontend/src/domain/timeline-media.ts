export interface PeakBuckets {
  min: Float32Array
  max: Float32Array
}

/** Downsample PCM samples into per-bucket min/max envelope pairs. */
export function computePeakBuckets(samples: Float32Array, bucketCount: number): PeakBuckets {
  const buckets = Math.max(1, Math.floor(bucketCount))
  const min = new Float32Array(buckets)
  const max = new Float32Array(buckets)
  if (samples.length === 0) return { min, max }
  const bucketSize = Math.max(1, Math.floor(samples.length / buckets))
  for (let bucket = 0; bucket < buckets; bucket += 1) {
    const start = bucket * bucketSize
    if (start >= samples.length) {
      min[bucket] = 0
      max[bucket] = 0
      continue
    }
    const end = Math.min(samples.length, start + bucketSize)
    let low = samples[start]!
    let high = samples[start]!
    for (let index = start + 1; index < end; index += 1) {
      const value = samples[index]!
      if (value < low) low = value
      if (value > high) high = value
    }
    min[bucket] = low
    max[bucket] = high
  }
  return { min, max }
}

/** Evenly spaced source ticks (clip-relative) for thumbnail sampling. */
export function thumbnailSliceTicks(durationTicks: number, sliceCount: number): number[] {
  const count = Math.max(1, Math.floor(sliceCount))
  if (durationTicks <= 0) return []
  return Array.from(
    { length: count },
    (_, index) => Math.round(((index + 0.5) / count) * durationTicks),
  )
}

/** Map a clip's source range onto bucket indices of a fixed-size peak cache. */
export function peakBucketRange(
  sourceInTick: number,
  durationTicks: number,
  mediaDurationSeconds: number,
  timeBase: number,
  bucketCount: number,
): { start: number; end: number } {
  if (!(mediaDurationSeconds > 0) || !(timeBase > 0)) return { start: 0, end: 0 }
  const mediaTicks = mediaDurationSeconds * timeBase
  const scale = bucketCount / mediaTicks
  const start = Math.max(0, Math.min(bucketCount - 1, Math.floor(sourceInTick * scale)))
  const end = Math.max(start + 1, Math.min(bucketCount, Math.ceil((sourceInTick + durationTicks) * scale)))
  return { start, end }
}
