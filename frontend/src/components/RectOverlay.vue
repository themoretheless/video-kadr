<script setup lang="ts">
import { computed, ref } from 'vue'
import { state } from '../store'

interface Rect {
  x: number
  y: number
  w: number
  h: number
}

const props = withDefaults(
  defineProps<{
    rect: Rect
    color?: string
    // crop dims the area outside; mask fills the rectangle instead.
    mode?: 'crop' | 'mask'
  }>(),
  { color: 'var(--accent)', mode: 'crop' },
)

const emit = defineEmits<{ 'update:rect': [Rect] }>()

const root = ref<HTMLElement | null>(null)
const MIN = 16

type Mode = 'move' | 'nw' | 'ne' | 'sw' | 'se'
let dragMode: Mode | null = null
let startX = 0
let startY = 0
let orig: Rect = { x: 0, y: 0, w: 0, h: 0 }

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(v, hi))
}

function dims() {
  return { W: state.video?.width || 1, H: state.video?.height || 1 }
}

function normalizeRect(r: Rect): Rect {
  const { W, H } = dims()
  const minW = Math.min(MIN, W)
  const minH = Math.min(MIN, H)
  const w = clamp(Math.round(r.w || minW), minW, W)
  const h = clamp(Math.round(r.h || minH), minH, H)
  return {
    x: clamp(Math.round(r.x || 0), 0, W - w),
    y: clamp(Math.round(r.y || 0), 0, H - h),
    w,
    h,
  }
}

function emitRect(r: Rect) {
  emit('update:rect', normalizeRect(r))
}

const rectStyle = computed(() => {
  const { W, H } = dims()
  const c = normalizeRect(props.rect)
  return {
    left: `${(c.x / W) * 100}%`,
    top: `${(c.y / H) * 100}%`,
    width: `${(c.w / W) * 100}%`,
    height: `${(c.h / H) * 100}%`,
    borderColor: props.color,
    boxShadow: props.mode === 'crop' ? '0 0 0 9999px rgba(0, 0, 0, 0.45)' : 'none',
    background: props.mode === 'mask' ? 'rgba(255, 80, 80, 0.32)' : 'transparent',
  }
})

function toSrc(dxPx: number, dyPx: number) {
  const r = root.value!.getBoundingClientRect()
  const { W, H } = dims()
  return { dx: (dxPx / r.width) * W, dy: (dyPx / r.height) * H }
}

function begin(m: Mode, e: PointerEvent) {
  dragMode = m
  startX = e.clientX
  startY = e.clientY
  orig = normalizeRect(props.rect)
  ;(e.target as HTMLElement).setPointerCapture?.(e.pointerId)
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  e.preventDefault()
  e.stopPropagation()
}

function onMove(e: PointerEvent) {
  if (!dragMode) return
  const { dx, dy } = toSrc(e.clientX - startX, e.clientY - startY)
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
  dragMode = null
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
}
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
