<script setup lang="ts">
import { computed, onUnmounted, ref, useId } from 'vue'
import { neutralColorWheel, sanitizeColorWheel } from '../../domain/edit'
import type { ColorWheelChannels } from '../../types'

const props = defineProps<{
  label: string
  modelValue: ColorWheelChannels
}>()

const emit = defineEmits<{
  'update:modelValue': [ColorWheelChannels]
  'interaction-start': []
  'interaction-end': []
}>()

const SQRT_THREE = Math.sqrt(3)
const KEY_STEP = 0.04
const CHANNELS = ['master', 'red', 'green', 'blue'] as const
const wheel = ref<HTMLElement | null>(null)
const instanceId = useId().replace(/:/g, '')
let pointerId: number | null = null
let pointerMoved = false
let pointerOrigin: { x: number; y: number } | null = null

const safeValue = computed(() => sanitizeColorWheel(props.modelValue))

function round(value: number): number {
  return Math.round(Math.max(-1, Math.min(1, value)) * 1000) / 1000
}

function chromaPoint(value = safeValue.value): { x: number; y: number } {
  const average = (value.red + value.green + value.blue) / 3
  let x = value.red - average
  let y = (value.blue - value.green) / SQRT_THREE
  const radius = Math.hypot(x, y)
  if (radius > 1) { x /= radius; y /= radius }
  return { x, y }
}

const point = computed(() => chromaPoint())
const handleStyle = computed(() => ({
  left: `${50 + point.value.x * 50}%`,
  top: `${50 - point.value.y * 50}%`,
  backgroundColor: `rgb(${Math.round(127.5 + safeValue.value.red * 127.5)} ${Math.round(127.5 + safeValue.value.green * 127.5)} ${Math.round(127.5 + safeValue.value.blue * 127.5)})`,
}))
const hue = computed(() => {
  if (Math.hypot(point.value.x, point.value.y) < 1e-6) return 0
  return Math.round((Math.atan2(point.value.y, point.value.x) * 180 / Math.PI + 360) % 360)
})
const saturation = computed(() => Math.round(Math.min(1, Math.hypot(point.value.x, point.value.y)) * 100))

function update(next: ColorWheelChannels): void {
  emit('update:modelValue', sanitizeColorWheel(next))
}

function updateChroma(requestedX: number, requestedY: number): void {
  const radius = Math.hypot(requestedX, requestedY)
  const x = radius > 1 ? requestedX / radius : requestedX
  const y = radius > 1 ? requestedY / radius : requestedY
  update({
    master: safeValue.value.master,
    red: round(x),
    green: round(-0.5 * x - (SQRT_THREE / 2) * y),
    blue: round(-0.5 * x + (SQRT_THREE / 2) * y),
  })
}

function pointFromPointer(event: PointerEvent): { x: number; y: number } | null {
  const element = wheel.value
  if (!element) return null
  const rect = element.getBoundingClientRect()
  if (rect.width <= 0 || rect.height <= 0) return null
  return {
    x: ((event.clientX - rect.left) / rect.width) * 2 - 1,
    y: 1 - ((event.clientY - rect.top) / rect.height) * 2,
  }
}

function onPointerMove(event: PointerEvent): void {
  if (pointerId !== event.pointerId) return
  const requested = pointFromPointer(event)
  if (!requested) return
  if (!pointerMoved) {
    if (!pointerOrigin || Math.hypot(event.clientX - pointerOrigin.x, event.clientY - pointerOrigin.y) < 3) return
    pointerMoved = true
    emit('interaction-start')
  }
  updateChroma(requested.x, requested.y)
}

function stopPointer(): void {
  if (pointerId === null) return
  pointerId = null
  window.removeEventListener('pointermove', onPointerMove)
  window.removeEventListener('pointerup', onPointerUp)
  window.removeEventListener('pointercancel', onPointerCancel)
  if (pointerMoved) emit('interaction-end')
  pointerMoved = false
  pointerOrigin = null
}

function onPointerUp(event: PointerEvent): void {
  if (pointerId !== event.pointerId) return
  stopPointer()
}

function onPointerCancel(event: PointerEvent): void {
  if (pointerId !== event.pointerId) return
  stopPointer()
}

function onPointerDown(event: PointerEvent): void {
  if (event.button !== 0) return
  stopPointer()
  pointerId = event.pointerId
  pointerMoved = false
  pointerOrigin = { x: event.clientX, y: event.clientY }
  wheel.value?.setPointerCapture?.(event.pointerId)
  window.addEventListener('pointermove', onPointerMove)
  window.addEventListener('pointerup', onPointerUp)
  window.addEventListener('pointercancel', onPointerCancel)
  event.preventDefault()
}

function onDoubleClick(): void {
  reset()
}

function onWheelKeydown(event: KeyboardEvent): void {
  if (event.key === 'Home') {
    event.preventDefault()
    reset()
    return
  }
  const step = event.altKey ? 0.01 : event.shiftKey ? KEY_STEP * 2.5 : KEY_STEP
  const current = point.value
  let next: { x: number; y: number } | null = null
  if (event.key === 'ArrowLeft') next = { x: current.x - step, y: current.y }
  else if (event.key === 'ArrowRight') next = { x: current.x + step, y: current.y }
  else if (event.key === 'ArrowDown') next = { x: current.x, y: current.y - step }
  else if (event.key === 'ArrowUp') next = { x: current.x, y: current.y + step }
  if (!next) return
  event.preventDefault()
  emit('interaction-start')
  updateChroma(next.x, next.y)
  emit('interaction-end')
}

function updateChannel(channel: keyof ColorWheelChannels, event: Event): void {
  const input = event.currentTarget as HTMLInputElement
  if (!Number.isFinite(input.valueAsNumber)) return
  update({ ...safeValue.value, [channel]: round(input.valueAsNumber) })
}

function finishNumeric(channel: keyof ColorWheelChannels, event: FocusEvent): void {
  const input = event.currentTarget as HTMLInputElement
  if (Number.isFinite(input.valueAsNumber)) updateChannel(channel, event)
  else input.value = String(safeValue.value[channel])
  emit('interaction-end')
}

function reset(): void {
  emit('interaction-start')
  update(neutralColorWheel())
  emit('interaction-end')
}

onUnmounted(stopPointer)
</script>

<template>
  <fieldset class="color-wheel-card">
    <legend>{{ label }}</legend>
    <div class="color-wheel-layout">
      <div
        ref="wheel"
        class="color-wheel-surface"
        role="slider"
        tabindex="0"
        :aria-label="`${label}: оттенок и насыщенность`"
        aria-valuemin="0"
        aria-valuemax="359"
        :aria-valuenow="hue"
        :aria-valuetext="`Оттенок ${hue}°, насыщенность ${saturation}%`"
        :aria-describedby="`${instanceId}-wheel-help`"
        title="Стрелки — движение; Alt — точно; Shift — крупно; двойной щелчок — нейтраль"
        @pointerdown="onPointerDown"
        @keydown="onWheelKeydown"
        @dblclick.prevent="onDoubleClick"
      >
        <span class="color-wheel-handle" :style="handleStyle" aria-hidden="true"></span>
      </div>

      <div class="color-wheel-values">
        <label v-for="channel in CHANNELS" :key="channel">
          <span>{{ channel === 'master' ? 'Master' : channel[0]!.toUpperCase() }}</span>
          <input
            :id="`${instanceId}-${channel}`"
            type="number"
            min="-1"
            max="1"
            step="0.01"
            :aria-label="`${label}: ${channel === 'master' ? 'Master' : channel[0]!.toUpperCase()}`"
            :value="safeValue[channel]"
            @focus="$emit('interaction-start')"
            @input="updateChannel(channel, $event)"
            @blur="finishNumeric(channel, $event)"
          />
        </label>
      </div>
    </div>
    <p :id="`${instanceId}-wheel-help`" class="color-wheel-help">
      Стрелки — движение · Alt — точно · Shift — крупно · двойной щелчок — сброс
    </p>
    <button type="button" class="btn ghost sm color-wheel-reset" :aria-label="`Сбросить ${label}`" @click="reset">
      Нейтраль
    </button>
  </fieldset>
</template>

<style scoped>
.color-wheel-card {
  min-width: 0;
  margin: 0;
  padding: 11px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--panel-2);
}

.color-wheel-card legend {
  padding: 0 5px;
  color: var(--text);
  font-size: 13px;
  font-weight: 650;
}

.color-wheel-layout {
  display: grid;
  grid-template-columns: minmax(90px, 124px) minmax(0, 1fr);
  gap: 10px;
  align-items: center;
}

.color-wheel-surface {
  position: relative;
  width: 100%;
  aspect-ratio: 1;
  border: 1px solid var(--border-strong);
  border-radius: 50%;
  background:
    radial-gradient(circle, rgb(255 255 255) 0%, rgb(255 255 255 / 0%) 72%),
    conic-gradient(from 90deg, #f33, #ff3, #3f3, #3ff, #33f, #f3f, #f33);
  box-shadow: inset 0 0 18px rgb(0 0 0 / 26%);
  cursor: crosshair;
  touch-action: none;
}

.color-wheel-surface:focus-visible {
  outline: 3px solid color-mix(in srgb, var(--accent) 65%, transparent);
  outline-offset: 3px;
}

.color-wheel-handle {
  position: absolute;
  width: 14px;
  height: 14px;
  border: 2px solid white;
  border-radius: 50%;
  box-shadow: 0 1px 5px rgb(0 0 0 / 70%);
  transform: translate(-50%, -50%);
  pointer-events: none;
}

.color-wheel-values {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 7px;
}

.color-wheel-values label {
  display: grid;
  gap: 3px;
  color: var(--muted);
  font-size: 11px;
}

.color-wheel-values input {
  min-width: 0;
  width: 100%;
  box-sizing: border-box;
  padding: 6px 7px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--panel);
  color: var(--text);
  font-variant-numeric: tabular-nums;
}

.color-wheel-values input:focus {
  border-color: var(--accent);
  outline: none;
}

.color-wheel-reset {
  width: 100%;
  margin-top: 9px;
}

.color-wheel-help {
  margin: 8px 0 0;
  color: var(--muted);
  font-size: 10px;
  line-height: 1.35;
}

@media (max-width: 420px) {
  .color-wheel-layout {
    grid-template-columns: 105px minmax(0, 1fr);
  }
}
</style>
