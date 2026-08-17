// Client-side filmstrip thumbnails. A detached <video> is seeked to the middle
// of each requested time bucket and painted onto a small canvas; the resulting
// data URL is cached by bucket so scrolling and playhead ticks never trigger a
// re-decode. One seek is in flight at a time, which keeps the main thread free
// while the user drags.

const THUMB_WIDTH = 96
const THUMB_HEIGHT = 54
/** Bounded cache: 400 tiles at this size stay well under a megabyte. */
const MAX_CACHED = 400
/** Give the browser a frame between tiles so dragging never stutters. */
const YIELD_MS = 32

const cache = new Map<string, string>()

interface Worker {
  key: string
  url: string
  video: HTMLVideoElement | null
  queue: number[]
  running: boolean
  broken: boolean
}

let worker: Worker | null = null

function cacheKey(key: string, bucket: number): string {
  return `${key}#${bucket}`
}

/** Bucket index a time falls into, for a given bucket width in seconds. */
export function bucketOf(t: number, bucketSeconds: number): number {
  if (!Number.isFinite(t) || !Number.isFinite(bucketSeconds) || bucketSeconds <= 0) return 0
  return Math.max(0, Math.floor(t / bucketSeconds))
}

export function cachedThumbnail(key: string, bucket: number): string | undefined {
  return cache.get(cacheKey(key, bucket))
}

function store(key: string, bucket: number, url: string): void {
  if (cache.size >= MAX_CACHED) {
    const oldest = cache.keys().next()
    if (!oldest.done) cache.delete(oldest.value)
  }
  cache.set(cacheKey(key, bucket), url)
}

/** Drop the detached element and stop any pending work (source changed). */
export function releaseFilmstrip(): void {
  if (!worker) return
  worker.queue = []
  if (worker.video) worker.video.src = ''
  worker = null
}

function ensureWorker(key: string, url: string): Worker {
  if (worker && worker.key === key && worker.url === url) return worker
  releaseFilmstrip()
  worker = { key, url, video: null, queue: [], running: false, broken: false }
  return worker
}

function once(target: EventTarget, event: string, timeoutMs: number): Promise<boolean> {
  return new Promise((resolve) => {
    let done = false
    const finish = (ok: boolean): void => {
      if (done) return
      done = true
      target.removeEventListener(event, hit)
      target.removeEventListener('error', miss)
      resolve(ok)
    }
    const hit = (): void => finish(true)
    const miss = (): void => finish(false)
    target.addEventListener(event, hit)
    target.addEventListener('error', miss)
    setTimeout(() => finish(false), timeoutMs)
  })
}

async function prepare(active: Worker): Promise<HTMLVideoElement | null> {
  if (active.video) return active.video
  const video = document.createElement('video')
  video.crossOrigin = 'anonymous'
  video.preload = 'auto'
  video.muted = true
  video.playsInline = true
  video.src = active.url
  active.video = video
  const ready = await once(video, 'loadeddata', 8000)
  if (!ready || worker !== active) {
    active.broken = true
    return null
  }
  return video
}

async function grab(video: HTMLVideoElement, t: number): Promise<string | null> {
  try {
    video.currentTime = t
  } catch {
    return null
  }
  if (!(await once(video, 'seeked', 4000))) return null
  const canvas = document.createElement('canvas')
  canvas.width = THUMB_WIDTH
  canvas.height = THUMB_HEIGHT
  const context = canvas.getContext('2d')
  if (!context) return null
  try {
    context.drawImage(video, 0, 0, THUMB_WIDTH, THUMB_HEIGHT)
    return canvas.toDataURL('image/jpeg', 0.5)
  } catch {
    // A cross-origin source taints the canvas: give up on thumbnails for it.
    return null
  }
}

async function drain(active: Worker, bucketSeconds: number, onReady: () => void): Promise<void> {
  if (active.running) return
  active.running = true
  try {
    const video = await prepare(active)
    if (!video) return
    while (active.queue.length && worker === active) {
      const bucket = active.queue.shift()
      if (bucket === undefined || cachedThumbnail(active.key, bucket)) continue
      const url = await grab(video, (bucket + 0.5) * bucketSeconds)
      if (worker !== active) return
      if (!url) {
        active.broken = true
        return
      }
      store(active.key, bucket, url)
      onReady()
      await new Promise((resolve) => setTimeout(resolve, YIELD_MS))
    }
  } finally {
    active.running = false
  }
}

/**
 * Queue the buckets that are not cached yet. Safe to call on every scroll or
 * zoom change: already cached and already queued buckets are skipped, so the
 * cost of a repeated call is a short array scan.
 */
export function requestThumbnails(
  key: string,
  url: string,
  buckets: readonly number[],
  bucketSeconds: number,
  onReady: () => void,
): void {
  if (!key || !url || !Number.isFinite(bucketSeconds) || bucketSeconds <= 0) return
  if (typeof document === 'undefined') return
  const active = ensureWorker(key, url)
  if (active.broken) return
  for (const bucket of buckets) {
    if (!Number.isInteger(bucket) || bucket < 0) continue
    if (cachedThumbnail(key, bucket) || active.queue.includes(bucket)) continue
    active.queue.push(bucket)
  }
  if (active.queue.length) void drain(active, bucketSeconds, onReady)
}
