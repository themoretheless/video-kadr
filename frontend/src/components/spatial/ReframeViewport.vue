<script setup lang="ts">
// Drag-to-look viewport over the 360 source. Horizontal drag is yaw, vertical
// drag is pitch, wheel or pinch is field of view, exactly like Insta360 Studio.
//
// The picture is a real reprojection: an equirectangular source is resampled by
// the WebGL sampler with the same projection maths the render stage asks `v360`
// for. When that is not possible (a fisheye source, or a browser without WebGL)
// the viewport degrades to an honest wireframe: the flattened source frame with
// the view cone drawn over it, clearly labelled as an indicator rather than a
// preview. It never shows a picture the render would not produce.

import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { beginEditTransaction, endEditTransaction, state } from '../../store'
import { fovRange, setView, spatialState } from '../../store/spatial'
import type { EquirectSampler } from './equirectSampler'

// Two canvases, because a canvas that has handed out a WebGL context can never
// hand out a 2D one: the sampler owns the first, the wireframe the second, and
// only one of them is visible at a time.
const glCanvas = ref<HTMLCanvasElement | null>(null)
const flatCanvas = ref<HTMLCanvasElement | null>(null)
const frameEl = ref<HTMLElement | null>(null)
const videoEl = ref<HTMLVideoElement | null>(null)

/** null while the sampler has not been asked for yet. */
const samplerReady = ref<boolean | null>(null)
let sampler: EquirectSampler | null = null
let context2d: CanvasRenderingContext2D | null = null
let frameHandle = 0

const reframe = computed(() => spatialState.reframe360)
/**
 * The live camera. The panel keeps it in step with the keyframe tracks as the
 * playhead moves, so dragging here always moves what is on screen and never
 * silently fights an animated track.
 */
const view = computed(() => {
  const camera = reframe.value.view
  // Horizon lock cancels roll in the render stage, so the viewport shows level.
  return reframe.value.horizonLock ? { ...camera, roll: 0 } : camera
})
/** The sampler only claims to be correct for an equirectangular source. */
const canSample = computed(() => reframe.value.inputProjection === 'equirect')
const isPreview = computed(() => canSample.value && samplerReady.value === true)

const aspect = computed(() => {
  const { outputWidth, outputHeight } = reframe.value
  return outputHeight > 0 ? outputWidth / outputHeight : 16 / 9
})

const fallbackReason = computed(() => {
  if (isPreview.value) return ''
  if (!canSample.value) {
    return 'Для fisheye-источника предпросмотр перекадрирования недоступен: показан исходный кадр без разворота. Углы и поле зрения применятся только при экспорте.'
  }
  if (samplerReady.value === false) {
    return 'WebGL недоступен: показан развёрнутый кадр источника, рамка отмечает выбранный сектор. Это индикатор, а не предпросмотр.'
  }
  return ''
})

// --- source frame ---
// The panel keeps its own muted <video> on the same source. It follows the main
// player's position instead of driving it, so the two never fight over seeks.

let lastPlayerTime = 0
let lastPlayerChange = 0

watch(
  () => state.playerTime,
  (time) => {
    if (time === lastPlayerTime) return
    lastPlayerTime = time
    lastPlayerChange = performance.now()
  },
)

watch(
  () => state.video?.url,
  () => {
    videoEl.value?.load()
  },
)

function syncSource(): void {
  const video = videoEl.value
  if (!video || !video.src) return
  const playing = performance.now() - lastPlayerChange < 600
  if (Math.abs(video.currentTime - state.playerTime) > 0.35) {
    try {
      video.currentTime = Math.max(0, state.playerTime)
    } catch {
      // A seek before metadata arrives throws; the next frame retries.
    }
  }
  if (playing && video.paused) void video.play().catch(() => undefined)
  if (!playing && !video.paused) video.pause()
}

// --- drawing ---

function resizeCanvas(canvas: HTMLCanvasElement): void {
  const ratio = Math.min(2, window.devicePixelRatio || 1)
  const width = Math.max(1, Math.round(canvas.clientWidth * ratio))
  const height = Math.max(1, Math.round(canvas.clientHeight * ratio))
  if (canvas.width !== width) canvas.width = width
  if (canvas.height !== height) canvas.height = height
}

function drawWireframe(): void {
  const canvas = flatCanvas.value
  const video = videoEl.value
  if (!canvas) return
  resizeCanvas(canvas)
  context2d ??= canvas.getContext('2d')
  const ctx = context2d
  if (!ctx) return
  const { width, height } = canvas
  ctx.clearRect(0, 0, width, height)
  if (video && video.readyState >= 2) {
    ctx.drawImage(video, 0, 0, width, height)
  }
  if (!canSample.value) return

  // View cone over a flattened equirectangular frame: longitude across, latitude
  // down. It marks the framed sector, it does not resample it.
  const current = view.value
  const verticalFov = Math.max(1, (current.fov * reframe.value.outputHeight) / reframe.value.outputWidth)
  const boxWidth = (Math.min(360, current.fov) / 360) * width
  const boxHeight = (Math.min(180, verticalFov) / 180) * height
  const centerX = (current.yaw / 360 + 0.5) * width
  const centerY = (0.5 - current.pitch / 180) * height
  ctx.lineWidth = Math.max(2, width / 320)
  ctx.strokeStyle = '#4f8cff'
  ctx.setLineDash([ctx.lineWidth * 3, ctx.lineWidth * 2])
  for (const offset of [-width, 0, width]) {
    ctx.strokeRect(
      centerX - boxWidth / 2 + offset,
      centerY - boxHeight / 2,
      boxWidth,
      boxHeight,
    )
  }
  ctx.setLineDash([])
}

function drawFrame(): void {
  frameHandle = requestAnimationFrame(drawFrame)
  const frame = frameEl.value
  // Nothing to draw while the collapsible section is closed.
  if (!frame || !frame.isConnected || frame.offsetParent === null) return
  syncSource()
  const video = videoEl.value
  const canvas = glCanvas.value
  if (isPreview.value && sampler && canvas && video && video.readyState >= 2) {
    resizeCanvas(canvas)
    sampler.draw(video, { ...view.value, projection: reframe.value.outputProjection })
    return
  }
  drawWireframe()
}

async function initSampler(): Promise<void> {
  if (sampler || samplerReady.value !== null) return
  const canvas = glCanvas.value
  if (!canvas) return
  try {
    const module = await import('./equirectSampler')
    sampler = module.createEquirectSampler(canvas)
  } catch {
    sampler = null
  }
  samplerReady.value = sampler !== null
}

onMounted(() => {
  // The sampler is only built for the source it is honest about.
  if (canSample.value) void initSampler()
  frameHandle = requestAnimationFrame(drawFrame)
})

// Switching back to an equirectangular source builds the sampler on first need.
watch(canSample, (usable) => {
  if (usable) void initSampler()
})

onBeforeUnmount(() => {
  cancelAnimationFrame(frameHandle)
  sampler?.dispose()
  sampler = null
})

// --- pointer / wheel / keyboard input ---

interface DragOrigin {
  x: number
  y: number
  yaw: number
  pitch: number
}

const pointers = new Map<number, { x: number; y: number }>()
let drag: DragOrigin | null = null
let pinchDistance = 0
let pinchFov = 0
let wheelTimer: ReturnType<typeof setTimeout> | null = null
const dragging = ref(false)

function canvasSize(): { width: number; height: number } {
  const frame = frameEl.value
  return {
    width: Math.max(1, frame?.clientWidth ?? 1),
    height: Math.max(1, frame?.clientHeight ?? 1),
  }
}

function onPointerDown(event: PointerEvent): void {
  const frame = frameEl.value
  if (!frame) return
  frame.setPointerCapture(event.pointerId)
  pointers.set(event.pointerId, { x: event.clientX, y: event.clientY })
  if (pointers.size === 1) {
    beginEditTransaction('reframe-look')
    dragging.value = true
    drag = { x: event.clientX, y: event.clientY, yaw: view.value.yaw, pitch: view.value.pitch }
  } else if (pointers.size === 2) {
    drag = null
    pinchDistance = pointerDistance()
    pinchFov = view.value.fov
  }
  event.preventDefault()
}

function pointerDistance(): number {
  const [first, second] = [...pointers.values()]
  if (!first || !second) return 0
  return Math.hypot(first.x - second.x, first.y - second.y)
}

function onPointerMove(event: PointerEvent): void {
  if (!pointers.has(event.pointerId)) return
  pointers.set(event.pointerId, { x: event.clientX, y: event.clientY })

  if (pointers.size >= 2) {
    const distance = pointerDistance()
    if (pinchDistance > 0 && distance > 0) setView({ fov: pinchFov * (pinchDistance / distance) })
    return
  }
  if (!drag) return
  const { width, height } = canvasSize()
  const current = view.value
  const verticalFov = (current.fov * reframe.value.outputHeight) / reframe.value.outputWidth
  // Grab-and-pull: the frame follows the finger, so the camera moves the other
  // way. One canvas width is exactly one field of view, like Insta360 Studio.
  setView({
    yaw: drag.yaw - ((event.clientX - drag.x) / width) * current.fov,
    pitch: drag.pitch + ((event.clientY - drag.y) / height) * verticalFov,
  })
}

function onPointerUp(event: PointerEvent): void {
  if (!pointers.delete(event.pointerId)) return
  frameEl.value?.releasePointerCapture?.(event.pointerId)
  if (pointers.size === 0) {
    drag = null
    dragging.value = false
    endEditTransaction()
  }
}

function onWheel(event: WheelEvent): void {
  event.preventDefault()
  if (!wheelTimer) beginEditTransaction('reframe-fov')
  else clearTimeout(wheelTimer)
  wheelTimer = setTimeout(() => {
    wheelTimer = null
    endEditTransaction()
  }, 300)
  setView({ fov: view.value.fov * Math.exp(event.deltaY * 0.0015) })
}

/** Keyboard equivalent of the drag, so the viewport is not mouse-only. */
function onKeydown(event: KeyboardEvent): void {
  const step = event.shiftKey ? 10 : 2
  const current = view.value
  const moves: Record<string, () => void> = {
    ArrowLeft: () => setView({ yaw: current.yaw - step }),
    ArrowRight: () => setView({ yaw: current.yaw + step }),
    ArrowUp: () => setView({ pitch: current.pitch + step }),
    ArrowDown: () => setView({ pitch: current.pitch - step }),
    '+': () => setView({ fov: current.fov - step }),
    '-': () => setView({ fov: current.fov + step }),
  }
  const move = moves[event.key]
  if (!move) return
  event.preventDefault()
  beginEditTransaction('reframe-look')
  move()
  endEditTransaction()
}

const fovLimits = computed(() => fovRange(reframe.value.outputProjection))
const viewLabel = computed(() => {
  const current = view.value
  return `Рыскание ${Math.round(current.yaw)}°, тангаж ${Math.round(current.pitch)}°, поле зрения ${Math.round(current.fov)}°`
})
</script>

<template>
  <div class="reframe-viewport">
    <div
      ref="frameEl"
      class="reframe-frame"
      :class="{ 'is-dragging': dragging }"
      :style="{ aspectRatio: String(aspect) }"
      tabindex="0"
      role="application"
      :aria-label="`Обзор 360. ${viewLabel}. Стрелки поворачивают камеру, плюс и минус меняют поле зрения.`"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @wheel="onWheel"
      @keydown="onKeydown"
    >
      <canvas v-show="isPreview" ref="glCanvas" class="reframe-canvas"></canvas>
      <canvas v-show="!isPreview" ref="flatCanvas" class="reframe-canvas"></canvas>
    </div>
    <video
      ref="videoEl"
      class="reframe-source"
      :src="state.video?.url"
      muted
      playsinline
      preload="auto"
      aria-hidden="true"
    ></video>
    <div class="reframe-readout">
      <span>{{ viewLabel }}</span>
      <span class="reframe-fov-hint">
        поле зрения {{ fovLimits[0] }}–{{ fovLimits[1] }}°, колесо или щипок
      </span>
    </div>
    <p v-if="fallbackReason" class="hint reframe-warning" role="status">{{ fallbackReason }}</p>
  </div>
</template>

<style scoped>
.reframe-viewport {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.reframe-frame {
  position: relative;
  width: 100%;
  max-height: 320px;
  background: var(--panel-3);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  overflow: hidden;
  cursor: grab;
  touch-action: none;
}

.reframe-frame.is-dragging {
  cursor: grabbing;
}

.reframe-frame:focus-visible {
  outline: none;
  box-shadow: var(--ring);
}

.reframe-canvas {
  display: block;
  width: 100%;
  height: 100%;
}

/* The source frame is only a texture; it is never shown directly. */
.reframe-source {
  position: absolute;
  width: 1px;
  height: 1px;
  opacity: 0;
  pointer-events: none;
}

.reframe-readout {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  justify-content: space-between;
  color: var(--muted);
  font-size: 12px;
}

.reframe-fov-hint {
  color: var(--faint);
}

.reframe-warning {
  margin: 0;
}
</style>
