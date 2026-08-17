<script setup lang="ts">
// Ken Burns stage: the source frame with a draggable start and end rectangle,
// and a live view of the transform sampled at the playhead.
//
// The stage element carries the source aspect ratio, so it *is* the content box
// and every rectangle is already in normalized frame coordinates. That is why
// `domain/contentBox.ts` is not used here: there is no letterbox to correct for,
// and its mapping would resolve to the identity on every frame. The one case
// that could break the assumption, a library entry with no stored dimensions, is
// closed by reading the real frame size back off the element.

import { computed, onUnmounted, ref, watch } from 'vue'
import {
  clamp,
  MAX_ZOOM,
  previewTransform,
  windowFromZoomPan,
  zoomPanFromWindow,
  type FrameWindow,
  type MotionSample,
} from '../../domain/keyframes'

const props = defineProps<{
  src: string
  width: number
  height: number
  /** Position inside the source file the preview frame should show. */
  time: number
  /** Transform sampled at the playhead, drawn as the live result. */
  current: MotionSample
  start: MotionSample
  end: MotionSample
  /** Live result instead of the framing rectangles. */
  showResult: boolean
}>()

const emit = defineEmits<{
  'update:start': [MotionSample]
  'update:end': [MotionSample]
  'interaction-start': []
  'interaction-end': []
}>()

type Handle = 'start' | 'end'

const stage = ref<HTMLElement | null>(null)
const video = ref<HTMLVideoElement | null>(null)

let drag: {
  pointerId: number
  handle: Handle
  resize: boolean
  originX: number
  originY: number
  frame: FrameWindow
} | null = null

/** Filled from the element when the library entry carries no dimensions. */
const measured = ref<{ width: number; height: number } | null>(null)

const ratio = computed(() => {
  const source =
    props.width > 0 && props.height > 0
      ? { width: props.width, height: props.height }
      : measured.value
  return source ? `${source.width} / ${source.height}` : '16 / 9'
})

function onMetadata(): void {
  const element = video.value
  if (!element || !element.videoWidth || !element.videoHeight) return
  measured.value = { width: element.videoWidth, height: element.videoHeight }
}

const startWindow = computed(() => windowOf(props.start))
const endWindow = computed(() => windowOf(props.end))
const frameStyle = computed(() => ({ transform: previewTransform(props.current) }))

function windowOf(sample: MotionSample): FrameWindow {
  return windowFromZoomPan(sample.zoom, sample.panX, sample.panY)
}

function boxStyle(frame: FrameWindow): Record<string, string> {
  return {
    left: `${frame.x * 100}%`,
    top: `${frame.y * 100}%`,
    width: `${frame.size * 100}%`,
    height: `${frame.size * 100}%`,
  }
}

/** Keep the preview frame on the playhead without owning playback. */
watch(
  () => [props.time, props.src] as const,
  ([time]) => {
    const element = video.value
    if (!element || !Number.isFinite(time)) return
    if (Math.abs(element.currentTime - time) < 0.04) return
    try {
      element.currentTime = Math.max(0, time)
    } catch {
      // Seeking before metadata arrives throws in some browsers; the next
      // playhead change retries once the element is ready.
    }
  },
  { immediate: true },
)

function begin(handle: Handle, resize: boolean, event: PointerEvent): void {
  if (event.button !== 0) return
  drag = {
    pointerId: event.pointerId,
    handle,
    resize,
    originX: event.clientX,
    originY: event.clientY,
    frame: handle === 'start' ? startWindow.value : endWindow.value,
  }
  emit('interaction-start')
  ;(event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId)
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', stop)
  window.addEventListener('pointercancel', stop)
  event.preventDefault()
  event.stopPropagation()
}

function onMove(event: PointerEvent): void {
  const current = drag
  const element = stage.value
  if (!current || !element || event.pointerId !== current.pointerId) return
  const rect = element.getBoundingClientRect()
  if (rect.width <= 0 || rect.height <= 0) return

  const dx = (event.clientX - current.originX) / rect.width
  const dy = (event.clientY - current.originY) / rect.height
  const source = current.frame
  let next: FrameWindow
  if (current.resize) {
    // Resize from the centre so the framing does not creep while zooming.
    const size = clamp(source.size + (dx + dy) / 2, 1 / MAX_ZOOM, 1)
    const shift = (size - source.size) / 2
    next = { x: source.x - shift, y: source.y - shift, size }
  } else {
    next = { x: source.x + dx, y: source.y + dy, size: source.size }
  }
  next.x = clamp(next.x, 0, 1 - next.size)
  next.y = clamp(next.y, 0, 1 - next.size)

  const previous = current.handle === 'start' ? props.start : props.end
  const sample = { ...zoomPanFromWindow(next), rotation: previous.rotation }
  if (current.handle === 'start') emit('update:start', sample)
  else emit('update:end', sample)
}

function stop(): void {
  if (!drag) return
  drag = null
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', stop)
  window.removeEventListener('pointercancel', stop)
  emit('interaction-end')
}

onUnmounted(stop)
</script>

<template>
  <div ref="stage" class="kb-stage" :style="{ aspectRatio: ratio }">
    <video
      ref="video"
      class="kb-frame"
      :class="{ 'is-result': showResult }"
      :style="showResult ? frameStyle : undefined"
      :src="src"
      muted
      playsinline
      preload="metadata"
      @loadedmetadata="onMetadata"
    ></video>

    <template v-if="!showResult">
      <div class="kb-box is-current" :style="boxStyle(windowOf(current))" aria-hidden="true"></div>
      <div
        class="kb-box is-start"
        :style="boxStyle(startWindow)"
        role="group"
        aria-label="Начальный кадр"
        @pointerdown="begin('start', false, $event)"
      >
        <span class="kb-tag">Старт</span>
        <span class="kb-grip" @pointerdown="begin('start', true, $event)"></span>
      </div>
      <div
        class="kb-box is-end"
        :style="boxStyle(endWindow)"
        role="group"
        aria-label="Конечный кадр"
        @pointerdown="begin('end', false, $event)"
      >
        <span class="kb-tag">Финиш</span>
        <span class="kb-grip" @pointerdown="begin('end', true, $event)"></span>
      </div>
    </template>
  </div>
</template>

<style scoped>
.kb-stage {
  position: relative;
  width: 100%;
  overflow: hidden;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: #000;
  touch-action: none;
  user-select: none;
}

.kb-frame {
  display: block;
  width: 100%;
  height: 100%;
  object-fit: fill;
}

.kb-frame.is-result {
  transform-origin: 50% 50%;
}

.kb-box {
  position: absolute;
  border: 1.5px solid;
  border-radius: 2px;
}

.kb-box.is-current {
  border-style: solid;
  border-color: rgba(255, 255, 255, 0.75);
  pointer-events: none;
}

.kb-box.is-start {
  border-style: dashed;
  border-color: var(--ok);
  cursor: move;
}

.kb-box.is-end {
  border-style: dashed;
  border-color: var(--accent);
  cursor: move;
}

.kb-tag {
  position: absolute;
  top: -2px;
  left: 2px;
  padding: 0 3px;
  border-radius: 2px;
  background: rgba(0, 0, 0, 0.6);
  color: #fff;
  font-size: 0.62rem;
  line-height: 1.5;
  pointer-events: none;
}

.kb-grip {
  position: absolute;
  right: -6px;
  bottom: -6px;
  width: 13px;
  height: 13px;
  border: 1.5px solid currentColor;
  border-radius: 3px;
  background: var(--panel);
  color: inherit;
  cursor: nwse-resize;
}

.is-start .kb-grip {
  border-color: var(--ok);
}

.is-end .kb-grip {
  border-color: var(--accent);
}
</style>
