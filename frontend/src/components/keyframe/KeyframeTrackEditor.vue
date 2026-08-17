<script setup lang="ts">
// Reusable keyframe lane: a value curve with diamond markers that drag in both
// time and value, plus add/delete and the track interpolation picker. The
// component owns no state of its own beyond the selection, so every edit goes
// back through `update:modelValue` and lands in the store sanitized.

import { computed, onUnmounted, ref, useId } from 'vue'
import {
  indexOfTime,
  MAX_KEYFRAMES,
  moveKeyframe,
  putKeyframe,
  quantizeTime,
  removeKeyframe,
  sampleTrack,
  sampleTrackCurve,
  setTrackInterpolation,
  trackBounds,
  trackInterpolation,
} from '../../domain/keyframes'
import type { Interpolation, KeyframeTrack } from '../../types'

const props = withDefaults(
  defineProps<{
    modelValue: KeyframeTrack
    /** Length of the timeline the lane spans, in seconds. */
    duration: number
    min: number
    max: number
    /** Value a missing track behaves as (1 for zoom and speed, 0 for pan). */
    neutral: number
    label: string
    unit?: string
    /** Playhead position on the same timeline, in seconds. */
    playhead?: number
    /** Digits used in the readouts and the numeric input step. */
    precision?: number
  }>(),
  { unit: '', playhead: 0, precision: 2 },
)

const emit = defineEmits<{
  'update:modelValue': [KeyframeTrack]
  'interaction-start': []
  'interaction-end': []
}>()

const INTERPOLATIONS: { key: Interpolation; label: string; title: string }[] = [
  { key: 'hold', label: 'Ступень', title: 'Значение держится до следующего ключа' },
  { key: 'linear', label: 'Линейно', title: 'Равномерный переход между ключами' },
  { key: 'smooth', label: 'Плавно', title: 'Плавный разгон и торможение' },
]

const VIEW_WIDTH = 320
const VIEW_HEIGHT = 96
const PADDING_X = 6
const PADDING_Y = 10
const PLOT_WIDTH = VIEW_WIDTH - PADDING_X * 2
const PLOT_HEIGHT = VIEW_HEIGHT - PADDING_Y * 2
const CURVE_STEPS = 96

const lane = ref<SVGSVGElement | null>(null)
const selected = ref(0)
const instanceId = useId().replace(/:/g, '')

let drag: { pointerId: number; index: number } | null = null

const span = computed(() => (props.duration > 0.05 ? props.duration : 1))
const bounds = computed(() => trackBounds(props.modelValue, props.neutral, props.min, props.max))
const interp = computed(() => trackInterpolation(props.modelValue))
const isEmpty = computed(() => props.modelValue.length === 0)
const isFull = computed(() => props.modelValue.length >= MAX_KEYFRAMES)
const selectedPoint = computed(() => props.modelValue[selected.value] ?? null)
const playheadValue = computed(() =>
  sampleTrack(props.modelValue, props.playhead, props.neutral),
)

const curvePoints = computed(() => {
  if (isEmpty.value) return ''
  const samples = sampleTrackCurve(props.modelValue, span.value, CURVE_STEPS, props.neutral)
  return samples
    .map((value, index) => `${toX((span.value * index) / CURVE_STEPS)},${toY(value)}`)
    .join(' ')
})

function toX(seconds: number): number {
  return PADDING_X + (Math.min(Math.max(seconds, 0), span.value) / span.value) * PLOT_WIDTH
}

function toY(value: number): number {
  const { min, max } = bounds.value
  const range = max - min || 1
  return PADDING_Y + (1 - (Math.min(Math.max(value, min), max) - min) / range) * PLOT_HEIGHT
}

function fromPointer(event: PointerEvent): { t: number; v: number } | null {
  const element = lane.value
  if (!element) return null
  const rect = element.getBoundingClientRect()
  if (rect.width <= 0 || rect.height <= 0) return null
  const x = ((event.clientX - rect.left) / rect.width) * VIEW_WIDTH
  const y = ((event.clientY - rect.top) / rect.height) * VIEW_HEIGHT
  const { min, max } = bounds.value
  const ratio = (y - PADDING_Y) / PLOT_HEIGHT
  const seconds = ((x - PADDING_X) / PLOT_WIDTH) * span.value
  return {
    t: quantizeTime(Math.min(span.value, seconds)),
    v: Math.min(max, Math.max(min, max - ratio * (max - min))),
  }
}

function format(value: number): string {
  return value.toFixed(props.precision).replace('.', ',')
}

function commit(track: KeyframeTrack): void {
  emit('update:modelValue', track)
}

function discrete(update: () => void): void {
  emit('interaction-start')
  update()
  emit('interaction-end')
}

function addAtPlayhead(): void {
  if (isFull.value) return
  discrete(() => {
    const t = quantizeTime(props.playhead)
    const next = putKeyframe(props.modelValue, t, playheadValue.value)
    selected.value = Math.max(0, indexOfTime(next, t))
    commit(next)
  })
}

function deleteSelected(): void {
  if (!selectedPoint.value) return
  discrete(() => {
    const index = selected.value
    const next = removeKeyframe(props.modelValue, index)
    selected.value = Math.max(0, Math.min(index, next.length - 1))
    commit(next)
  })
}

function clearTrack(): void {
  if (isEmpty.value) return
  discrete(() => {
    selected.value = 0
    commit([])
  })
}

function pickInterpolation(next: Interpolation): void {
  if (next === interp.value || isEmpty.value) return
  discrete(() => commit(setTrackInterpolation(props.modelValue, next)))
}

function onLanePointerDown(event: PointerEvent): void {
  if (event.button !== 0 || isFull.value) return
  const point = fromPointer(event)
  if (!point) return
  emit('interaction-start')
  const next = putKeyframe(props.modelValue, point.t, point.v)
  const index = Math.max(0, indexOfTime(next, point.t))
  selected.value = index
  commit(next)
  startDrag(index, event)
}

function startDrag(index: number, event: PointerEvent, begin = false): void {
  if (event.button !== 0) return
  if (begin) emit('interaction-start')
  selected.value = index
  drag = { pointerId: event.pointerId, index }
  lane.value?.setPointerCapture?.(event.pointerId)
  window.addEventListener('pointermove', onPointerMove)
  window.addEventListener('pointerup', stopDrag)
  window.addEventListener('pointercancel', stopDrag)
  event.preventDefault()
  event.stopPropagation()
}

function onPointerMove(event: PointerEvent): void {
  if (!drag || event.pointerId !== drag.pointerId) return
  const point = fromPointer(event)
  if (!point) return
  const next = moveKeyframe(props.modelValue, drag.index, point.t, point.v)
  const index = indexOfTime(next, point.t)
  if (index >= 0) {
    drag.index = index
    selected.value = index
  }
  commit(next)
}

function stopDrag(): void {
  if (!drag) return
  const { pointerId } = drag
  drag = null
  window.removeEventListener('pointermove', onPointerMove)
  window.removeEventListener('pointerup', stopDrag)
  window.removeEventListener('pointercancel', stopDrag)
  if (lane.value?.hasPointerCapture?.(pointerId)) lane.value.releasePointerCapture(pointerId)
  emit('interaction-end')
}

function onMarkerKeydown(index: number, event: KeyboardEvent): void {
  selected.value = index
  if (event.key === 'Delete' || event.key === 'Backspace') {
    event.preventDefault()
    deleteSelected()
    return
  }
  const point = props.modelValue[index]
  if (!point) return
  const timeStep = event.shiftKey ? 1 : 0.1
  const valueStep = ((props.max - props.min) / 100) * (event.shiftKey ? 10 : 1)
  let t = point.t
  let v = point.v
  if (event.key === 'ArrowLeft') t -= timeStep
  else if (event.key === 'ArrowRight') t += timeStep
  else if (event.key === 'ArrowUp') v += valueStep
  else if (event.key === 'ArrowDown') v -= valueStep
  else return

  event.preventDefault()
  discrete(() => {
    const next = moveKeyframe(props.modelValue, index, Math.max(0, t), v)
    const moved = indexOfTime(next, quantizeTime(Math.max(0, t)))
    if (moved >= 0) selected.value = moved
    commit(next)
  })
}

function updateSelected(field: 't' | 'v', event: Event): void {
  const input = event.currentTarget as HTMLInputElement
  const point = selectedPoint.value
  if (!point || !Number.isFinite(input.valueAsNumber)) return
  const t = field === 't' ? Math.max(0, input.valueAsNumber) : point.t
  const v = field === 'v' ? input.valueAsNumber : point.v
  discrete(() => {
    const next = moveKeyframe(props.modelValue, selected.value, t, v)
    const moved = indexOfTime(next, quantizeTime(t))
    if (moved >= 0) selected.value = moved
    commit(next)
  })
}

onUnmounted(stopDrag)
</script>

<template>
  <div class="kf-track">
    <div class="kf-head">
      <span class="kf-label">{{ label }}</span>
      <span class="kf-readout" aria-live="polite">
        {{ format(playheadValue) }}{{ unit }} · {{ modelValue.length }}/{{ MAX_KEYFRAMES }}
      </span>
    </div>

    <p :id="`${instanceId}-help`" class="visually-hidden">
      Нажмите на дорожку, чтобы добавить ключ. Стрелки двигают выбранный ключ по времени и
      значению, Delete удаляет его.
    </p>

    <svg
      ref="lane"
      class="kf-lane"
      :viewBox="`0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}`"
      role="group"
      :aria-label="`Ключевые кадры: ${label}`"
      :aria-describedby="`${instanceId}-help`"
      @pointerdown="onLanePointerDown"
    >
      <rect class="kf-bg" :x="PADDING_X" :y="PADDING_Y" :width="PLOT_WIDTH" :height="PLOT_HEIGHT" rx="3" />
      <line
        class="kf-neutral"
        :x1="PADDING_X"
        :x2="PADDING_X + PLOT_WIDTH"
        :y1="toY(neutral)"
        :y2="toY(neutral)"
      />
      <polyline v-if="!isEmpty" class="kf-curve" :points="curvePoints" />
      <line
        class="kf-playhead"
        :x1="toX(playhead)"
        :x2="toX(playhead)"
        :y1="PADDING_Y"
        :y2="PADDING_Y + PLOT_HEIGHT"
      />
      <g
        v-for="(point, index) in modelValue"
        :key="index"
        class="kf-marker"
        :class="{ selected: selected === index }"
        :transform="`translate(${toX(point.t)} ${toY(point.v)})`"
        role="button"
        tabindex="0"
        :aria-label="`${label}: ключ ${index + 1}, ${format(point.t)} с, значение ${format(point.v)}`"
        @focus="selected = index"
        @pointerdown.stop="startDrag(index, $event, true)"
        @keydown="onMarkerKeydown(index, $event)"
      >
        <rect class="kf-hit" x="-9" y="-9" width="18" height="18" />
        <rect class="kf-diamond" x="-4.5" y="-4.5" width="9" height="9" transform="rotate(45)" />
      </g>
    </svg>

    <div class="kf-tools">
      <div class="kf-interp" role="group" aria-label="Интерполяция дорожки">
        <button
          v-for="option in INTERPOLATIONS"
          :key="option.key"
          type="button"
          class="kf-chip"
          :title="option.title"
          :aria-pressed="interp === option.key"
          :disabled="isEmpty"
          @click="pickInterpolation(option.key)"
        >
          {{ option.label }}
        </button>
      </div>
      <button type="button" class="kf-chip" :disabled="isFull" @click="addAtPlayhead">
        Ключ на курсоре
      </button>
      <button type="button" class="kf-chip" :disabled="!selectedPoint" @click="deleteSelected">
        Удалить
      </button>
      <button type="button" class="kf-chip" :disabled="isEmpty" @click="clearTrack">Сброс</button>
    </div>

    <div v-if="selectedPoint" class="kf-fields">
      <label>
        <span>Время, с</span>
        <input
          type="number"
          step="0.05"
          min="0"
          :max="span"
          :value="selectedPoint.t"
          @change="updateSelected('t', $event)"
        />
      </label>
      <label>
        <span>Значение{{ unit ? `, ${unit.trim()}` : '' }}</span>
        <input
          type="number"
          :step="Math.max(0.01, (max - min) / 100)"
          :min="min"
          :max="max"
          :value="selectedPoint.v"
          @change="updateSelected('v', $event)"
        />
      </label>
    </div>
  </div>
</template>

<style scoped>
.kf-track {
  display: grid;
  gap: 6px;
  min-width: 0;
}

.kf-head {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 8px;
}

.kf-label {
  font-size: 0.84rem;
  font-weight: 600;
}

.kf-readout {
  color: var(--muted);
  font-size: 0.76rem;
  font-variant-numeric: tabular-nums;
}

.kf-lane {
  display: block;
  width: 100%;
  height: auto;
  aspect-ratio: 320 / 96;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--panel-3);
  touch-action: none;
  user-select: none;
  cursor: crosshair;
}

.kf-bg {
  fill: var(--panel-2);
}

.kf-neutral,
.kf-playhead {
  stroke-width: 1;
  vector-effect: non-scaling-stroke;
}

.kf-neutral {
  stroke: var(--faint);
  stroke-dasharray: 3 3;
}

.kf-playhead {
  stroke: var(--warn);
}

.kf-curve {
  fill: none;
  stroke: var(--accent);
  stroke-width: 2;
  stroke-linejoin: round;
  pointer-events: none;
  vector-effect: non-scaling-stroke;
}

.kf-marker {
  cursor: grab;
  outline: none;
}

.kf-marker:active {
  cursor: grabbing;
}

.kf-hit {
  fill: transparent;
}

.kf-diamond {
  fill: var(--panel-2);
  stroke: var(--accent);
  stroke-width: 2;
  pointer-events: none;
  vector-effect: non-scaling-stroke;
}

.kf-marker.selected .kf-diamond,
.kf-marker:focus-visible .kf-diamond {
  fill: var(--accent);
  stroke: var(--text);
}

.kf-tools {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.kf-interp {
  display: flex;
  gap: 4px;
  margin-right: auto;
}

.kf-chip {
  min-height: 30px;
  padding: 3px 9px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text);
  font: inherit;
  font-size: 0.76rem;
  cursor: pointer;
}

.kf-chip:hover:not(:disabled) {
  border-color: var(--border-strong);
}

.kf-chip[aria-pressed='true'] {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.kf-chip:disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

.kf-fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px;
}

.kf-fields label {
  display: grid;
  gap: 3px;
  color: var(--muted);
  font-size: 0.76rem;
}

.kf-fields input {
  width: 100%;
  min-width: 0;
  min-height: 32px;
  padding: 4px 7px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--panel-2);
  color: var(--text);
  font: inherit;
  font-variant-numeric: tabular-nums;
}

.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
}
</style>
