import type { WaveformSummary } from './waveform'

export const MAX_AUTO_BEAT_MARKERS = 256

export interface BeatDetectionOptions {
  /** 0 is conservative, 1 is permissive. */
  readonly sensitivity?: number
  readonly minimumSpacingSeconds?: number
  readonly maxBeats?: number
}

export interface DetectedBeat {
  readonly timeSeconds: number
  readonly strength: number
}

export interface BeatDetectionResult {
  readonly beats: readonly DetectedBeat[]
  readonly estimatedBpm: number | null
}

/**
 * Deterministic onset detection over bounded waveform RMS buckets. This is a
 * classical local energy-flux detector: it uses no model, network or semantic
 * audio classification.
 */
export function detectWaveformBeats(
  summary: WaveformSummary,
  options: BeatDetectionOptions = {},
): BeatDetectionResult {
  validateSummary(summary)
  const sensitivity = boundedNumber(options.sensitivity ?? 0.5, 0, 1, 'beat sensitivity')
  const minimumSpacingSeconds = boundedNumber(
    options.minimumSpacingSeconds ?? 0.2,
    0.1,
    2,
    'minimum beat spacing',
  )
  const maxBeats = options.maxBeats ?? MAX_AUTO_BEAT_MARKERS
  if (!Number.isSafeInteger(maxBeats) || maxBeats < 1 || maxBeats > MAX_AUTO_BEAT_MARKERS) {
    throw new Error(`Auto Beat supports 1..=${MAX_AUTO_BEAT_MARKERS} markers`)
  }

  const bucketRate = summary.buckets.length / summary.durationSeconds
  const energy = summary.buckets.map((bucket) => Math.log1p(12 * clampUnit(bucket.rms)))
  const smoothed = energy.map((value, index) => (
    (energy[index - 1] ?? value) + value * 2 + (energy[index + 1] ?? value)
  ) / 4)
  const novelty = smoothed.map((value, index) => index === 0 ? 0 : Math.max(0, value - smoothed[index - 1]!))
  const halfWindow = Math.max(2, Math.min(64, Math.round(bucketRate * 0.5)))
  const candidates: DetectedBeat[] = []
  for (let index = 1; index < novelty.length - 1; index += 1) {
    const value = novelty[index]!
    if (value <= 0 || value < novelty[index - 1]! || value < novelty[index + 1]!) continue
    const neighborhood = novelty.slice(Math.max(0, index - halfWindow), Math.min(novelty.length, index + halfWindow + 1))
    const median = percentile(neighborhood, 0.5)
    const deviations = neighborhood.map((sample) => Math.abs(sample - median))
    const mad = percentile(deviations, 0.5)
    const threshold = median + (2.8 - sensitivity * 2) * Math.max(0.0025, mad)
    if (value <= threshold) continue
    candidates.push({
      timeSeconds: ((index + 0.5) / summary.buckets.length) * summary.durationSeconds,
      strength: value / Math.max(threshold, 1e-9),
    })
  }

  const spaced: DetectedBeat[] = []
  for (const candidate of candidates) {
    const previous = spaced[spaced.length - 1]
    if (!previous || candidate.timeSeconds - previous.timeSeconds >= minimumSpacingSeconds) {
      spaced.push(candidate)
    } else if (candidate.strength > previous.strength) {
      spaced[spaced.length - 1] = candidate
    }
  }
  const beats = spaced.length <= maxBeats
    ? spaced
    : [...spaced]
      .sort((left, right) => right.strength - left.strength || left.timeSeconds - right.timeSeconds)
      .slice(0, maxBeats)
      .sort((left, right) => left.timeSeconds - right.timeSeconds)
  return { beats, estimatedBpm: estimateTempo(beats) }
}

function estimateTempo(beats: readonly DetectedBeat[]): number | null {
  if (beats.length < 3) return null
  const histogram = new Map<number, { weight: number; weightedBpm: number }>()
  for (let index = 1; index < beats.length; index += 1) {
    const interval = beats[index]!.timeSeconds - beats[index - 1]!.timeSeconds
    if (interval < 0.1 || interval > 4) continue
    let bpm = 60 / interval
    while (bpm < 60) bpm *= 2
    while (bpm > 180) bpm /= 2
    const bin = Math.round(bpm)
    const weight = Math.min(beats[index - 1]!.strength, beats[index]!.strength)
    const current = histogram.get(bin) ?? { weight: 0, weightedBpm: 0 }
    current.weight += weight
    current.weightedBpm += bpm * weight
    histogram.set(bin, current)
  }
  const best = [...histogram.entries()].sort(
    ([leftBin, left], [rightBin, right]) => right.weight - left.weight || leftBin - rightBin,
  )[0]
  if (!best || best[1].weight <= 0) return null
  return Number((best[1].weightedBpm / best[1].weight).toFixed(1))
}

function validateSummary(summary: WaveformSummary): void {
  if (!Number.isFinite(summary.durationSeconds) || summary.durationSeconds <= 0) {
    throw new Error('Waveform duration is invalid')
  }
  if (!Number.isFinite(summary.sampleRate) || summary.sampleRate <= 0) throw new Error('Waveform sample rate is invalid')
  if (summary.buckets.length < 3 || summary.buckets.length > 8_192) {
    throw new Error('Auto Beat requires 3..=8192 waveform buckets')
  }
  for (const bucket of summary.buckets) {
    if (!Number.isFinite(bucket.rms) || bucket.rms < 0 || bucket.rms > 1) {
      throw new Error('Waveform RMS must be in 0..=1')
    }
  }
}

function percentile(values: readonly number[], ratio: number): number {
  const sorted = [...values].sort((left, right) => left - right)
  const index = Math.min(sorted.length - 1, Math.max(0, Math.floor(ratio * (sorted.length - 1))))
  return sorted[index] ?? 0
}

function boundedNumber(value: number, minimum: number, maximum: number, label: string): number {
  if (!Number.isFinite(value) || value < minimum || value > maximum) throw new Error(`Invalid ${label}`)
  return value
}

function clampUnit(value: number): number {
  return Math.max(0, Math.min(1, value))
}
