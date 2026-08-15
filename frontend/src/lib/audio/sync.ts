import type { WaveformBucket } from './waveform'

export const MAX_SYNC_SAMPLES = 16_384

export interface WaveformSyncOptions {
  /** Number of envelope samples per second. */
  readonly sampleRateHz: number
  readonly maxOffsetSeconds: number
  readonly minOverlapSeconds?: number
}

export interface WaveformSyncResult {
  /** Add this value to the candidate timeline start to align it to reference. */
  readonly candidateStartOffsetSeconds: number
  readonly correlation: number
  /** 0..1 estimate based on peak height and separation from competing peaks. */
  readonly confidence: number
  readonly overlapSeconds: number
}

/** Convert bounded min/max/RMS summaries into a positive activity envelope. */
export function waveformEnergy(buckets: readonly WaveformBucket[]): Float32Array {
  if (buckets.length > MAX_SYNC_SAMPLES) throw new Error('Слишком много samples для локальной синхронизации')
  return Float32Array.from(buckets, (bucket) => {
    const rms = finiteUnit(bucket.rms)
    const peak = Math.max(Math.abs(finiteSigned(bucket.min)), Math.abs(finiteSigned(bucket.max)))
    return Math.max(rms, peak * 0.5)
  })
}

/**
 * Find the candidate timeline offset with normalized cross-correlation.
 * Positive means the candidate starts later; negative means it starts earlier.
 */
export function estimateWaveformOffset(
  reference: Float32Array,
  candidate: Float32Array,
  options: WaveformSyncOptions,
): WaveformSyncResult | null {
  validateInputs(reference, candidate, options)
  const sampleRate = options.sampleRateHz
  const maxOffset = Math.min(
    Math.round(options.maxOffsetSeconds * sampleRate),
    Math.max(reference.length, candidate.length) - 1,
  )
  const minimumOverlap = Math.max(
    8,
    Math.round((options.minOverlapSeconds ?? Math.min(1, options.maxOffsetSeconds)) * sampleRate),
  )
  const centeredReference = center(reference)
  const centeredCandidate = center(candidate)
  if (!hasActivity(centeredReference) || !hasActivity(centeredCandidate)) return null

  const candidates: Array<{ shift: number; correlation: number; overlap: number }> = []
  for (let shift = -maxOffset; shift <= maxOffset; shift += 1) {
    const start = Math.max(0, shift)
    const end = Math.min(reference.length, candidate.length + shift)
    const overlap = end - start
    if (overlap < minimumOverlap) continue

    let dot = 0
    let referenceEnergy = 0
    let candidateEnergy = 0
    for (let referenceIndex = start; referenceIndex < end; referenceIndex += 1) {
      const candidateIndex = referenceIndex - shift
      const left = centeredReference[referenceIndex]!
      const right = centeredCandidate[candidateIndex]!
      dot += left * right
      referenceEnergy += left * left
      candidateEnergy += right * right
    }
    const denominator = Math.sqrt(referenceEnergy * candidateEnergy)
    if (denominator <= Number.EPSILON) continue
    candidates.push({ shift, correlation: dot / denominator, overlap })
  }
  if (!candidates.length) return null
  candidates.sort(
    (left, right) =>
      right.correlation - left.correlation ||
      Math.abs(left.shift) - Math.abs(right.shift) ||
      left.shift - right.shift,
  )
  const best = candidates[0]!
  if (best.correlation < 0.2) return null
  const exclusionRadius = Math.max(1, Math.round(sampleRate * 0.05))
  const competitor = candidates.find((candidate) => Math.abs(candidate.shift - best.shift) > exclusionRadius)
  const separation = Math.max(0, best.correlation - (competitor?.correlation ?? 0))
  const confidence = clampUnit(best.correlation * 0.7 + Math.min(1, separation * 4) * 0.3)
  return {
    candidateStartOffsetSeconds: best.shift / sampleRate,
    correlation: best.correlation,
    confidence,
    overlapSeconds: best.overlap / sampleRate,
  }
}

function validateInputs(
  reference: Float32Array,
  candidate: Float32Array,
  options: WaveformSyncOptions,
): void {
  if (!reference.length || !candidate.length) throw new Error('Для синхронизации нужны две непустые формы волны')
  if (reference.length > MAX_SYNC_SAMPLES || candidate.length > MAX_SYNC_SAMPLES) {
    throw new Error('Слишком много samples для локальной синхронизации')
  }
  if (!Number.isFinite(options.sampleRateHz) || options.sampleRateHz <= 0 || options.sampleRateHz > 1_000) {
    throw new Error('Некорректная частота envelope')
  }
  if (!Number.isFinite(options.maxOffsetSeconds) || options.maxOffsetSeconds < 0 || options.maxOffsetSeconds > 600) {
    throw new Error('Некорректное окно синхронизации')
  }
  if (
    options.minOverlapSeconds !== undefined &&
    (!Number.isFinite(options.minOverlapSeconds) || options.minOverlapSeconds <= 0)
  ) {
    throw new Error('Некорректная минимальная длительность overlap')
  }
}

function center(values: Float32Array): Float64Array {
  let mean = 0
  for (const value of values) mean += Number.isFinite(value) ? value : 0
  mean /= values.length
  return Float64Array.from(values, (value) => (Number.isFinite(value) ? value : 0) - mean)
}

function hasActivity(values: Float64Array): boolean {
  let energy = 0
  for (const value of values) energy += value * value
  return energy > 1e-8
}

function finiteSigned(value: number): number {
  return Number.isFinite(value) ? Math.max(-1, Math.min(1, value)) : 0
}

function finiteUnit(value: number): number {
  return Number.isFinite(value) ? clampUnit(value) : 0
}

function clampUnit(value: number): number {
  return Math.max(0, Math.min(1, value))
}
