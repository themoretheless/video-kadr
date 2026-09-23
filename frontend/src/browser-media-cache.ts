import { computePeakBuckets, type PeakBuckets } from './domain/timeline-media'

export interface AudioPeakCacheEntry {
  peaks: PeakBuckets
  durationSeconds: number
}

const peaksCache = new Map<string, Promise<AudioPeakCacheEntry>>()

export const WAVEFORM_BUCKET_COUNT = 512

export function getAudioPeaks(url: string): Promise<AudioPeakCacheEntry> {
  let entry = peaksCache.get(url)
  if (!entry) {
    entry = decodeAudioPeaks(url).catch((error: unknown) => {
      peaksCache.delete(url)
      throw error
    })
    peaksCache.set(url, entry)
  }
  return entry
}

async function decodeAudioPeaks(url: string): Promise<AudioPeakCacheEntry> {
  const response = await fetch(url)
  if (!response.ok) throw new Error(`не удалось загрузить аудио (${response.status})`)
  const data = await response.arrayBuffer()
  const AudioContextClass = window.AudioContext
    ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext
  if (!AudioContextClass) throw new Error('Web Audio API недоступна')
  const context = new AudioContextClass()
  try {
    const buffer = await context.decodeAudioData(data)
    return {
      peaks: computePeakBuckets(buffer.getChannelData(0), WAVEFORM_BUCKET_COUNT),
      durationSeconds: buffer.duration,
    }
  } finally {
    void context.close().catch(() => undefined)
  }
}

interface StripEntry {
  promise: Promise<HTMLCanvasElement[]>
  width: number
  height: number
}

const stripCache = new Map<string, StripEntry>()

/**
 * Seek a shared muted video element through evenly spaced times and paint each
 * frame into its own small canvas. Results are cached per URL and consumed
 * sequentially so a single element serves every clip strip.
 */
export function getThumbnailStrip(
  url: string,
  timesSeconds: number[],
  width = 96,
  height = 54,
): Promise<HTMLCanvasElement[]> {
  const key = `${url}@${width}x${height}:${timesSeconds.map((value) => value.toFixed(2)).join(',')}`
  const cached = stripCache.get(key)
  if (cached && cached.width === width && cached.height === height) return cached.promise
  const promise = renderThumbnailStrip(url, timesSeconds, width, height)
  stripCache.set(key, { promise, width, height })
  promise.catch(() => stripCache.delete(key))
  return promise
}

async function renderThumbnailStrip(
  url: string,
  timesSeconds: number[],
  width: number,
  height: number,
): Promise<HTMLCanvasElement[]> {
  const video = document.createElement('video')
  video.muted = true
  video.playsInline = true
  video.preload = 'auto'
  video.crossOrigin = 'anonymous'
  video.src = url
  try {
    await waitFor(video, 'loadedmetadata', 10_000)
    const frames: HTMLCanvasElement[] = []
    for (const time of timesSeconds) {
      if (video.seekable.length === 0) break
      const clamped = Math.max(0, Math.min(time, video.duration - 0.05))
      if (Math.abs(video.currentTime - clamped) > 0.01) {
        const seeked = waitFor(video, 'seeked', 10_000)
        video.currentTime = clamped
        await seeked
      }
      const canvas = document.createElement('canvas')
      canvas.width = width
      canvas.height = height
      const context = canvas.getContext('2d')
      if (!context) break
      const scale = Math.max(width / (video.videoWidth || width), height / (video.videoHeight || height))
      const drawWidth = width / scale
      const drawHeight = height / scale
      context.drawImage(
        video,
        (video.videoWidth - drawWidth) / 2,
        (video.videoHeight - drawHeight) / 2,
        drawWidth,
        drawHeight,
        0,
        0,
        width,
        height,
      )
      frames.push(canvas)
    }
    return frames
  } finally {
    video.pause()
    video.removeAttribute('src')
    video.load()
  }
}

function waitFor(target: EventTarget, event: string, timeoutMs: number): Promise<void> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      target.removeEventListener(event, onEvent)
      reject(new Error(`ожидание ${event} истекло`))
    }, timeoutMs)
    const onEvent = () => {
      clearTimeout(timer)
      target.removeEventListener(event, onEvent)
      resolve()
    }
    const onError = () => {
      clearTimeout(timer)
      target.removeEventListener(event, onEvent)
      reject(new Error('ошибка загрузки медиа'))
    }
    target.addEventListener(event, onEvent, { once: true })
    if ('addEventListener' in target) {
      (target as HTMLVideoElement).addEventListener('error', onError, { once: true })
    }
  })
}
