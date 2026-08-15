export const DEFAULT_WAVEFORM_BUCKETS = 2_048
export const MAX_WAVEFORM_BUCKETS = 8_192

export interface WaveformBucket {
  readonly min: number
  readonly max: number
  readonly rms: number
}

export interface WaveformSummary {
  readonly durationSeconds: number
  readonly sampleRate: number
  readonly buckets: readonly WaveformBucket[]
}

export interface DecodedAudioLike {
  readonly duration: number
  readonly sampleRate: number
  readonly numberOfChannels: number
  readonly length: number
  getChannelData(channel: number): Float32Array
}

export interface AudioDecoderPort {
  decodeAudioData(data: ArrayBuffer): Promise<DecodedAudioLike>
  close(): Promise<void>
}

export type AudioDecoderFactory = () => AudioDecoderPort

/**
 * Reduce PCM into bounded min/max/RMS buckets. Every input frame belongs to
 * exactly one bucket, so the result is deterministic and independent of the
 * canvas size used to display it.
 */
export function buildWaveformBuckets(
  channels: readonly Float32Array[],
  requestedBuckets: number,
  startFrame = 0,
  endFrame = channels[0]?.length ?? 0,
): WaveformBucket[] {
  if (!channels.length) return []
  const frameLength = Math.min(...channels.map((channel) => channel.length))
  const start = clampInteger(startFrame, 0, frameLength)
  const end = clampInteger(endFrame, start, frameLength)
  if (end <= start) return []

  const bucketCount = clampInteger(requestedBuckets, 1, Math.min(MAX_WAVEFORM_BUCKETS, end - start))
  const buckets: WaveformBucket[] = []
  for (let bucketIndex = 0; bucketIndex < bucketCount; bucketIndex += 1) {
    const bucketStart = start + Math.floor(((end - start) * bucketIndex) / bucketCount)
    const bucketEnd = start + Math.floor(((end - start) * (bucketIndex + 1)) / bucketCount)
    let min = 1
    let max = -1
    let sumSquares = 0
    let sampleCount = 0
    for (let frame = bucketStart; frame < Math.max(bucketStart + 1, bucketEnd); frame += 1) {
      for (const channel of channels) {
        const sample = sanitizeSample(channel[frame])
        min = Math.min(min, sample)
        max = Math.max(max, sample)
        sumSquares += sample * sample
        sampleCount += 1
      }
    }
    buckets.push({
      min: min === 1 && max === -1 ? 0 : min,
      max: min === 1 && max === -1 ? 0 : max,
      rms: sampleCount ? Math.sqrt(sumSquares / sampleCount) : 0,
    })
  }
  return buckets
}

/** Select and resample a normalized section of an already bounded summary. */
export function sliceWaveformBuckets(
  buckets: readonly WaveformBucket[],
  startRatio: number,
  endRatio: number,
  requestedBuckets: number,
): WaveformBucket[] {
  if (!buckets.length) return []
  const start = clampUnit(startRatio)
  const end = clampUnit(endRatio)
  if (end <= start) return []
  const first = Math.min(buckets.length - 1, Math.floor(start * buckets.length))
  const lastExclusive = Math.max(first + 1, Math.ceil(end * buckets.length))
  const source = buckets.slice(first, Math.min(lastExclusive, buckets.length))
  const count = clampInteger(requestedBuckets, 1, Math.min(MAX_WAVEFORM_BUCKETS, source.length))
  if (count === source.length) return source.map((bucket) => ({ ...bucket }))

  const result: WaveformBucket[] = []
  for (let outputIndex = 0; outputIndex < count; outputIndex += 1) {
    const sourceStart = Math.floor((source.length * outputIndex) / count)
    const sourceEnd = Math.max(sourceStart + 1, Math.floor((source.length * (outputIndex + 1)) / count))
    const group = source.slice(sourceStart, sourceEnd)
    result.push({
      min: Math.min(...group.map((bucket) => bucket.min)),
      max: Math.max(...group.map((bucket) => bucket.max)),
      rms: Math.sqrt(group.reduce((sum, bucket) => sum + bucket.rms * bucket.rms, 0) / group.length),
    })
  }
  return result
}

/** Build a compact closed SVG polygon around the zero axis. */
export function waveformPath(
  buckets: readonly WaveformBucket[],
  width: number,
  height: number,
): string {
  if (!buckets.length || !Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return ''
  }
  const center = height / 2
  const amplitude = Math.max(0, center - 1)
  const x = (index: number): string => formatCoordinate(((index + 0.5) / buckets.length) * width)
  const y = (sample: number): string => formatCoordinate(center - sanitizeSample(sample) * amplitude)
  const upper = buckets.map((bucket, index) => `${x(index)},${y(bucket.max)}`)
  const lower = [...buckets]
    .reverse()
    .map((bucket, reverseIndex) => `${x(buckets.length - reverseIndex - 1)},${y(bucket.min)}`)
  return `M${upper.join(' L')} L${lower.join(' L')} Z`
}

export async function decodeWaveform(
  data: ArrayBuffer,
  decoderFactory: AudioDecoderFactory = browserAudioDecoder,
  bucketCount = DEFAULT_WAVEFORM_BUCKETS,
): Promise<WaveformSummary> {
  const decoder = decoderFactory()
  try {
    const decoded = await decoder.decodeAudioData(data.slice(0))
    if (
      !Number.isFinite(decoded.duration) ||
      decoded.duration <= 0 ||
      !Number.isFinite(decoded.sampleRate) ||
      decoded.sampleRate <= 0 ||
      !Number.isSafeInteger(decoded.numberOfChannels) ||
      decoded.numberOfChannels <= 0 ||
      !Number.isSafeInteger(decoded.length) ||
      decoded.length <= 0
    ) {
      throw new Error('Декодер вернул некорректный аудиопоток')
    }
    const channels = Array.from(
      { length: decoded.numberOfChannels },
      (_, channel) => decoded.getChannelData(channel),
    )
    return {
      durationSeconds: decoded.duration,
      sampleRate: decoded.sampleRate,
      buckets: buildWaveformBuckets(channels, bucketCount),
    }
  } finally {
    await decoder.close().catch(() => undefined)
  }
}

function browserAudioDecoder(): AudioDecoderPort {
  const AudioContextConstructor = globalThis.AudioContext
  if (!AudioContextConstructor) throw new Error('Браузер не поддерживает локальное декодирование аудио')
  return new AudioContextConstructor() as AudioDecoderPort
}

function sanitizeSample(value: number | undefined): number {
  return Number.isFinite(value) ? Math.max(-1, Math.min(1, value!)) : 0
}

function clampUnit(value: number): number {
  return Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : 0
}

function clampInteger(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) return min
  return Math.max(min, Math.min(max, Math.round(value)))
}

function formatCoordinate(value: number): string {
  return Number(value.toFixed(3)).toString()
}
