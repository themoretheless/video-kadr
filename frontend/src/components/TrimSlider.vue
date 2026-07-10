<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'

const props = withDefaults(
  defineProps<{
    min: number
    max: number
    start: number
    end: number
    step?: number
  }>(),
  { step: 0.1 },
)

const emit = defineEmits<{
  'update:start': [number]
  'update:end': [number]
}>()

const GAP = 0.1
const track = ref<HTMLElement | null>(null)
let active: 'start' | 'end' | null = null

function pct(v: number): number {
  const range = props.max - props.min
  if (range <= 0) return 0
  return Math.max(0, Math.min(100, ((v - props.min) / range) * 100))
}

const startPct = computed(() => pct(props.start))
const endPct = computed(() => pct(props.end))

function snap(v: number): number {
  const s = props.step
  return Math.round(v / s) * s
}

function clampStart(v: number): number {
  return Math.max(props.min, Math.min(snap(v), props.end - GAP))
}

function clampEnd(v: number): number {
  return Math.min(props.max, Math.max(snap(v), props.start + GAP))
}

function valueFromClientX(x: number): number {
  const el = track.value
  if (!el) return props.min
  const rect = el.getBoundingClientRect()
  const t = rect.width <= 0 ? 0 : (x - rect.left) / rect.width
  return props.min + Math.max(0, Math.min(1, t)) * (props.max - props.min)
}

function onMove(e: PointerEvent): void {
  if (!active) return
  const v = valueFromClientX(e.clientX)
  if (active === 'start') emit('update:start', clampStart(v))
  else emit('update:end', clampEnd(v))
}

function onUp(): void {
  stopDrag()
}

function stopDrag(): void {
  active = null
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', onUp)
}

onUnmounted(stopDrag)

function startDrag(which: 'start' | 'end', e: PointerEvent): void {
  active = which
  ;(e.target as HTMLElement).setPointerCapture?.(e.pointerId)
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', onUp)
}

// Clicking the bare track jumps the nearest handle there, then drags it.
function onTrackDown(e: PointerEvent): void {
  if ((e.target as HTMLElement).classList.contains('trim-handle')) return
  const v = valueFromClientX(e.clientX)
  const which = Math.abs(v - props.start) <= Math.abs(v - props.end) ? 'start' : 'end'
  if (which === 'start') emit('update:start', clampStart(v))
  else emit('update:end', clampEnd(v))
  startDrag(which, e)
}

function onKey(which: 'start' | 'end', e: KeyboardEvent): void {
  const big = e.shiftKey ? 1 : props.step
  let d = 0
  if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') d = -big
  else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') d = big
  else return
  e.preventDefault()
  if (which === 'start') emit('update:start', clampStart(props.start + d))
  else emit('update:end', clampEnd(props.end + d))
}
</script>

<template>
  <div class="trim">
    <div ref="track" class="trim-track" @pointerdown="onTrackDown">
      <div class="trim-fill" :style="{ left: startPct + '%', width: endPct - startPct + '%' }"></div>
      <div
        class="trim-handle"
        :style="{ left: startPct + '%' }"
        tabindex="0"
        role="slider"
        aria-label="Начало обрезки"
        :aria-valuemin="min"
        :aria-valuemax="max"
        :aria-valuenow="start"
        @pointerdown="startDrag('start', $event)"
        @keydown="onKey('start', $event)"
      ></div>
      <div
        class="trim-handle"
        :style="{ left: endPct + '%' }"
        tabindex="0"
        role="slider"
        aria-label="Конец обрезки"
        :aria-valuemin="min"
        :aria-valuemax="max"
        :aria-valuenow="end"
        @pointerdown="startDrag('end', $event)"
        @keydown="onKey('end', $event)"
      ></div>
    </div>
  </div>
</template>
