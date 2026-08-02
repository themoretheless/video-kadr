<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, useId, watch } from 'vue'
import {
  cloneCurves,
  identityCurve,
  isIdentityCurve,
  MAX_CURVE_POINTS,
  sampleCurvePchip,
  sanitizeCurve,
} from '../../domain/edit'
import type { ColorCurves, CurvePoint } from '../../types'

type CurveChannel = keyof ColorCurves
type InteractionSource = 'pointer' | 'numeric'

interface ChannelOption {
  key: CurveChannel
  label: string
  longLabel: string
  color: string
}

interface PointerDrag {
  channel: CurveChannel
  index: number
  pointerId: number
  points: CurvePoint[]
}

const props = defineProps<{
  modelValue: ColorCurves
}>()

const emit = defineEmits<{
  'update:modelValue': [ColorCurves]
  'interaction-start': []
  'interaction-end': []
}>()

const CHANNELS: ChannelOption[] = [
  { key: 'master', label: 'Общая', longLabel: 'Общая', color: 'var(--text)' },
  { key: 'red', label: 'R', longLabel: 'Красная (R)', color: '#ff626c' },
  { key: 'green', label: 'G', longLabel: 'Зелёная (G)', color: '#3ecf8e' },
  { key: 'blue', label: 'B', longLabel: 'Синяя (B)', color: '#5d94ff' },
]

const VALUE_MIN = 0
const VALUE_MAX = 255
const VIEW_SIZE = 256
const PLOT_PADDING = 8
const PLOT_SIZE = VIEW_SIZE - PLOT_PADDING * 2
const GRID_TICKS = [64, 128, 192]

const instanceId = useId().replace(/:/g, '')
const tabList = ref<HTMLElement | null>(null)
const plot = ref<SVGSVGElement | null>(null)
const selectedXInput = ref<HTMLInputElement | null>(null)
const activeChannel = ref<CurveChannel>('master')
const selectedIndex = ref(0)
const interactionSources = new Set<InteractionSource>()

let pointerDrag: PointerDrag | null = null

const activeOption = computed(
  () => CHANNELS.find((channel) => channel.key === activeChannel.value) ?? CHANNELS[0]!,
)
const activePoints = computed(() => sanitizeCurve(props.modelValue[activeChannel.value]))
const selectedPoint = computed(
  () => activePoints.value[selectedIndex.value] ?? activePoints.value[0]!,
)
const selectedIsEndpoint = computed(
  () => selectedIndex.value === 0 || selectedIndex.value === activePoints.value.length - 1,
)
const selectedXMin = computed(() => activePoints.value[selectedIndex.value - 1]?.x + 1 || VALUE_MIN)
const selectedXMax = computed(
  () => activePoints.value[selectedIndex.value + 1]?.x - 1 || VALUE_MAX,
)
const polylinePoints = computed(() =>
  sampleCurvePchip(activePoints.value)
    .map((point) => `${toSvgX(point.x)},${toSvgY(point.y)}`)
    .join(' '),
)
const isActiveIdentity = computed(() => isIdentityCurve(activePoints.value))

watch(
  () => activePoints.value.length,
  (length) => {
    selectedIndex.value = clamp(selectedIndex.value, 0, Math.max(0, length - 1))
  },
)

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(value, max))
}

function clampValue(value: number): number {
  return Math.round(clamp(value, VALUE_MIN, VALUE_MAX))
}

function clonePoints(points: CurvePoint[]): CurvePoint[] {
  return points.map((point) => ({ ...point }))
}

function toSvgX(value: number): number {
  return PLOT_PADDING + (value / VALUE_MAX) * PLOT_SIZE
}

function toSvgY(value: number): number {
  return PLOT_PADDING + ((VALUE_MAX - value) / VALUE_MAX) * PLOT_SIZE
}

function pointFromPointer(event: PointerEvent): CurvePoint | null {
  const element = plot.value
  if (!element) return null
  const rect = element.getBoundingClientRect()
  if (rect.width <= 0 || rect.height <= 0) return null

  const svgX = ((event.clientX - rect.left) / rect.width) * VIEW_SIZE
  const svgY = ((event.clientY - rect.top) / rect.height) * VIEW_SIZE
  return {
    x: clampValue(((svgX - PLOT_PADDING) / PLOT_SIZE) * VALUE_MAX),
    y: clampValue(VALUE_MAX - ((svgY - PLOT_PADDING) / PLOT_SIZE) * VALUE_MAX),
  }
}

function tabId(channel: CurveChannel): string {
  return `${instanceId}-curve-tab-${channel}`
}

function panelId(channel: CurveChannel): string {
  return `${instanceId}-curve-panel-${channel}`
}

function beginInteraction(source: InteractionSource): void {
  if (interactionSources.has(source)) return
  if (interactionSources.size === 0) emit('interaction-start')
  interactionSources.add(source)
}

function endInteraction(source: InteractionSource): void {
  if (!interactionSources.delete(source)) return
  if (interactionSources.size === 0) emit('interaction-end')
}

function endAllInteractions(): void {
  if (interactionSources.size === 0) return
  interactionSources.clear()
  emit('interaction-end')
}

function discreteUpdate(update: () => void): void {
  const wasIdle = interactionSources.size === 0
  if (wasIdle) emit('interaction-start')
  update()
  if (wasIdle) emit('interaction-end')
}

function replaceChannel(channel: CurveChannel, points: CurvePoint[]): void {
  const next = cloneCurves(props.modelValue)
  next[channel] = clonePoints(points)
  emit('update:modelValue', next)
}

function constrainPoint(
  points: CurvePoint[],
  index: number,
  requestedX: number,
  requestedY: number,
): CurvePoint {
  const current = points[index]
  if (!current) return { x: VALUE_MIN, y: VALUE_MIN }

  const endpoint = index === 0 || index === points.length - 1
  const minX = points[index - 1] ? points[index - 1]!.x + 1 : current.x
  const maxX = points[index + 1] ? points[index + 1]!.x - 1 : current.x
  return {
    x: endpoint ? current.x : clamp(clampValue(requestedX), minX, maxX),
    y: clampValue(requestedY),
  }
}

function updatedPoints(
  points: CurvePoint[],
  index: number,
  requestedX: number,
  requestedY: number,
): CurvePoint[] {
  const next = clonePoints(points)
  next[index] = constrainPoint(next, index, requestedX, requestedY)
  return next
}

function selectChannel(channel: CurveChannel): void {
  if (channel === activeChannel.value) return
  stopPointerDrag()
  endAllInteractions()
  activeChannel.value = channel
  selectedIndex.value = 0
}

function onTabKeydown(event: KeyboardEvent, index: number): void {
  let nextIndex: number | null = null
  if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
    nextIndex = (index + 1) % CHANNELS.length
  } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
    nextIndex = (index - 1 + CHANNELS.length) % CHANNELS.length
  } else if (event.key === 'Home') {
    nextIndex = 0
  } else if (event.key === 'End') {
    nextIndex = CHANNELS.length - 1
  }
  if (nextIndex === null) return

  event.preventDefault()
  selectChannel(CHANNELS[nextIndex]!.key)
  void nextTick(() => {
    tabList.value
      ?.querySelectorAll<HTMLButtonElement>('[role="tab"]')
      .item(nextIndex!)
      .focus()
  })
}

function startPointerDrag(
  index: number,
  event: PointerEvent,
  points: CurvePoint[] = clonePoints(activePoints.value),
): void {
  if (event.button !== 0) return
  stopPointerDrag()
  selectedIndex.value = index
  pointerDrag = {
    channel: activeChannel.value,
    index,
    pointerId: event.pointerId,
    points: clonePoints(points),
  }
  beginInteraction('pointer')
  plot.value?.setPointerCapture?.(event.pointerId)
  window.addEventListener('pointermove', onPointerMove)
  window.addEventListener('pointerup', onPointerUp)
  window.addEventListener('pointercancel', onPointerUp)
  event.preventDefault()
}

function onPlotPointerDown(event: PointerEvent): void {
  if (event.button !== 0) return
  const point = pointFromPointer(event)
  if (!point) return

  const points = clonePoints(activePoints.value)
  let index = points.findIndex((candidate) => candidate.x === point.x)
  let added = false
  if (index < 0) {
    if (points.length >= MAX_CURVE_POINTS) return
    points.push(point)
    points.sort((left, right) => left.x - right.x)
    index = points.findIndex((candidate) => candidate.x === point.x)
    added = true
  }

  startPointerDrag(index, event, points)
  if (added) replaceChannel(activeChannel.value, points)
}

function onPointerMove(event: PointerEvent): void {
  const drag = pointerDrag
  if (!drag || event.pointerId !== drag.pointerId) return
  const requested = pointFromPointer(event)
  if (!requested) return

  const current = drag.points[drag.index]
  if (!current) return
  const points = updatedPoints(drag.points, drag.index, requested.x, requested.y)
  const next = points[drag.index]!
  if (current.x === next.x && current.y === next.y) return

  drag.points = points
  replaceChannel(drag.channel, points)
}

function onPointerUp(event: PointerEvent): void {
  if (!pointerDrag || event.pointerId !== pointerDrag.pointerId) return
  stopPointerDrag()
}

function stopPointerDrag(): void {
  const drag = pointerDrag
  if (!drag) return
  pointerDrag = null
  window.removeEventListener('pointermove', onPointerMove)
  window.removeEventListener('pointerup', onPointerUp)
  window.removeEventListener('pointercancel', onPointerUp)
  if (plot.value?.hasPointerCapture?.(drag.pointerId)) {
    plot.value.releasePointerCapture(drag.pointerId)
  }
  endInteraction('pointer')
}

function onPointKeydown(index: number, event: KeyboardEvent): void {
  selectedIndex.value = index
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault()
    return
  }
  if (event.key === 'Delete' || event.key === 'Backspace') {
    if (index === 0 || index === activePoints.value.length - 1) return
    event.preventDefault()
    discreteUpdate(() => {
      const points = clonePoints(activePoints.value)
      points.splice(index, 1)
      selectedIndex.value = Math.min(index, points.length - 1)
      replaceChannel(activeChannel.value, points)
    })
  }
}

function updateSelectedCoordinate(axis: 'x' | 'y', event: Event): void {
  const input = event.currentTarget as HTMLInputElement
  if (!Number.isFinite(input.valueAsNumber)) return
  const points = clonePoints(activePoints.value)
  const point = points[selectedIndex.value]
  if (!point) return

  const requestedX = axis === 'x' ? input.valueAsNumber : point.x
  const requestedY = axis === 'y' ? input.valueAsNumber : point.y
  replaceChannel(
    activeChannel.value,
    updatedPoints(points, selectedIndex.value, requestedX, requestedY),
  )
}

function blurOnEnter(event: KeyboardEvent): void {
  if (event.key === 'Enter') (event.currentTarget as HTMLInputElement).blur()
}

function deleteSelectedPoint(): void {
  const index = selectedIndex.value
  if (index === 0 || index === activePoints.value.length - 1) return
  discreteUpdate(() => {
    const points = clonePoints(activePoints.value)
    points.splice(index, 1)
    selectedIndex.value = Math.min(index, points.length - 1)
    replaceChannel(activeChannel.value, points)
  })
}

function addKeyboardPoint(): void {
  const points = clonePoints(activePoints.value)
  if (points.length >= MAX_CURVE_POINTS) return

  let gapIndex = 0
  for (let index = 1; index < points.length - 1; index += 1) {
    if (points[index + 1]!.x - points[index]!.x > points[gapIndex + 1]!.x - points[gapIndex]!.x) {
      gapIndex = index
    }
  }
  const left = points[gapIndex]!
  const right = points[gapIndex + 1]!
  if (right.x - left.x <= 1) return
  const x = Math.round((left.x + right.x) / 2)
  const ratio = (x - left.x) / (right.x - left.x)
  const y = clampValue(left.y + (right.y - left.y) * ratio)

  discreteUpdate(() => {
    points.splice(gapIndex + 1, 0, { x, y })
    selectedIndex.value = gapIndex + 1
    replaceChannel(activeChannel.value, points)
  })
  void nextTick(() => selectedXInput.value?.focus())
}

function resetActiveChannel(): void {
  if (isActiveIdentity.value) return
  discreteUpdate(() => {
    selectedIndex.value = 0
    replaceChannel(activeChannel.value, identityCurve())
  })
}

onUnmounted(() => {
  stopPointerDrag()
  endAllInteractions()
})
</script>

<template>
  <div class="curves-editor">
    <div ref="tabList" class="curve-tabs" role="tablist" aria-label="Канал кривой">
      <button
        v-for="(channel, index) in CHANNELS"
        :id="tabId(channel.key)"
        :key="channel.key"
        type="button"
        class="curve-tab"
        :class="`channel-${channel.key}`"
        role="tab"
        :aria-controls="panelId(channel.key)"
        :aria-selected="activeChannel === channel.key"
        :aria-label="channel.longLabel"
        :tabindex="activeChannel === channel.key ? 0 : -1"
        @click="selectChannel(channel.key)"
        @keydown="onTabKeydown($event, index)"
      >
        <span class="channel-swatch" aria-hidden="true"></span>
        {{ channel.label }}
      </button>
    </div>

    <div
      :id="panelId(activeChannel)"
      class="curve-panel"
      role="tabpanel"
      :aria-labelledby="tabId(activeChannel)"
    >
      <p :id="`${instanceId}-curve-help`" class="visually-hidden">
        Нажмите на график или кнопку «Добавить точку». Выберите точку и измените её координаты в
        полях «Вход» и «Выход». Клавиши Delete или Backspace удаляют внутреннюю точку.
      </p>

      <div class="curve-toolbar">
        <span class="point-count" aria-live="polite">
          {{ activePoints.length }} / {{ MAX_CURVE_POINTS }} точек
        </span>
        <button
          type="button"
          class="curve-action"
          :disabled="activePoints.length >= MAX_CURVE_POINTS"
          @click="addKeyboardPoint"
        >
          Добавить точку
        </button>
        <button
          type="button"
          class="curve-action"
          :disabled="selectedIsEndpoint"
          @click="deleteSelectedPoint"
        >
          Удалить точку
        </button>
        <button
          type="button"
          class="curve-action"
          :disabled="isActiveIdentity"
          @click="resetActiveChannel"
        >
          Сбросить: {{ activeOption.label }}
        </button>
      </div>

      <div class="curve-plot-frame">
        <svg
          ref="plot"
          class="curve-plot"
          :style="{ color: activeOption.color }"
          viewBox="0 0 256 256"
          role="group"
          :aria-label="`${activeOption.longLabel} тоновая кривая`"
          :aria-describedby="`${instanceId}-curve-help`"
          @pointerdown="onPlotPointerDown"
        >
          <rect
            class="plot-background"
            :x="PLOT_PADDING"
            :y="PLOT_PADDING"
            :width="PLOT_SIZE"
            :height="PLOT_SIZE"
            rx="2"
          />
          <g class="plot-guides" aria-hidden="true">
            <template v-for="tick in GRID_TICKS" :key="tick">
              <line
                :x1="toSvgX(tick)"
                :x2="toSvgX(tick)"
                :y1="PLOT_PADDING"
                :y2="PLOT_PADDING + PLOT_SIZE"
              />
              <line
                :x1="PLOT_PADDING"
                :x2="PLOT_PADDING + PLOT_SIZE"
                :y1="toSvgY(tick)"
                :y2="toSvgY(tick)"
              />
            </template>
            <line
              class="identity-line"
              :x1="toSvgX(VALUE_MIN)"
              :y1="toSvgY(VALUE_MIN)"
              :x2="toSvgX(VALUE_MAX)"
              :y2="toSvgY(VALUE_MAX)"
            />
          </g>
          <polyline class="curve-line" :points="polylinePoints" aria-hidden="true" />
          <g
            v-for="(point, index) in activePoints"
            :key="`${point.x}-${index}`"
            class="curve-point"
            :class="{ selected: selectedIndex === index }"
            :transform="`translate(${toSvgX(point.x)} ${toSvgY(point.y)})`"
            role="button"
            tabindex="0"
            :aria-label="`${activeOption.longLabel}: точка ${index + 1}, вход ${point.x}, выход ${point.y}`"
            @focus="selectedIndex = index"
            @pointerdown.stop="startPointerDrag(index, $event)"
            @keydown="onPointKeydown(index, $event)"
          >
            <circle class="point-hit-area" r="12" />
            <circle class="point-focus-ring" r="8" />
            <circle class="point-dot" r="4.5" />
          </g>
        </svg>
      </div>

      <div class="point-controls" aria-label="Координаты выбранной точки кривой">
        <label>
          <span>Вход (X)</span>
          <input
            ref="selectedXInput"
            type="number"
            inputmode="numeric"
            step="1"
            :min="selectedXMin"
            :max="selectedXMax"
            :value="selectedPoint.x"
            :disabled="selectedIsEndpoint"
            @focus="beginInteraction('numeric')"
            @change="updateSelectedCoordinate('x', $event)"
            @keydown="blurOnEnter"
            @blur="endInteraction('numeric')"
          />
        </label>
        <label>
          <span>Выход (Y)</span>
          <input
            type="number"
            inputmode="numeric"
            step="1"
            :min="VALUE_MIN"
            :max="VALUE_MAX"
            :value="selectedPoint.y"
            @focus="beginInteraction('numeric')"
            @change="updateSelectedCoordinate('y', $event)"
            @keydown="blurOnEnter"
            @blur="endInteraction('numeric')"
          />
        </label>
      </div>
    </div>
  </div>
</template>

<style scoped>
.curves-editor {
  display: grid;
  gap: 10px;
  min-width: 0;
}

.curve-tabs {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 6px;
}

.curve-tab,
.curve-action {
  min-height: 36px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--panel-2);
  color: var(--text);
  font: inherit;
  cursor: pointer;
}

.curve-tab {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  padding: 6px 8px;
  font-size: 0.86rem;
  font-weight: 650;
}

.curve-tab:hover,
.curve-action:hover:not(:disabled) {
  border-color: var(--border-strong);
}

.curve-tab[aria-selected='true'] {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.channel-swatch {
  width: 8px;
  height: 8px;
  flex: none;
  border-radius: 50%;
  background: var(--text);
}

.channel-red .channel-swatch {
  background: #ff626c;
}

.channel-green .channel-swatch {
  background: #3ecf8e;
}

.channel-blue .channel-swatch {
  background: #5d94ff;
}

.curve-panel {
  display: grid;
  gap: 10px;
  min-width: 0;
}

.curve-toolbar {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 7px;
  flex-wrap: wrap;
}

.point-count {
  margin-right: auto;
  color: var(--muted);
  font-size: 0.78rem;
  font-variant-numeric: tabular-nums;
}

.curve-action {
  min-height: 32px;
  padding: 5px 10px;
  background: transparent;
  font-size: 0.78rem;
}

.curve-action:disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

.curve-plot-frame {
  width: min(100%, 360px);
  margin-inline: auto;
  overflow: hidden;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius);
  background: var(--panel-3);
}

.curve-plot {
  display: block;
  width: 100%;
  height: auto;
  aspect-ratio: 1;
  touch-action: none;
  user-select: none;
  cursor: crosshair;
}

.plot-background {
  fill: var(--panel-2);
  stroke: var(--border-strong);
  vector-effect: non-scaling-stroke;
}

.plot-guides {
  pointer-events: none;
}

.plot-guides line {
  stroke: var(--border);
  stroke-width: 1;
  vector-effect: non-scaling-stroke;
}

.plot-guides .identity-line {
  stroke: var(--faint);
  stroke-dasharray: 4 4;
}

.curve-line {
  fill: none;
  stroke: currentColor;
  stroke-width: 2.25;
  stroke-linecap: round;
  stroke-linejoin: round;
  pointer-events: none;
  vector-effect: non-scaling-stroke;
}

.curve-point {
  color: inherit;
  cursor: grab;
  outline: none;
}

.curve-point:active {
  cursor: grabbing;
}

.point-hit-area {
  fill: transparent;
  pointer-events: all;
}

.point-focus-ring {
  fill: none;
  stroke: var(--accent);
  stroke-width: 2;
  opacity: 0;
  pointer-events: none;
  vector-effect: non-scaling-stroke;
}

.point-dot {
  fill: var(--panel-2);
  stroke: currentColor;
  stroke-width: 2;
  pointer-events: none;
  vector-effect: non-scaling-stroke;
}

.curve-point.selected .point-dot {
  fill: currentColor;
  stroke: var(--panel-2);
}

.curve-point:focus-visible .point-focus-ring {
  opacity: 1;
}

.point-controls {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
}

.point-controls label {
  display: grid;
  grid-template-columns: auto minmax(72px, 1fr);
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-size: 0.82rem;
}

.point-controls input {
  width: 100%;
  min-width: 0;
  min-height: 36px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--panel-2);
  color: var(--text);
  padding: 6px 8px;
  font: inherit;
  font-variant-numeric: tabular-nums;
}

.point-controls input:disabled {
  color: var(--faint);
  cursor: not-allowed;
  opacity: 0.65;
}

.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}

@media (max-width: 440px) {
  .point-controls {
    grid-template-columns: 1fr;
  }

  .curve-action {
    flex: 1 1 auto;
  }
}
</style>
