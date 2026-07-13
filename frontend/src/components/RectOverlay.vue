<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'
import {
  clampRect,
  point,
  rect as geometryRect,
  Transform2D,
  type NormalizedSpace,
  type PreviewSpace,
  type SourceSpace,
} from '../domain/geometry'
import { state } from '../store'

interface OverlayRect {
  x: number
  y: number
  w: number
  h: number
}

const props = withDefaults(
  defineProps<{
    rect: OverlayRect
    color?: string
    // crop dims the area outside; mask fills the rectangle instead.
    mode?: 'crop' | 'mask'
  }>(),
  { color: 'var(--accent)', mode: 'crop' },
)

const emit = defineEmits<{
  'update:rect': [OverlayRect]
  'interaction-start': []
  'interaction-end': []
}>()

const root = ref<HTMLElement | null>(null)
const MIN = 16

type Mode = 'move' | 'nw' | 'ne' | 'sw' | 'se'
let dragMode: Mode | null = null
let startX = 0
let startY = 0
let orig: OverlayRect = { x: 0, y: 0, w: 0, h: 0 }

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(v, hi))
}

function dims() {
  return { W: state.video?.width || 1, H: state.video?.height || 1 }
}

function normalizeRect(r: OverlayRect): OverlayRect {
  const { W, H } = dims()
  const minW = Math.min(MIN, W)
  const minH = Math.min(MIN, H)
  const finite = (value: number, fallback: number) => (Number.isFinite(value) ? value : fallback)
  const candidate = geometryRect<SourceSpace>(
    Math.round(finite(r.x, 0)),
    Math.round(finite(r.y, 0)),
    Math.max(0, Math.round(finite(r.w, minW))),
    Math.max(0, Math.round(finite(r.h, minH))),
  )
  const normalized = clampRect(
    candidate,
    geometryRect<SourceSpace>(0, 0, W, H),
    [minW, minH],
  )
  return {
    x: normalized.x,
    y: normalized.y,
    w: normalized.width,
    h: normalized.height,
  }
}

function emitRect(r: OverlayRect) {
  emit('update:rect', normalizeRect(r))
}

const rectStyle = computed(() => {
  const { W, H } = dims()
  const c = normalizeRect(props.rect)
  const normalized = Transform2D.scale<SourceSpace, NormalizedSpace>(1 / W, 1 / H).applyRect(
    geometryRect<SourceSpace>(c.x, c.y, c.w, c.h),
  )
  return {
    left: `${normalized.x * 100}%`,
    top: `${normalized.y * 100}%`,
    width: `${normalized.width * 100}%`,
    height: `${normalized.height * 100}%`,
    borderColor: props.color,
    boxShadow: props.mode === 'crop' ? '0 0 0 9999px rgba(0, 0, 0, 0.45)' : 'none',
    background: props.mode === 'mask' ? 'rgba(255, 80, 80, 0.32)' : 'transparent',
  }
})

function toSrc(dxPx: number, dyPx: number) {
  const element = root.value
  if (!element) return null
  const r = element.getBoundingClientRect()
  if (r.width <= 0 || r.height <= 0) return null
  const { W, H } = dims()
  const sourceToPreview = Transform2D.scale<SourceSpace, PreviewSpace>(r.width / W, r.height / H)
  const delta = sourceToPreview.inverse().applyVector(point<PreviewSpace>(dxPx, dyPx))
  return { dx: delta.x, dy: delta.y }
}

function begin(m: Mode, e: PointerEvent) {
  dragMode = m
  startX = e.clientX
  startY = e.clientY
  orig = normalizeRect(props.rect)
  emit('interaction-start')
  ;(e.target as HTMLElement).setPointerCapture?.(e.pointerId)
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  e.preventDefault()
  e.stopPropagation()
}

function onMove(e: PointerEvent) {
  if (!dragMode) return
  const delta = toSrc(e.clientX - startX, e.clientY - startY)
  if (!delta) {
    stopDrag()
    return
  }
  const { dx, dy } = delta
  const { W, H } = dims()
  const minW = Math.min(MIN, W)
  const minH = Math.min(MIN, H)
  if (dragMode === 'move') {
    const x = clamp(orig.x + dx, 0, W - orig.w)
    const y = clamp(orig.y + dy, 0, H - orig.h)
    emitRect({ x: Math.round(x), y: Math.round(y), w: orig.w, h: orig.h })
    return
  }
  let x1 = orig.x
  let y1 = orig.y
  let x2 = orig.x + orig.w
  let y2 = orig.y + orig.h
  if (dragMode.includes('w')) x1 = clamp(orig.x + dx, 0, x2 - minW)
  if (dragMode.includes('e')) x2 = clamp(orig.x + orig.w + dx, x1 + minW, W)
  if (dragMode.includes('n')) y1 = clamp(orig.y + dy, 0, y2 - minH)
  if (dragMode.includes('s')) y2 = clamp(orig.y + orig.h + dy, y1 + minH, H)
  emitRect({
    x: Math.round(x1),
    y: Math.round(y1),
    w: Math.round(x2 - x1),
    h: Math.round(y2 - y1),
  })
}

function onUp() {
  stopDrag()
}

function stopDrag() {
  const wasDragging = dragMode !== null
  dragMode = null
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
  if (wasDragging) emit('interaction-end')
}

onUnmounted(stopDrag)
</script>

<template>
  <div ref="root" class="crop-overlay">
    <div class="crop-rect" :style="rectStyle" @pointerdown="begin('move', $event)">
      <span class="crop-handle nw" :style="{ borderColor: color }" @pointerdown="begin('nw', $event)"></span>
      <span class="crop-handle ne" :style="{ borderColor: color }" @pointerdown="begin('ne', $event)"></span>
      <span class="crop-handle sw" :style="{ borderColor: color }" @pointerdown="begin('sw', $event)"></span>
      <span class="crop-handle se" :style="{ borderColor: color }" @pointerdown="begin('se', $event)"></span>
    </div>
  </div>
</template>
