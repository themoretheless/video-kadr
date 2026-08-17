// Throttled grabber for the current player frame, shared by the scopes and the
// before/after wipe.
//
// The <video> element belongs to VideoPreview.vue, which this feature does not
// own, so it is found through its stable `.player` class instead of a shared
// store handle. See the integration note in the feature report: exposing the
// element (or an ImageBitmap) from the store would be the cleaner seam.
//
// The loop is an interval, not requestAnimationFrame: scopes only have to track
// the eye, and re-reading a frame 60 times a second would burn the main thread
// for nothing. It stops entirely while the panel is collapsed (the caller's
// `active` flag) or the tab is hidden.

import { onBeforeUnmount, ref, shallowRef, watch, type Ref, type ShallowRef } from 'vue'
import { sampleDimensions, type FrameSample } from '../../domain/scopes'

/** Default sampling period, milliseconds. */
const INTERVAL_MS = 200
/** Widest sample the scopes work on. 320px is ~57k pixels, cheap to walk. */
const SAMPLE_WIDTH = 320

export function playerElement(): HTMLVideoElement | null {
  if (typeof document === 'undefined') return null
  const element = document.querySelector('video.player')
  return element instanceof HTMLVideoElement ? element : null
}

/** True once the element has decoded at least one frame we can copy. */
export function playerHasFrame(video: HTMLVideoElement | null): video is HTMLVideoElement {
  return Boolean(video && video.readyState >= 2 && video.videoWidth > 0 && video.videoHeight > 0)
}

export interface FrameGrabber {
  /** Latest reduced-resolution sample, or null while nothing could be read. */
  frame: ShallowRef<FrameSample | null>
  /** Bumped on every successful grab, for consumers that redraw themselves. */
  tick: Ref<number>
  /** Russian, user-facing; empty while everything is fine. */
  error: Ref<string>
}

export function useFrameGrabber(active: Ref<boolean>, intervalMs = INTERVAL_MS): FrameGrabber {
  const frame = shallowRef<FrameSample | null>(null)
  const tick = ref(0)
  const error = ref('')
  let canvas: HTMLCanvasElement | null = null
  let timer: ReturnType<typeof setInterval> | null = null

  function grab(): void {
    const video = playerElement()
    if (!playerHasFrame(video)) return
    const size = sampleDimensions(video.videoWidth, video.videoHeight, SAMPLE_WIDTH)
    if (!canvas) canvas = document.createElement('canvas')
    canvas.width = size.width
    canvas.height = size.height
    const context = canvas.getContext('2d', { willReadFrequently: true })
    if (!context) {
      error.value = 'Браузер не дал доступ к canvas, осциллограммы недоступны'
      stop()
      return
    }
    try {
      context.drawImage(video, 0, 0, size.width, size.height)
      const image = context.getImageData(0, 0, size.width, size.height)
      frame.value = { width: size.width, height: size.height, data: image.data }
      tick.value++
      error.value = ''
    } catch {
      // A cross-origin source taints the canvas and getImageData throws. There
      // is nothing to retry, so stop the loop instead of spinning on it.
      error.value = 'Кадр недоступен для чтения: источник из другого домена'
      stop()
    }
  }

  function running(): boolean {
    return active.value && typeof document !== 'undefined' && !document.hidden
  }

  function start(): void {
    if (timer || !running()) return
    grab()
    timer = setInterval(grab, Math.max(50, intervalMs))
  }

  function stop(): void {
    if (!timer) return
    clearInterval(timer)
    timer = null
  }

  function sync(): void {
    if (running()) start()
    else stop()
  }

  watch(active, sync, { immediate: true })
  if (typeof document !== 'undefined') {
    document.addEventListener('visibilitychange', sync)
  }

  onBeforeUnmount(() => {
    stop()
    canvas = null
    if (typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', sync)
    }
  })

  return { frame, tick, error }
}
