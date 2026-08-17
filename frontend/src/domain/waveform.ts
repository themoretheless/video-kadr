// Waveform peaks for audio assets. Decoding is the expensive part, so an asset
// is fetched and decoded exactly once per bucket count and every panel that
// shows the same track reuses the cached envelope. Nothing here touches the
// store or the transport layer, so the timeline panel can import it too.

/** Structural subset of `AudioBuffer` that peak extraction needs. */
export interface DecodedAudioLike {
  numberOfChannels: number
  length: number
  duration: number
  getChannelData(channel: number): Float32Array
}

/** Downsampled envelope: one min/max pair per horizontal bucket, -1..1. */
export interface WaveformPeaks {
  buckets: number
  /** Source duration in seconds, 0 when the decoder could not report one. */
  duration: number
  min: Float32Array
  max: Float32Array
}

export interface WaveformStyle {
  /** CSS colour of the envelope. */
  color: string
  /** Optional background fill; the canvas is cleared when it is omitted. */
  background?: string
  /** 0..1 of the width drawn in `progressColor` instead of `color`. */
  progress?: number
  progressColor?: string
}

export const DEFAULT_WAVEFORM_BUCKETS = 320

/** Hard caps: a hostile duration or a huge canvas must not allocate without bound. */
const MAX_BUCKETS = 4096
const MAX_CHANNELS = 32

function bucketCount(requested: number): number {
  if (!Number.isFinite(requested)) return DEFAULT_WAVEFORM_BUCKETS
  return Math.max(1, Math.min(MAX_BUCKETS, Math.round(requested)))
}

/**
 * Reduce decoded PCM to one min/max pair per bucket, merging every channel into
 * the same envelope. Non-finite samples are skipped rather than propagated.
 */
export function extractPeaks(
  audio: DecodedAudioLike,
  buckets: number = DEFAULT_WAVEFORM_BUCKETS,
): WaveformPeaks {
  const count = bucketCount(buckets)
  const min = new Float32Array(count)
  const max = new Float32Array(count)
  const duration = Number.isFinite(audio.duration) ? Math.max(0, audio.duration) : 0
  const channels = Math.max(0, Math.min(MAX_CHANNELS, Math.trunc(audio.numberOfChannels)))
  const length = Number.isFinite(audio.length) ? Math.max(0, Math.trunc(audio.length)) : 0
  if (channels === 0 || length === 0) return { buckets: count, duration, min, max }

  const step = length / count
  for (let channel = 0; channel < channels; channel += 1) {
    const data = audio.getChannelData(channel)
    for (let bucket = 0; bucket < count; bucket += 1) {
      const from = Math.min(length - 1, Math.floor(bucket * step))
      const to = bucket === count - 1 ? length : Math.max(from + 1, Math.floor((bucket + 1) * step))
      let low = 0
      let high = 0
      for (let index = from; index < to && index < data.length; index += 1) {
        const sample = data[index] ?? 0
        if (!Number.isFinite(sample)) continue
        if (sample < low) low = sample
        if (sample > high) high = sample
      }
      if (low < (min[bucket] ?? 0)) min[bucket] = Math.max(-1, low)
      if (high > (max[bucket] ?? 0)) max[bucket] = Math.min(1, high)
    }
  }
  return { buckets: count, duration, min, max }
}

type AudioContextConstructor = new () => AudioContext

/** WebKit still ships the prefixed constructor; both are optional at runtime. */
function audioContextConstructor(): AudioContextConstructor | null {
  const scope = globalThis as {
    AudioContext?: AudioContextConstructor
    webkitAudioContext?: AudioContextConstructor
  }
  return scope.AudioContext ?? scope.webkitAudioContext ?? null
}

/**
 * Decode one encoded audio body and reduce it to peaks. Returns null when the
 * environment has no Web Audio implementation or the bytes are not decodable.
 */
export async function decodeAudioPeaks(
  bytes: ArrayBuffer,
  buckets: number = DEFAULT_WAVEFORM_BUCKETS,
): Promise<WaveformPeaks | null> {
  const Constructor = audioContextConstructor()
  if (!Constructor) return null
  const context = new Constructor()
  try {
    const decoded = await context.decodeAudioData(bytes)
    return extractPeaks(decoded, buckets)
  } catch {
    // An unsupported codec is a preview problem, never a render problem.
    return null
  } finally {
    void context.close?.()
  }
}

const waveformCache = new Map<string, Promise<WaveformPeaks | null>>()

/**
 * Peaks for a stored asset URL, decoded at most once per bucket count. A failed
 * load drops out of the cache so a later retry can succeed.
 */
export function loadWaveform(
  url: string,
  buckets: number = DEFAULT_WAVEFORM_BUCKETS,
): Promise<WaveformPeaks | null> {
  const key = `${bucketCount(buckets)}|${url}`
  const cached = waveformCache.get(key)
  if (cached) return cached
  const pending = fetch(url)
    .then((response) => (response.ok ? response.arrayBuffer() : null))
    .then((bytes) => (bytes ? decodeAudioPeaks(bytes, buckets) : null))
    .catch(() => null)
    .then((peaks) => {
      if (!peaks) waveformCache.delete(key)
      return peaks
    })
  waveformCache.set(key, pending)
  return pending
}

export function clearWaveformCache(): void {
  waveformCache.clear()
}

/**
 * Paint an envelope into a canvas. The canvas backing store is resized to the
 * CSS box times the device pixel ratio, so the result stays crisp on HiDPI.
 * Returns false when no 2D context is available (older browsers, jsdom).
 */
export function drawWaveform(
  canvas: HTMLCanvasElement,
  peaks: WaveformPeaks | null,
  style: WaveformStyle,
): boolean {
  const context = canvas.getContext?.('2d')
  if (!context) return false

  const ratio = Math.max(1, Math.min(3, globalThis.devicePixelRatio || 1))
  const width = Math.max(1, Math.round((canvas.clientWidth || canvas.width || 1) * ratio))
  const height = Math.max(1, Math.round((canvas.clientHeight || canvas.height || 1) * ratio))
  if (canvas.width !== width) canvas.width = width
  if (canvas.height !== height) canvas.height = height

  context.clearRect(0, 0, width, height)
  if (style.background) {
    context.fillStyle = style.background
    context.fillRect(0, 0, width, height)
  }
  if (!peaks || peaks.buckets === 0) return true

  const middle = height / 2
  const half = middle - 1
  const columnWidth = width / peaks.buckets
  const progressX =
    style.progress === undefined ? -1 : Math.max(0, Math.min(1, style.progress)) * width
  for (let bucket = 0; bucket < peaks.buckets; bucket += 1) {
    const x = bucket * columnWidth
    const top = middle - (peaks.max[bucket] ?? 0) * half
    const bottom = middle - (peaks.min[bucket] ?? 0) * half
    context.fillStyle = x < progressX ? (style.progressColor ?? style.color) : style.color
    // A silent bucket still gets a hairline so the track reads as continuous.
    context.fillRect(x, top, Math.max(1, columnWidth - 0.5), Math.max(1, bottom - top))
  }
  return true
}
