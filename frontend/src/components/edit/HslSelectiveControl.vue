<script setup lang="ts">
import { computed, onUnmounted, ref, useId } from 'vue'
import {
  HSL_SELECTIVE_DEFAULTS,
  sanitizeHslSelective,
  wrapDegrees,
  type HslSelective,
} from '../../domain/hsl-selective'

type Control = 'center' | 'halfWidth' | 'feather' | 'hue' | 'saturation' | 'lightness'

const props = defineProps<{
  modelValue: HslSelective
  maskPreview: boolean
}>()

const emit = defineEmits<{
  'update:modelValue': [HslSelective]
  'update:maskPreview': [boolean]
  'interaction-start': []
  'interaction-end': []
}>()

const instanceId = useId().replace(/:/g, '')
const ring = ref<HTMLElement | null>(null)
const safeValue = computed(() => sanitizeHslSelective(props.modelValue))

const controls: Array<{
  key: Control
  label: string
  min: number
  max: number
  step: number
  unit: string
}> = [
  { key: 'center', label: 'Центр', min: 0, max: 360, step: 1, unit: '°' },
  { key: 'halfWidth', label: 'Полуширина core', min: 0, max: 180, step: 1, unit: '°' },
  { key: 'feather', label: 'Растушёвка', min: 0, max: 90, step: 1, unit: '°' },
]

const adjustments: Array<{
  key: Control
  label: string
  min: number
  max: number
  step: number
  unit: string
}> = [
  { key: 'hue', label: 'Сдвиг оттенка', min: -180, max: 180, step: 1, unit: '°' },
  { key: 'saturation', label: 'Насыщенность', min: -100, max: 100, step: 1, unit: '%' },
  { key: 'lightness', label: 'Светлота', min: -100, max: 100, step: 1, unit: '%' },
]

let interactionSource: string | null = null
let interactionSnapshot: HslSelective | null = null
let pointerId: number | null = null

function cloneValue(value = safeValue.value): HslSelective {
  return {
    selection: { ...value.selection },
    adjustment: { ...value.adjustment },
  }
}

function update(next: HslSelective): void {
  emit('update:modelValue', sanitizeHslSelective(next))
}

function beginInteraction(source: string): void {
  if (interactionSource === source) return
  if (interactionSource) finishInteraction()
  interactionSource = source
  interactionSnapshot = cloneValue()
  emit('interaction-start')
}

function finishInteraction(source?: string): void {
  if (!interactionSource || (source && source !== interactionSource)) return
  interactionSource = null
  interactionSnapshot = null
  emit('interaction-end')
}

function cancelInteraction(source?: string): void {
  if (!interactionSource || (source && source !== interactionSource)) return
  const snapshot = interactionSnapshot
  interactionSource = null
  interactionSnapshot = null
  if (snapshot) update(snapshot)
  emit('interaction-end')
}

function rawValue(control: Control): number {
  const value = safeValue.value
  switch (control) {
    case 'center': return value.selection.centerDegrees
    case 'halfWidth': return value.selection.halfWidthDegrees
    case 'feather': return value.selection.featherDegrees
    case 'hue': return value.adjustment.hueDegrees
    case 'saturation': return value.adjustment.saturation
    case 'lightness': return value.adjustment.lightness
  }
}

function displayValue(control: Control): number {
  const value = rawValue(control)
  return control === 'saturation' || control === 'lightness'
    ? Math.round(value * 1000) / 10
    : Math.round(value * 10) / 10
}

function fromDisplayValue(control: Control, value: number): number {
  return control === 'saturation' || control === 'lightness' ? value / 100 : value
}

function setControl(control: Control, value: number): void {
  const next = cloneValue()
  switch (control) {
    case 'center': next.selection.centerDegrees = wrapDegrees(value); break
    case 'halfWidth': next.selection.halfWidthDegrees = value; break
    case 'feather': next.selection.featherDegrees = value; break
    case 'hue': next.adjustment.hueDegrees = value; break
    case 'saturation': next.adjustment.saturation = value; break
    case 'lightness': next.adjustment.lightness = value; break
  }
  update(next)
}

function controlDefault(control: Control): number {
  switch (control) {
    case 'center': return HSL_SELECTIVE_DEFAULTS.selection.centerDegrees
    case 'halfWidth': return HSL_SELECTIVE_DEFAULTS.selection.halfWidthDegrees
    case 'feather': return HSL_SELECTIVE_DEFAULTS.selection.featherDegrees
    case 'hue': return HSL_SELECTIVE_DEFAULTS.adjustment.hueDegrees
    case 'saturation': return HSL_SELECTIVE_DEFAULTS.adjustment.saturation
    case 'lightness': return HSL_SELECTIVE_DEFAULTS.adjustment.lightness
  }
}

function controlLimit(control: Control, edge: 'min' | 'max'): number {
  if (control === 'center' && edge === 'max') return 359
  if (control === 'feather' && edge === 'max') {
    return Math.min(90, 180 - safeValue.value.selection.halfWidthDegrees)
  }
  const spec = [...controls, ...adjustments].find(candidate => candidate.key === control)!
  const value = edge === 'min' ? spec.min : spec.max
  return fromDisplayValue(control, value)
}

function onNumericFocus(control: Control): void {
  beginInteraction(`numeric-${control}`)
}

function onNumericInput(control: Control, event: Event): void {
  const input = event.currentTarget as HTMLInputElement
  if (!Number.isFinite(input.valueAsNumber)) return
  setControl(control, fromDisplayValue(control, input.valueAsNumber))
}

function onNumericBlur(control: Control, event: FocusEvent): void {
  const input = event.currentTarget as HTMLInputElement
  if (!Number.isFinite(input.valueAsNumber)) input.value = String(displayValue(control))
  finishInteraction(`numeric-${control}`)
}

function onNumericKeydown(control: Control, event: KeyboardEvent): void {
  if (event.key === 'Escape') {
    event.preventDefault()
    cancelInteraction(`numeric-${control}`)
    ;(event.currentTarget as HTMLInputElement).blur()
  } else if (event.key === 'Enter') {
    event.preventDefault()
    ;(event.currentTarget as HTMLInputElement).blur()
  }
}

const keyboardKeys = new Set([
  'ArrowLeft', 'ArrowRight', 'ArrowDown', 'ArrowUp',
  'PageDown', 'PageUp', 'Home', 'End',
])

function onRangeKeydown(control: Control, event: KeyboardEvent): void {
  if (!keyboardKeys.has(event.key)) return
  event.preventDefault()
  const source = `keyboard-${control}`
  beginInteraction(source)
  const rawStep = control === 'saturation' || control === 'lightness' ? 0.01 : 1
  const step = event.altKey ? rawStep / 10 : event.shiftKey ? rawStep * 10 : rawStep
  if (event.key === 'Home') setControl(control, controlDefault(control))
  else if (event.key === 'End') setControl(control, controlLimit(control, 'max'))
  else {
    const direction = event.key === 'ArrowLeft' || event.key === 'ArrowDown' || event.key === 'PageDown' ? -1 : 1
    const amount = event.key === 'PageDown' || event.key === 'PageUp' ? step * 10 : step
    setControl(control, rawValue(control) + direction * amount)
  }
}

function onRangeKeyup(control: Control, event: KeyboardEvent): void {
  if (keyboardKeys.has(event.key)) finishInteraction(`keyboard-${control}`)
}

function onRangeInput(control: Control, event: Event): void {
  const input = event.currentTarget as HTMLInputElement
  if (Number.isFinite(input.valueAsNumber)) {
    setControl(control, fromDisplayValue(control, input.valueAsNumber))
  }
}

function pointerAngle(event: PointerEvent): number | null {
  const element = ring.value
  if (!element) return null
  const rect = element.getBoundingClientRect()
  if (rect.width <= 0 || rect.height <= 0) return null
  const x = event.clientX - (rect.left + rect.width / 2)
  const y = event.clientY - (rect.top + rect.height / 2)
  return wrapDegrees(Math.atan2(y, x) * 180 / Math.PI + 90)
}

function onRingPointerMove(event: PointerEvent): void {
  if (event.pointerId !== pointerId) return
  const angle = pointerAngle(event)
  if (angle !== null) setControl('center', angle)
}

function cleanupPointer(): void {
  window.removeEventListener('pointermove', onRingPointerMove)
  window.removeEventListener('pointerup', onRingPointerUp)
  window.removeEventListener('pointercancel', onRingPointerCancel)
  pointerId = null
}

function cancelActivePointer(): void {
  if (pointerId === null) return
  cleanupPointer()
  cancelInteraction('ring-pointer')
}

function onRingPointerDown(event: PointerEvent): void {
  if (event.button !== 0) return
  cancelActivePointer()
  pointerId = event.pointerId
  beginInteraction('ring-pointer')
  ring.value?.setPointerCapture?.(event.pointerId)
  window.addEventListener('pointermove', onRingPointerMove)
  window.addEventListener('pointerup', onRingPointerUp)
  window.addEventListener('pointercancel', onRingPointerCancel)
  const angle = pointerAngle(event)
  if (angle !== null) setControl('center', angle)
  event.preventDefault()
}

function onRingPointerUp(event: PointerEvent): void {
  if (event.pointerId !== pointerId) return
  cleanupPointer()
  finishInteraction('ring-pointer')
}

function onRingPointerCancel(event: PointerEvent): void {
  if (event.pointerId !== pointerId) return
  cleanupPointer()
  cancelInteraction('ring-pointer')
}

function onLostPointerCapture(event: PointerEvent): void {
  if (event.pointerId === pointerId) onRingPointerCancel(event)
}

function onRingKeydown(event: KeyboardEvent): void {
  onRangeKeydown('center', event)
}

function onRingKeyup(event: KeyboardEvent): void {
  onRangeKeyup('center', event)
}

function reset(): void {
  beginInteraction('reset')
  update({
    selection: { ...HSL_SELECTIVE_DEFAULTS.selection },
    adjustment: { ...HSL_SELECTIVE_DEFAULTS.adjustment },
  })
  finishInteraction('reset')
}

function markerStyle(degrees: number, radius = 42): Record<string, string> {
  const radians = degrees * Math.PI / 180
  return {
    left: `${50 + Math.sin(radians) * radius}%`,
    top: `${50 - Math.cos(radians) * radius}%`,
  }
}

const coreArcLength = computed(() => Math.max(0.01, safeValue.value.selection.halfWidthDegrees * 2))
const featherArcLength = computed(() => Math.max(
  0.01,
  (safeValue.value.selection.halfWidthDegrees + safeValue.value.selection.featherDegrees) * 2,
))
const coreRotation = computed(() => safeValue.value.selection.centerDegrees - safeValue.value.selection.halfWidthDegrees - 90)
const featherRotation = computed(() => safeValue.value.selection.centerDegrees
  - safeValue.value.selection.halfWidthDegrees
  - safeValue.value.selection.featherDegrees
  - 90)
const coreStartStyle = computed(() => markerStyle(
  safeValue.value.selection.centerDegrees - safeValue.value.selection.halfWidthDegrees,
))
const coreEndStyle = computed(() => markerStyle(
  safeValue.value.selection.centerDegrees + safeValue.value.selection.halfWidthDegrees,
))
const centerStyle = computed(() => markerStyle(safeValue.value.selection.centerDegrees, 39))

onUnmounted(() => {
  if (pointerId !== null) {
    cancelActivePointer()
  } else {
    finishInteraction()
  }
})
</script>

<template>
  <fieldset class="hsl-selective-tool color-tool">
    <legend>Selective HSL</legend>
    <div class="color-tool-head">
      <div>
        <h3>Selective HSL</h3>
        <p>Выберите диапазон оттенка и скорректируйте только его.</p>
      </div>
      <span class="color-tool-badge">encoded sRGB</span>
    </div>

    <div class="hsl-selective-layout">
      <div class="hue-ring-column">
        <div
          ref="ring"
          class="hue-range-ring"
          role="slider"
          tabindex="0"
          aria-label="Центр диапазона оттенка"
          aria-valuemin="0"
          aria-valuemax="359"
          :aria-valuenow="Math.round(safeValue.selection.centerDegrees) % 360"
          :aria-valuetext="`Центр ${displayValue('center')}°, core ±${displayValue('halfWidth')}°, растушёвка ${displayValue('feather')}°`"
          :aria-describedby="`${instanceId}-hue-help`"
          @pointerdown="onRingPointerDown"
          @lostpointercapture="onLostPointerCapture"
          @keydown="onRingKeydown"
          @keyup="onRingKeyup"
          @blur="finishInteraction('keyboard-center')"
        >
          <svg class="hue-range-arcs" viewBox="0 0 100 100" aria-hidden="true">
            <circle
              class="hue-range-feather-arc"
              cx="50" cy="50" r="43" pathLength="360"
              :stroke-dasharray="`${featherArcLength} ${360 - featherArcLength}`"
              :style="{ transform: `rotate(${featherRotation}deg)` }"
            />
            <circle
              class="hue-range-core-arc"
              cx="50" cy="50" r="43" pathLength="360"
              :stroke-dasharray="`${coreArcLength} ${360 - coreArcLength}`"
              :style="{ transform: `rotate(${coreRotation}deg)` }"
            />
          </svg>
          <span class="hue-range-boundary" :style="coreStartStyle" aria-hidden="true"></span>
          <span class="hue-range-boundary" :style="coreEndStyle" aria-hidden="true"></span>
          <span class="hue-range-center" :style="centerStyle" aria-hidden="true"></span>
        </div>
        <p :id="`${instanceId}-hue-help`" class="color-wheel-help">
          Стрелки ±1° · Shift ±10° · Alt ±0,1° · Home — 0°
        </p>
      </div>

      <div class="hsl-selection-controls">
        <label v-for="control in controls" :key="control.key" class="hsl-control-row">
          <span>{{ control.label }}</span>
          <input
            v-if="control.key !== 'center'"
            type="range"
            :min="control.min"
            :max="control.key === 'feather' ? controlLimit('feather', 'max') : control.max"
            :step="control.step"
            :value="displayValue(control.key)"
            :aria-label="control.label"
            @pointerdown="beginInteraction(`range-${control.key}`)"
            @pointerup="finishInteraction(`range-${control.key}`)"
            @pointercancel="cancelInteraction(`range-${control.key}`)"
            @lostpointercapture="cancelInteraction(`range-${control.key}`)"
            @input="onRangeInput(control.key, $event)"
            @keydown="onRangeKeydown(control.key, $event)"
            @keyup="onRangeKeyup(control.key, $event)"
            @blur="finishInteraction(`keyboard-${control.key}`)"
          />
          <span v-else class="hsl-center-summary" aria-hidden="true">Круговой выбор</span>
          <span class="hsl-number-wrap">
            <input
              type="number"
              :min="control.min"
              :max="control.key === 'feather' ? controlLimit('feather', 'max') : control.max"
              :step="control.key === 'center' ? 0.1 : control.step"
              :value="displayValue(control.key)"
              :aria-label="`${control.label}, числом`"
              @focus="onNumericFocus(control.key)"
              @input="onNumericInput(control.key, $event)"
              @keydown="onNumericKeydown(control.key, $event)"
              @blur="onNumericBlur(control.key, $event)"
            />
            <span aria-hidden="true">{{ control.unit }}</span>
          </span>
        </label>
      </div>
    </div>

    <div class="hsl-adjustment-grid">
      <label v-for="control in adjustments" :key="control.key" class="hsl-control-row">
        <span>{{ control.label }}</span>
        <input
          type="range"
          :min="control.min"
          :max="control.max"
          :step="control.step"
          :value="displayValue(control.key)"
          :aria-label="control.label"
          @pointerdown="beginInteraction(`range-${control.key}`)"
          @pointerup="finishInteraction(`range-${control.key}`)"
          @pointercancel="cancelInteraction(`range-${control.key}`)"
          @lostpointercapture="cancelInteraction(`range-${control.key}`)"
          @input="onRangeInput(control.key, $event)"
          @keydown="onRangeKeydown(control.key, $event)"
          @keyup="onRangeKeyup(control.key, $event)"
          @blur="finishInteraction(`keyboard-${control.key}`)"
        />
        <span class="hsl-number-wrap">
          <input
            type="number"
            :min="control.min"
            :max="control.max"
            :step="control.step"
            :value="displayValue(control.key)"
            :aria-label="`${control.label}, числом`"
            @focus="onNumericFocus(control.key)"
            @input="onNumericInput(control.key, $event)"
            @keydown="onNumericKeydown(control.key, $event)"
            @blur="onNumericBlur(control.key, $event)"
          />
          <span aria-hidden="true">{{ control.unit }}</span>
        </span>
      </label>
    </div>

    <div class="hsl-selective-actions">
      <button
        type="button"
        class="btn ghost sm"
        :class="{ active: maskPreview }"
        :aria-pressed="maskPreview"
        aria-controls="color-preview-surface"
        @click="$emit('update:maskPreview', !maskPreview)"
      >
        {{ maskPreview ? 'Скрыть маску' : 'Показать маску' }}
      </button>
      <button type="button" class="btn ghost sm" @click="reset">Сбросить Selective HSL</button>
    </div>
    <p v-if="maskPreview" class="advanced-color-note hsl-mask-note" role="status">
      Маска: белое выбрано, серое — растушёвка, чёрное исключено. Предпросмотр поставлен на паузу.
    </p>
  </fieldset>
</template>

<style scoped>
.hsl-selective-tool {
  min-width: 0;
  margin: 0;
  padding: 13px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
}

.hsl-selective-tool > legend {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}

.hsl-selective-layout {
  display: grid;
  grid-template-columns: minmax(132px, 164px) minmax(0, 1fr);
  gap: 14px;
  align-items: center;
  margin-top: 10px;
}

.hue-ring-column {
  min-width: 0;
}

.hue-range-ring {
  position: relative;
  width: min(100%, 164px);
  aspect-ratio: 1;
  margin: 0 auto;
  border: 2px solid var(--border-strong);
  border-radius: 50%;
  background: conic-gradient(from 0deg, #f33, #ff3, #3f3, #3ff, #33f, #f3f, #f33);
  box-shadow: inset 0 0 0 28px var(--panel-2), inset 0 0 16px 29px rgb(0 0 0 / 32%);
  cursor: crosshair;
  touch-action: none;
}

.hue-range-ring:focus-visible {
  outline: 3px solid color-mix(in srgb, var(--accent) 65%, transparent);
  outline-offset: 3px;
}

.hue-range-arcs {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  overflow: visible;
  pointer-events: none;
}

.hue-range-arcs circle {
  fill: none;
  transform-origin: 50px 50px;
  stroke-linecap: butt;
}

.hue-range-feather-arc {
  stroke: rgb(255 255 255 / 45%);
  stroke-width: 14;
}

.hue-range-core-arc {
  stroke: white;
  stroke-width: 8;
}

.hue-range-boundary,
.hue-range-center {
  position: absolute;
  border: 2px solid white;
  border-radius: 50%;
  background: var(--panel);
  box-shadow: 0 1px 5px rgb(0 0 0 / 70%);
  transform: translate(-50%, -50%);
  pointer-events: none;
}

.hue-range-boundary {
  width: 8px;
  height: 8px;
}

.hue-range-center {
  width: 14px;
  height: 14px;
  background: var(--accent);
}

.hsl-selection-controls,
.hsl-adjustment-grid {
  display: grid;
  gap: 9px;
}

.hsl-adjustment-grid {
  margin-top: 12px;
}

.hsl-control-row {
  display: grid;
  grid-template-columns: minmax(116px, 0.8fr) minmax(90px, 1fr) 88px;
  gap: 8px;
  align-items: center;
  color: var(--muted);
  font-size: 11px;
}

.hsl-control-row > span:first-child {
  color: var(--text);
}

.hsl-center-summary {
  font-size: 10px;
}

.hsl-number-wrap {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 4px;
  align-items: center;
}

.hsl-number-wrap input {
  min-width: 0;
  width: 100%;
  box-sizing: border-box;
  padding: 6px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--panel);
  color: var(--text);
  font-variant-numeric: tabular-nums;
}

.hsl-number-wrap input:focus {
  border-color: var(--accent);
  outline: none;
}

.hsl-selective-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 12px;
}

.hsl-selective-actions .active {
  border-color: var(--accent);
  color: var(--accent);
}

.hsl-mask-note {
  margin-bottom: 0;
}

@media (max-width: 620px) {
  .hsl-selective-layout {
    grid-template-columns: 1fr;
  }

  .hsl-control-row {
    grid-template-columns: minmax(105px, 0.8fr) minmax(70px, 1fr) 82px;
  }
}

@media (forced-colors: active) {
  .hue-range-ring {
    background: Canvas;
    box-shadow: inset 0 0 0 28px Canvas;
  }

  .hue-range-feather-arc {
    stroke: GrayText;
  }

  .hue-range-core-arc,
  .hue-range-boundary {
    stroke: Highlight;
    border-color: Highlight;
  }

  .hue-range-center {
    border-color: Highlight;
    background: Highlight;
  }
}
</style>
