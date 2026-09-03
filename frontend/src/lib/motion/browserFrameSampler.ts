import { rgbaToLuma, type GrayFrame } from './tracker'

export const MAX_BROWSER_TRACKING_WIDTH = 640
export const MAX_BROWSER_TRACKING_HEIGHT = 360
export const MAX_BROWSER_TRACKING_TOTAL_PIXELS = 64 * 1024 * 1024
export const DEFAULT_BROWSER_TRACKING_TIMEOUT_MS = 30_000

const LOCAL_MEDIA_PATH = /^\/files\/(?:sources|outputs|proxies)\/[^/%\\]{1,512}$/

export interface BrowserTrackingFrames {
  readonly frames: readonly GrayFrame[]
  readonly width: number
  readonly height: number
  /** Multiply sampled X coordinates by this value to recover source-raster pixels. */
  readonly sampleToSourceScaleX: number
  /** Multiply sampled Y coordinates by this value to recover source-raster pixels. */
  readonly sampleToSourceScaleY: number
}

export interface BrowserFrameSamplerOptions {
  readonly signal?: AbortSignal
  readonly origin?: string
  readonly maxWidth?: number
  readonly maxHeight?: number
  readonly timeoutMs?: number
  readonly createVideo?: () => HTMLVideoElement
  readonly createCanvas?: () => HTMLCanvasElement
}

/** Resolve only same-origin media mounts so canvas reads cannot leak remote pixels. */
export function validateBrowserTrackingMediaUrl(value: string, origin?: string): string {
  const baseOrigin = origin ?? window.location.origin
  const url = new URL(value, `${baseOrigin}/`)
  if (url.origin !== baseOrigin || !['http:', 'https:'].includes(url.protocol)) {
    throw new Error('Tracking читает только same-origin media')
  }
  if (!LOCAL_MEDIA_PATH.test(url.pathname)) {
    throw new Error('Tracking URL не принадлежит разрешённому media mount')
  }
  if (url.username || url.password || url.hash || url.search) {
    throw new Error('Tracking media URL содержит запрещённые credentials/query/hash')
  }
  return url.href
}

export function trackingRasterSize(
  sourceWidth: number,
  sourceHeight: number,
  maxWidth = MAX_BROWSER_TRACKING_WIDTH,
  maxHeight = MAX_BROWSER_TRACKING_HEIGHT,
): { width: number; height: number } {
  for (const [value, label] of [[sourceWidth, 'source width'], [sourceHeight, 'source height'], [maxWidth, 'max width'], [maxHeight, 'max height']] as const) {
    if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`Invalid ${label}`)
  }
  const scale = Math.min(1, maxWidth / sourceWidth, maxHeight / sourceHeight)
  return {
    width: Math.max(1, Math.round(sourceWidth * scale)),
    height: Math.max(1, Math.round(sourceHeight * scale)),
  }
}

/**
 * Seek a private same-origin video to bounded timestamps and copy luma frames.
 * Requests are decoded in ascending source order, then restored to presentation
 * order for reverse playback. Detached video/canvas resources are always released.
 */
export async function sampleBrowserVideoLumaFrames(
  mediaUrl: string,
  sourceSeconds: readonly number[],
  options: BrowserFrameSamplerOptions = {},
): Promise<BrowserTrackingFrames> {
  validateSampleTimes(sourceSeconds)
  const safeUrl = validateBrowserTrackingMediaUrl(mediaUrl, options.origin)
  const timeoutMs = options.timeoutMs ?? DEFAULT_BROWSER_TRACKING_TIMEOUT_MS
  if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120_000) {
    throw new Error('Invalid browser tracking timeout')
  }
  const deadline = Date.now() + timeoutMs
  const video = (options.createVideo ?? (() => document.createElement('video')))()
  const canvas = (options.createCanvas ?? (() => document.createElement('canvas')))()
  video.preload = 'auto'
  video.muted = true
  video.playsInline = true

  try {
    const metadata = waitForVideoEvent(video, 'loadedmetadata', options.signal, deadline)
    video.src = safeUrl
    video.load()
    await metadata
    if (!Number.isFinite(video.duration) || video.duration <= 0 || video.videoWidth <= 0 || video.videoHeight <= 0) {
      throw new Error('Tracking source не содержит декодируемого video metadata')
    }
    if (Math.max(...sourceSeconds) >= video.duration) {
      throw new Error('Tracking timestamp выходит за duration source video')
    }
    const size = trackingRasterSize(
      video.videoWidth,
      video.videoHeight,
      options.maxWidth,
      options.maxHeight,
    )
    const totalPixels = size.width * size.height * sourceSeconds.length
    if (!Number.isSafeInteger(totalPixels) || totalPixels > MAX_BROWSER_TRACKING_TOTAL_PIXELS) {
      throw new Error('Browser tracking request exceeds the luma memory budget')
    }
    canvas.width = size.width
    canvas.height = size.height
    const context = canvas.getContext('2d', { alpha: false, willReadFrequently: true })
    if (!context) throw new Error('Canvas 2D недоступен для local tracking')

    const frames: GrayFrame[] = Array(sourceSeconds.length)
    const decodeOrder = sourceSeconds
      .map((seconds, index) => ({ seconds, index }))
      .sort((left, right) => left.seconds - right.seconds)
    for (const { seconds, index } of decodeOrder) {
      await seekVideo(video, seconds, options.signal, deadline)
      context.drawImage(video, 0, 0, size.width, size.height)
      const pixels = context.getImageData(0, 0, size.width, size.height)
      frames[index] = rgbaToLuma(pixels.data, size.width, size.height)
    }
    return {
      frames,
      ...size,
      sampleToSourceScaleX: video.videoWidth / size.width,
      sampleToSourceScaleY: video.videoHeight / size.height,
    }
  } finally {
    video.pause()
    video.removeAttribute('src')
    video.load()
    canvas.width = 0
    canvas.height = 0
  }
}

function validateSampleTimes(values: readonly number[]): void {
  if (values.length < 2 || values.length > 300) throw new Error('Tracking требует 2..=300 timestamps')
  const unique = new Set<number>()
  for (const value of values) {
    if (!Number.isFinite(value) || value < 0 || unique.has(value)) {
      throw new Error('Tracking timestamps должны быть конечными, неотрицательными и уникальными')
    }
    unique.add(value)
  }
}

async function seekVideo(
  video: HTMLVideoElement,
  seconds: number,
  signal: AbortSignal | undefined,
  deadline: number,
): Promise<void> {
  if (signal?.aborted) throw abortError()
  if (video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA) {
    await waitForVideoEvent(video, 'loadeddata', signal, deadline)
  }
  if (Math.abs(video.currentTime - seconds) <= 0.0005) return
  const seeked = waitForVideoEvent(video, 'seeked', signal, deadline)
  video.currentTime = seconds
  await seeked
}

function waitForVideoEvent(
  video: HTMLVideoElement,
  eventName: 'loadedmetadata' | 'loadeddata' | 'seeked',
  signal: AbortSignal | undefined,
  deadline: number,
): Promise<void> {
  return new Promise((resolve, reject) => {
    const remaining = deadline - Date.now()
    if (remaining <= 0) {
      reject(new Error('Browser tracking timed out'))
      return
    }
    const cleanup = (): void => {
      clearTimeout(timer)
      video.removeEventListener(eventName, onReady)
      video.removeEventListener('error', onError)
      signal?.removeEventListener('abort', onAbort)
    }
    const onReady = (): void => { cleanup(); resolve() }
    const onError = (): void => { cleanup(); reject(new Error('Browser не смог декодировать tracking source')) }
    const onAbort = (): void => { cleanup(); reject(abortError()) }
    const timer = window.setTimeout(() => {
      cleanup()
      reject(new Error('Browser tracking timed out'))
    }, remaining)
    video.addEventListener(eventName, onReady, { once: true })
    video.addEventListener('error', onError, { once: true })
    signal?.addEventListener('abort', onAbort, { once: true })
  })
}

function abortError(): DOMException {
  return new DOMException('Browser tracking cancelled', 'AbortError')
}
