<script setup lang="ts">
import { computed, ref } from 'vue'
import { state } from '../store'

const root = ref<HTMLElement | null>(null)
const MIN = 16 // minimum crop size in source pixels

type Mode = 'move' | 'nw' | 'ne' | 'sw' | 'se'
let mode: Mode | null = null
let startX = 0
let startY = 0
let orig = { x: 0, y: 0, w: 0, h: 0 }

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(v, hi))
}

function dims() {
  return { W: state.video?.width || 1, H: state.video?.height || 1 }
}

// Crop rect as percentages of the source frame (the overlay exactly covers the
// displayed video, so percentages map regardless of the on-screen scale).
const rectStyle = computed(() => {
  const { W, H } = dims()
  const c = state.edit.crop
  return {
    left: `${(c.x / W) * 100}%`,
    top: `${(c.y / H) * 100}%`,
    width: `${(c.w / W) * 100}%`,
    height: `${(c.h / H) * 100}%`,
  }
})

function toSrc(dxPx: number, dyPx: number) {
  const r = root.value!.getBoundingClientRect()
  const { W, H } = dims()
  return { dx: (dxPx / r.width) * W, dy: (dyPx / r.height) * H }
}

function begin(m: Mode, e: PointerEvent) {
  mode = m
  startX = e.clientX
  startY = e.clientY
  orig = { ...state.edit.crop }
  ;(e.target as HTMLElement).setPointerCapture?.(e.pointerId)
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
  e.preventDefault()
  e.stopPropagation()
}

function onMove(e: PointerEvent) {
  if (!mode) return
  const { dx, dy } = toSrc(e.clientX - startX, e.clientY - startY)
  const { W, H } = dims()
  if (mode === 'move') {
    const x = clamp(orig.x + dx, 0, W - orig.w)
    const y = clamp(orig.y + dy, 0, H - orig.h)
    state.edit.crop = { x: Math.round(x), y: Math.round(y), w: orig.w, h: orig.h }
    return
  }
  let x1 = orig.x
  let y1 = orig.y
  let x2 = orig.x + orig.w
  let y2 = orig.y + orig.h
  if (mode.includes('w')) x1 = clamp(orig.x + dx, 0, x2 - MIN)
  if (mode.includes('e')) x2 = clamp(orig.x + orig.w + dx, x1 + MIN, W)
  if (mode.includes('n')) y1 = clamp(orig.y + dy, 0, y2 - MIN)
  if (mode.includes('s')) y2 = clamp(orig.y + orig.h + dy, y1 + MIN, H)
  state.edit.crop = {
    x: Math.round(x1),
    y: Math.round(y1),
    w: Math.round(x2 - x1),
    h: Math.round(y2 - y1),
  }
}

function onUp() {
  mode = null
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
}
</script>

<template>
  <div ref="root" class="crop-overlay">
    <div class="crop-rect" :style="rectStyle" @pointerdown="begin('move', $event)">
      <span class="crop-handle nw" @pointerdown="begin('nw', $event)"></span>
      <span class="crop-handle ne" @pointerdown="begin('ne', $event)"></span>
      <span class="crop-handle sw" @pointerdown="begin('sw', $event)"></span>
      <span class="crop-handle se" @pointerdown="begin('se', $event)"></span>
    </div>
  </div>
</template>
