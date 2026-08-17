<script setup lang="ts">
// One primary wheel: a hue disc for the colour offset, a ring around it for the
// master (luminance) offset, and a disclosure with the raw RGB numbers. Both
// controls are keyboard operable and reset on double click.
import { computed, onBeforeUnmount } from 'vue'
import { beginEditTransaction, endEditTransaction } from '../../store'
import type { Rgb } from '../../types'
import {
  clampChannel,
  formatChannel,
  isNeutralWheel,
  masterOf,
  neutralRgb,
  nudgeWheel,
  pointFromRgb,
  rgbFromPoint,
  withMaster,
  WHEEL_CONFIG,
  type WheelMode,
} from './wheel'

const props = defineProps<{ modelValue: Rgb; mode: WheelMode }>()
const emit = defineEmits<{ 'update:modelValue': [Rgb] }>()

/** Arrow-key step in disc units, and the finer step Shift gives. */
const STEP = 0.05
const FINE_STEP = 0.01
/** Half the angular travel of the ring, degrees. */
const RING_SWEEP = 150

const config = computed(() => WHEEL_CONFIG[props.mode])
const point = computed(() => pointFromRgb(props.modelValue, props.mode))
const master = computed(() => masterOf(props.modelValue, props.mode))
const neutral = computed(() => isNeutralWheel(props.modelValue, props.mode))

const puckStyle = computed(() => ({
  left: `${50 + point.value.x * 50}%`,
  top: `${50 - point.value.y * 50}%`,
}))

const ringStyle = computed(() => {
  const angle = ((-90 + master.value * RING_SWEEP) * Math.PI) / 180
  return {
    left: `${50 + Math.cos(angle) * 47}%`,
    top: `${50 + Math.sin(angle) * 47}%`,
  }
})

const channels = [
  { key: 'r', label: 'R' },
  { key: 'g', label: 'G' },
  { key: 'b', label: 'B' },
] as const

let transactionOpen = false

function begin(): void {
  if (transactionOpen) return
  transactionOpen = true
  beginEditTransaction(`wheel-${props.mode}`)
}

function end(): void {
  if (!transactionOpen) return
  transactionOpen = false
  endEditTransaction()
}

onBeforeUnmount(end)

function commit(value: Rgb): void {
  emit('update:modelValue', value)
}

function reset(): void {
  begin()
  commit(neutralRgb(props.mode))
  end()
}

function discPoint(event: PointerEvent): { x: number; y: number } {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect()
  const radius = Math.max(1, Math.min(box.width, box.height) / 2)
  return {
    x: (event.clientX - (box.left + box.width / 2)) / radius,
    y: -(event.clientY - (box.top + box.height / 2)) / radius,
  }
}

function onDiscDown(event: PointerEvent): void {
  const target = event.currentTarget as HTMLElement
  target.setPointerCapture(event.pointerId)
  begin()
  const at = discPoint(event)
  commit(rgbFromPoint(at.x, at.y, props.mode))
}

function onDiscMove(event: PointerEvent): void {
  const target = event.currentTarget as HTMLElement
  if (!target.hasPointerCapture(event.pointerId)) return
  const at = discPoint(event)
  commit(rgbFromPoint(at.x, at.y, props.mode))
}

function stepOf(event: KeyboardEvent): number {
  return event.shiftKey ? FINE_STEP : STEP
}

function onDiscKey(event: KeyboardEvent): void {
  const step = stepOf(event)
  const moves: Record<string, [number, number]> = {
    ArrowLeft: [-step, 0],
    ArrowRight: [step, 0],
    ArrowUp: [0, step],
    ArrowDown: [0, -step],
  }
  const move = moves[event.key]
  if (move) {
    event.preventDefault()
    begin()
    commit(nudgeWheel(props.modelValue, props.mode, move[0], move[1]))
    return
  }
  if (event.key === 'Home' || event.key === 'Backspace' || event.key === 'Delete') {
    event.preventDefault()
    reset()
  }
}

function ringMaster(event: PointerEvent): number {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect()
  const dx = event.clientX - (box.left + box.width / 2)
  const dy = event.clientY - (box.top + box.height / 2)
  // Degrees away from the top of the ring, which is where neutral sits.
  let degrees = (Math.atan2(dy, dx) * 180) / Math.PI + 90
  if (degrees > 180) degrees -= 360
  return Math.max(-1, Math.min(1, degrees / RING_SWEEP))
}

function onRingDown(event: PointerEvent): void {
  const target = event.currentTarget as HTMLElement
  target.setPointerCapture(event.pointerId)
  begin()
  commit(withMaster(props.modelValue, props.mode, ringMaster(event)))
}

function onRingMove(event: PointerEvent): void {
  const target = event.currentTarget as HTMLElement
  if (!target.hasPointerCapture(event.pointerId)) return
  commit(withMaster(props.modelValue, props.mode, ringMaster(event)))
}

function onRingKey(event: KeyboardEvent): void {
  const step = stepOf(event)
  const deltas: Record<string, number> = {
    ArrowLeft: -step,
    ArrowDown: -step,
    ArrowRight: step,
    ArrowUp: step,
  }
  const delta = deltas[event.key]
  if (delta !== undefined) {
    event.preventDefault()
    begin()
    commit(withMaster(props.modelValue, props.mode, master.value + delta))
    return
  }
  if (event.key === 'Home') {
    event.preventDefault()
    begin()
    commit(withMaster(props.modelValue, props.mode, 0))
    end()
  }
}

function setChannel(key: 'r' | 'g' | 'b', raw: string): void {
  const parsed = Number.parseFloat(raw)
  begin()
  commit({ ...props.modelValue, [key]: clampChannel(parsed, props.mode) })
  end()
}
</script>

<template>
  <div class="wheel">
    <div class="wheel-head">
      <span>{{ config.label }}</span>
      <button
        type="button"
        class="btn ghost sm"
        :disabled="neutral"
        :title="`Сбросить «${config.label}»`"
        @click="reset"
      >
        сброс
      </button>
    </div>

    <div class="wheel-stage" @dblclick="reset">
      <div
        class="wheel-ring"
        role="slider"
        tabindex="0"
        :aria-label="`${config.label}: яркость`"
        aria-valuemin="-1"
        aria-valuemax="1"
        :aria-valuenow="Number(master.toFixed(2))"
        :aria-valuetext="`яркость ${master.toFixed(2)}`"
        @pointerdown="onRingDown"
        @pointermove="onRingMove"
        @pointerup="end"
        @pointercancel="end"
        @keydown="onRingKey"
        @keyup="end"
        @blur="end"
      >
        <span class="wheel-ring-dot" :style="ringStyle"></span>
      </div>
      <div
        class="wheel-disc"
        role="group"
        tabindex="0"
        :aria-label="`${config.label}: цветовое колесо, стрелки смещают оттенок, Home сбрасывает`"
        @pointerdown="onDiscDown"
        @pointermove="onDiscMove"
        @pointerup="end"
        @pointercancel="end"
        @keydown="onDiscKey"
        @keyup="end"
        @blur="end"
      >
        <span class="wheel-puck" :style="puckStyle"></span>
      </div>
    </div>

    <details class="wheel-numbers">
      <summary>Числа RGB</summary>
      <div class="wheel-fields">
        <label v-for="channel in channels" :key="channel.key">
          {{ channel.label }}
          <input
            type="number"
            :min="config.min"
            :max="config.max"
            step="0.01"
            :value="formatChannel(modelValue[channel.key], mode)"
            @change="setChannel(channel.key, ($event.target as HTMLInputElement).value)"
          />
        </label>
      </div>
    </details>
  </div>
</template>

<style scoped>
.wheel {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.wheel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  color: var(--muted);
  font-size: 12px;
}

.wheel-stage {
  position: relative;
  aspect-ratio: 1;
  width: 100%;
}

.wheel-ring,
.wheel-disc {
  position: absolute;
  border-radius: 50%;
  touch-action: none;
}

.wheel-ring {
  inset: 0;
  border: 2px solid var(--border-strong);
  background: var(--panel-3);
  cursor: grab;
}

.wheel-disc {
  inset: 14%;
  cursor: crosshair;
  border: 1px solid var(--border);
  background:
    radial-gradient(circle at 50% 50%, var(--panel) 0%, transparent 72%),
    conic-gradient(
      #ff4d4d,
      #ff4dff,
      #4d6bff,
      #4dffff,
      #4dff6b,
      #ffff4d,
      #ff4d4d
    );
}

.wheel-ring:focus-visible,
.wheel-disc:focus-visible {
  outline: none;
  box-shadow: var(--ring);
}

.wheel-ring-dot,
.wheel-puck {
  position: absolute;
  border-radius: 50%;
  transform: translate(-50%, -50%);
  pointer-events: none;
}

.wheel-ring-dot {
  width: 10px;
  height: 10px;
  background: var(--text);
  border: 2px solid var(--panel);
}

.wheel-puck {
  width: 12px;
  height: 12px;
  border: 2px solid var(--text);
  background: transparent;
  box-shadow: 0 0 0 1px var(--panel);
}

.wheel-numbers summary {
  color: var(--muted);
  font-size: 12px;
  cursor: pointer;
}

.wheel-fields {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 4px;
  margin-top: 4px;
}

.wheel-fields label {
  display: flex;
  align-items: center;
  gap: 4px;
  color: var(--faint);
  font-size: 11px;
}

.wheel-fields input {
  width: 100%;
  min-width: 0;
  padding: 3px 4px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--panel);
  color: var(--text);
  font: inherit;
  font-size: 11px;
}

.wheel-fields input:focus {
  outline: none;
  box-shadow: var(--ring);
}
</style>
