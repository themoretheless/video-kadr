<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, useId, watch, type ComponentPublicInstance } from 'vue'
import type { VideoScopeKind, VideoScopesResult } from '../domain/video-scopes'
import {
  loadVideoScopesPreferences,
  sanitizeVideoScopesPreferences,
  saveVideoScopesPreferences,
  VIDEO_SCOPE_KINDS,
  type VideoScopesPreferences,
  type VideoScopesStage,
} from '../video-scopes/preferences'
import {
  VIDEO_SCOPE_LABELS,
  VIDEO_SCOPE_SUMMARY_COLUMNS,
  VIDEO_SCOPES_STATUS_LABELS,
  videoScopeSummaryRows,
  type VideoScopesPresentationStatus,
} from '../video-scopes/presentation'
import { useVideoScopes, type ScopeAnalysisResult, type VideoScopeStatus } from '../video-scopes/use-video-scopes'

const props = defineProps<{
  result?: VideoScopesResult | null
  status?: VideoScopesPresentationStatus
  preferences?: VideoScopesPreferences
}>()

const emit = defineEmits<{
  'update:preferences': [VideoScopesPreferences]
}>()

const instanceId = useId().replace(/:/g, '')
const localPreferences = ref(loadVideoScopesPreferences())
const controller = useVideoScopes()
const controlledPresentation = computed(() => props.result !== undefined || props.status !== undefined)
const effectivePreferences = computed(() => props.preferences
  ? sanitizeVideoScopesPreferences(props.preferences)
  : localPreferences.value)
const visibleScopes = computed(() => effectivePreferences.value.visibleScopes)
const panelElement = ref<HTMLElement | null>(null)
const canvasByScope = new Map<VideoScopeKind, HTMLCanvasElement>()
const densityRasterByKey = new Map<string, {
  canvas: HTMLCanvasElement
  context: CanvasRenderingContext2D
  image: ImageData
}>()
let resizeObserver: ResizeObserver | null = null
let intersectionObserver: IntersectionObserver | null = null

function controllerResult(result: ScopeAnalysisResult | null): VideoScopesResult | null {
  if (!result || result.frameIdentity.tapId !== controller.state.tapId) return null
  const { width, height } = result.frameIdentity
  const sampledAxis = (dimension: number) => Math.max(
    0,
    Math.ceil((dimension - Math.floor(result.stride / 2)) / result.stride),
  )
  return {
    schemaVersion: 1,
    descriptor: 'straight-rgba8-encoded-srgb',
    sourceWidth: width,
    sourceHeight: height,
    stride: result.stride,
    sampledColumns: sampledAxis(width),
    sampledRows: sampledAxis(height),
    sampledPixels: result.sampleCount,
    alphaWeight: result.alphaWeight,
    histogram: result.histogram,
    waveform: result.waveform,
    parade: result.parade,
    vectorscope: result.vectorscope,
  }
}

function controllerStatus(status: VideoScopeStatus): VideoScopesPresentationStatus {
  const sourceMode = controller.state.frameIdentity?.sourceMode
  switch (status) {
    case 'disabled': return { kind: 'idle' }
    case 'waiting': return { kind: 'idle', detail: 'Ожидание точного кадра', sourceMode }
    case 'analyzing': return { kind: 'analyzing', detail: 'Анализирую последний кадр', sourceMode }
    case 'ready': return {
      kind: controller.state.accuracy === 'exact-live'
        ? 'live'
        : controller.state.accuracy === 'exact-paused' ? 'exact' : 'analyzing',
      detail: controller.state.accuracy === 'exact-live'
        ? 'Точный live-кадр с ограниченной выборкой'
        : controller.state.accuracy === 'exact-paused' ? 'Точный остановленный кадр' : 'Уточняю режим кадра',
      sourceMode,
    }
    case 'held': return { kind: 'last-exact', detail: controller.state.reason, sourceMode }
    case 'unavailable': return { kind: 'unavailable', detail: controller.state.reason, sourceMode }
    case 'error': return { kind: 'error', detail: controller.state.reason, sourceMode }
  }
}

const displayedResult = computed(() => props.result !== undefined
  ? props.result
  : controllerResult(controller.state.result))
const displayedStatus = computed(() => props.status ?? controllerStatus(controller.state.status))

function updatePreferences(patch: Partial<Omit<VideoScopesPreferences, 'schemaVersion'>>): void {
  const next = sanitizeVideoScopesPreferences({
    ...effectivePreferences.value,
    ...patch,
  })
  localPreferences.value = next
  saveVideoScopesPreferences(next)
  emit('update:preferences', next)
}

function setStage(stage: VideoScopesStage): void {
  updatePreferences({ stage })
}

function toggleScope(scope: VideoScopeKind, checked: boolean): void {
  const selection = new Set(effectivePreferences.value.visibleScopes)
  if (checked) selection.add(scope)
  else selection.delete(scope)
  updatePreferences({ visibleScopes: VIDEO_SCOPE_KINDS.filter(candidate => selection.has(candidate)) })
}

function setIntensity(event: Event): void {
  const value = (event.currentTarget as HTMLInputElement).valueAsNumber
  if (Number.isFinite(value)) updatePreferences({ intensity: value })
}

function bindCanvas(scope: VideoScopeKind, value: Element | ComponentPublicInstance | null): void {
  const previous = canvasByScope.get(scope)
  if (previous) resizeObserver?.unobserve(previous)
  if (value instanceof HTMLCanvasElement) {
    canvasByScope.set(scope, value)
    resizeObserver?.observe(value)
  } else {
    canvasByScope.delete(scope)
  }
}

function contextFor(canvas: HTMLCanvasElement): {
  context: CanvasRenderingContext2D
  width: number
  height: number
} | null {
  const cssWidth = Math.max(220, Math.round(canvas.getBoundingClientRect().width || canvas.clientWidth || 320))
  const cssHeight = Math.max(150, Math.round(canvas.getBoundingClientRect().height || canvas.clientHeight || 180))
  const ratio = Math.max(1, Math.min(2, window.devicePixelRatio || 1))
  const width = Math.round(cssWidth * ratio)
  const height = Math.round(cssHeight * ratio)
  if (canvas.width !== width) canvas.width = width
  if (canvas.height !== height) canvas.height = height
  try {
    const context = canvas.getContext('2d')
    return context ? { context, width, height } : null
  } catch {
    return null
  }
}

function drawGrid(context: CanvasRenderingContext2D, width: number, height: number): void {
  context.save()
  context.strokeStyle = 'rgba(153, 176, 199, 0.22)'
  context.lineWidth = 1
  for (const amount of [0.25, 0.5, 0.75]) {
    context.beginPath()
    context.moveTo(0, Math.round(height * amount) + 0.5)
    context.lineTo(width, Math.round(height * amount) + 0.5)
    context.stroke()
    context.beginPath()
    context.moveTo(Math.round(width * amount) + 0.5, 0)
    context.lineTo(Math.round(width * amount) + 0.5, height)
    context.stroke()
  }
  context.restore()
}

function drawLabels(
  context: CanvasRenderingContext2D,
  scope: VideoScopeKind,
  width: number,
  height: number,
): void {
  context.save()
  context.fillStyle = 'rgba(231, 237, 245, 0.82)'
  context.font = `${Math.max(11, Math.round(height / 18))}px system-ui, sans-serif`
  context.textBaseline = 'top'
  if (scope === 'parade') {
    for (const [index, label] of ['R', 'G', 'B'].entries()) {
      context.fillText(label, (index + 0.5) * width / 3 - 4, 6)
    }
  } else if (scope === 'histogram') {
    const colors = ['Y′', 'R', 'G', 'B']
    colors.forEach((label, index) => context.fillText(label, 8 + index * 24, 6))
  } else if (scope === 'vectorscope') {
    const targets: Array<[string, number, number]> = [
      ['R', 0.88, 0.50], ['M', 0.76, 0.16], ['B', 0.30, 0.12],
      ['C', 0.10, 0.52], ['G', 0.28, 0.86], ['Y', 0.74, 0.86],
    ]
    targets.forEach(([label, x, y]) => context.fillText(label, x * width, y * height))
  } else {
    context.fillText('Y′', 8, 6)
  }
  context.restore()
}

function peak(values: Uint32Array): number {
  let maximum = 0
  for (const value of values) maximum = Math.max(maximum, value)
  return maximum
}

function densityImage(
  key: string,
  values: Uint32Array,
  width: number,
  height: number,
  color: readonly [number, number, number],
  intensity: number,
  layout: 'row-major' | 'x-major' = 'row-major',
): HTMLCanvasElement | null {
  const maximum = peak(values)
  if (!maximum || values.length !== width * height) return null
  let raster = densityRasterByKey.get(key)
  if (!raster || raster.canvas.width !== width || raster.canvas.height !== height) {
    const canvas = document.createElement('canvas')
    canvas.width = width
    canvas.height = height
    const context = canvas.getContext('2d')
    if (!context) return null
    raster = { canvas, context, image: context.createImageData(width, height) }
    densityRasterByKey.set(key, raster)
  }
  const denominator = Math.log1p(maximum)
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const sourceIndex = layout === 'x-major' ? x * height + y : y * width + x
    const amount = Math.min(1, Math.log1p(values[sourceIndex]!) / denominator * intensity)
    const offset = (y * width + x) * 4
    raster.image.data[offset] = color[0]
    raster.image.data[offset + 1] = color[1]
    raster.image.data[offset + 2] = color[2]
    raster.image.data[offset + 3] = Math.round(amount * 255)
  }
  raster.context.putImageData(raster.image, 0, 0)
  return raster.canvas
}

function drawHistogram(
  context: CanvasRenderingContext2D,
  result: VideoScopesResult,
  width: number,
  height: number,
  intensity: number,
): boolean {
  const histogram = result.histogram
  if (!histogram) return false
  const colors = ['rgba(235,240,247,.9)', 'rgba(255,83,83,.9)', 'rgba(69,226,126,.9)', 'rgba(78,139,255,.95)']
  let maximum = 0
  for (const value of histogram) maximum = Math.max(maximum, value)
  if (!maximum) return true
  context.save()
  context.lineWidth = Math.max(1, width / 500)
  for (let plane = 0; plane < 4; plane++) {
    context.strokeStyle = colors[plane]!
    context.beginPath()
    for (let bin = 0; bin < 256; bin++) {
      const count = histogram[plane * 256 + bin]!
      const normalized = Math.min(1, Math.log1p(count) / Math.log1p(maximum) * intensity)
      const x = bin / 255 * width
      const y = height - normalized * (height - 8)
      if (bin === 0) context.moveTo(x, y)
      else context.lineTo(x, y)
    }
    context.stroke()
  }
  context.restore()
  return true
}

function drawDensityScope(
  context: CanvasRenderingContext2D,
  scope: Exclude<VideoScopeKind, 'histogram'>,
  result: VideoScopesResult,
  width: number,
  height: number,
  intensity: number,
): boolean {
  if (scope === 'waveform') {
    if (!result.waveform) return false
    const traceWidth = result.waveform.length / 256
    if (!Number.isInteger(traceWidth)) return false
    const raster = densityImage('waveform', result.waveform, traceWidth, 256, [225, 241, 247], intensity, 'x-major')
    if (raster) context.drawImage(raster, 0, 0, width, height)
    return true
  }
  if (scope === 'vectorscope') {
    if (!result.vectorscope) return false
    const raster = densityImage('vectorscope', result.vectorscope, 256, 256, [93, 238, 201], intensity)
    if (raster) {
      const side = Math.min(width, height)
      context.drawImage(raster, (width - side) / 2, (height - side) / 2, side, side)
    }
    return true
  }
  if (!result.parade) return false
  const planeLength = result.parade.length / 3
  const traceWidth = planeLength / 256
  if (!Number.isInteger(planeLength) || !Number.isInteger(traceWidth)) return false
  const colors: Array<readonly [number, number, number]> = [[255, 76, 76], [65, 228, 125], [75, 135, 255]]
  for (let plane = 0; plane < 3; plane++) {
    const values = result.parade.subarray(plane * planeLength, (plane + 1) * planeLength)
    const raster = densityImage(`parade-${plane}`, values, traceWidth, 256, colors[plane]!, intensity, 'x-major')
    if (raster) context.drawImage(raster, plane * width / 3, 0, width / 3, height)
  }
  return true
}

function renderScope(scope: VideoScopeKind): void {
  const canvas = canvasByScope.get(scope)
  if (!canvas) return
  const prepared = contextFor(canvas)
  if (!prepared) return
  const { context, width, height } = prepared
  context.clearRect(0, 0, width, height)
  context.fillStyle = '#070a0f'
  context.fillRect(0, 0, width, height)
  drawGrid(context, width, height)
  const result = displayedResult.value
  const rendered = result && (scope === 'histogram'
    ? drawHistogram(context, result, width, height, effectivePreferences.value.intensity)
    : drawDensityScope(context, scope, result, width, height, effectivePreferences.value.intensity))
  if (!rendered) {
    context.fillStyle = 'rgba(210, 220, 232, .62)'
    context.font = `${Math.max(12, Math.round(height / 16))}px system-ui, sans-serif`
    context.textAlign = 'center'
    context.textBaseline = 'middle'
    context.fillText(displayedStatus.value.kind === 'unavailable' ? 'Недоступно' : 'Ожидание кадра', width / 2, height / 2)
  }
  drawLabels(context, scope, width, height)
}

function renderAll(): void {
  for (const scope of visibleScopes.value) renderScope(scope)
}

function summaryRows(scope: VideoScopeKind) {
  return videoScopeSummaryRows(scope, displayedResult.value)
}

const statusLabel = computed(() => VIDEO_SCOPES_STATUS_LABELS[displayedStatus.value.kind])
const statusDetail = computed(() => {
  const status = displayedStatus.value
  const parts = [status.detail]
  if (status.sourceMode) parts.push(status.sourceMode === 'proxy' ? 'Proxy' : 'Оригинал')
  if (typeof status.mediaTime === 'number' && Number.isFinite(status.mediaTime)) {
    parts.push(`${status.mediaTime.toFixed(3)} s`)
  }
  return parts.filter(Boolean).join(' · ')
})

watch(
  () => [
    displayedResult.value,
    displayedStatus.value.kind,
    effectivePreferences.value.intensity,
    visibleScopes.value.join('|'),
  ],
  () => void nextTick(renderAll),
  { flush: 'post' },
)

onMounted(() => {
  if (!controlledPresentation.value) {
    controller.setKinds([...visibleScopes.value])
    controller.setTap(effectivePreferences.value.stage === 'source' ? 'pre-grade' : 'post-grade')
    controller.setEnabled(visibleScopes.value.length > 0)
    controller.setVisible(true)
  }
  if (typeof ResizeObserver !== 'undefined') {
    resizeObserver = new ResizeObserver(entries => {
      for (const entry of entries) {
        const scope = [...canvasByScope.entries()].find(([, canvas]) => canvas === entry.target)?.[0]
        if (scope) renderScope(scope)
      }
    })
    for (const canvas of canvasByScope.values()) resizeObserver.observe(canvas)
  }
  if (!controlledPresentation.value && typeof IntersectionObserver !== 'undefined') {
    intersectionObserver = new IntersectionObserver(entries => {
      controller.setVisible(entries.some(entry => entry.isIntersecting))
    }, { threshold: 0.01 })
    if (panelElement.value) intersectionObserver.observe(panelElement.value)
  }
  void nextTick(renderAll)
})

watch(
  () => effectivePreferences.value.stage,
  stage => { if (!controlledPresentation.value) controller.setTap(stage === 'source' ? 'pre-grade' : 'post-grade') },
)

watch(
  () => visibleScopes.value.join('|'),
  () => {
    if (controlledPresentation.value) return
    controller.setKinds([...visibleScopes.value])
    controller.setEnabled(visibleScopes.value.length > 0)
  },
)

onBeforeUnmount(() => {
  resizeObserver?.disconnect()
  resizeObserver = null
  intersectionObserver?.disconnect()
  intersectionObserver = null
  if (!controlledPresentation.value) {
    controller.setVisible(false)
    controller.setEnabled(false)
  }
  canvasByScope.clear()
  densityRasterByKey.clear()
})
</script>

<template>
  <section ref="panelElement" class="card video-scopes-panel" :aria-labelledby="`${instanceId}-title`">
    <header class="video-scopes-header">
      <div>
        <h2 :id="`${instanceId}-title`">Видеоскопы</h2>
        <p>Display-referred encoded sRGB · значения 0–255</p>
      </div>
      <span class="scope-status" :class="`status-${displayedStatus.kind}`">
        {{ statusLabel }}
      </span>
    </header>

    <div class="video-scopes-toolbar">
      <fieldset class="scope-stage">
        <legend>Сигнал</legend>
        <label>
          <input
            type="radio"
            :name="`${instanceId}-video-scopes-stage`"
            value="source"
            :checked="effectivePreferences.stage === 'source'"
            @change="setStage('source')"
          />
          До эффектов
        </label>
        <label>
          <input
            type="radio"
            :name="`${instanceId}-video-scopes-stage`"
            value="post-effects"
            :checked="effectivePreferences.stage === 'post-effects'"
            @change="setStage('post-effects')"
          />
          После эффектов
        </label>
      </fieldset>

      <fieldset class="scope-visibility">
        <legend>Показывать</legend>
        <label v-for="scope in VIDEO_SCOPE_KINDS" :key="scope">
          <input
            type="checkbox"
            :value="scope"
            :checked="effectivePreferences.visibleScopes.includes(scope)"
            @change="toggleScope(scope, ($event.currentTarget as HTMLInputElement).checked)"
          />
          {{ VIDEO_SCOPE_LABELS[scope] }}
        </label>
      </fieldset>

      <label class="scope-intensity">
        <span>Яркость следа</span>
        <input
          type="range"
          min="0.25"
          max="3"
          step="0.25"
          :value="effectivePreferences.intensity"
          aria-label="Яркость следа"
          :aria-describedby="`${instanceId}-intensity-note`"
          @input="setIntensity"
        />
        <output>{{ effectivePreferences.intensity.toFixed(2) }}×</output>
      </label>
      <span :id="`${instanceId}-intensity-note`" class="sr-only">Меняет только отображение, а не измеренные значения.</span>

    </div>

    <p v-if="statusDetail" class="scope-status-detail">{{ statusDetail }}</p>

    <div v-if="visibleScopes.length" class="video-scopes-grid">
      <figure v-for="scope in visibleScopes" :key="scope" class="video-scope-figure">
        <figcaption>{{ VIDEO_SCOPE_LABELS[scope] }}</figcaption>
        <canvas
          :ref="value => bindCanvas(scope, value)"
          class="video-scope-canvas"
          :class="{ 'is-vectorscope': scope === 'vectorscope' }"
          aria-hidden="true"
        ></canvas>
        <table class="scope-summary">
          <caption class="sr-only">Числовая сводка: {{ VIDEO_SCOPE_LABELS[scope] }}</caption>
          <thead>
            <tr>
              <th scope="col">Канал</th>
              <th v-for="column in VIDEO_SCOPE_SUMMARY_COLUMNS[scope]" :key="column" scope="col">{{ column }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in summaryRows(scope)" :key="row.channel">
              <th scope="row">{{ row.channel }}</th>
              <td>{{ row.minimum }}</td>
              <td>{{ row.median }}</td>
              <td>{{ row.maximum }}</td>
            </tr>
          </tbody>
        </table>
      </figure>
    </div>
    <p v-else class="scope-empty">Выберите хотя бы один scope в группе «Показывать».</p>
  </section>
</template>

<style scoped>
.video-scopes-panel {
  container-type: inline-size;
  min-width: 0;
}

.video-scopes-header,
.video-scopes-toolbar,
.scope-stage,
.scope-visibility,
.scope-intensity {
  display: flex;
  align-items: center;
}

.video-scopes-header {
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}

.video-scopes-header h2 {
  margin: 0;
  font-size: 16px;
}

.video-scopes-header p {
  margin: 3px 0 0;
  color: var(--muted);
  font-size: 11px;
}

.scope-status {
  flex: none;
  padding: 4px 9px;
  border: 1px solid var(--border);
  border-radius: 999px;
  background: var(--panel-2);
  color: var(--muted);
  font-size: 11px;
  font-weight: 650;
}

.status-live,
.status-exact {
  border-color: color-mix(in srgb, var(--accent) 65%, var(--border));
  color: var(--accent);
}

.status-last-exact,
.status-analyzing {
  color: var(--warn);
}

.status-unavailable,
.status-error {
  color: var(--danger);
}

.video-scopes-toolbar {
  flex-wrap: wrap;
  gap: 10px 14px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--panel-2);
}

.scope-stage,
.scope-visibility {
  min-width: 0;
  margin: 0;
  padding: 0;
  border: 0;
  gap: 8px;
  flex-wrap: wrap;
}

.scope-stage legend,
.scope-visibility legend {
  margin-right: 2px;
  color: var(--faint);
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
}

.scope-stage label,
.scope-visibility label {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--text);
  font-size: 11px;
  white-space: nowrap;
}

.scope-stage input,
.scope-visibility input {
  margin: 0;
}

.scope-intensity {
  min-width: min(230px, 100%);
  flex: 1;
  gap: 7px;
  color: var(--text);
  font-size: 11px;
}

.scope-intensity input {
  min-width: 90px;
  flex: 1;
}

.scope-intensity output {
  min-width: 42px;
  color: var(--muted);
  font-variant-numeric: tabular-nums;
  text-align: right;
}

.scope-status-detail,
.scope-empty {
  margin: 9px 0 0;
  color: var(--muted);
  font-size: 11px;
}

.video-scopes-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px;
  margin-top: 10px;
}

.video-scope-figure {
  min-width: 0;
  margin: 0;
  padding: 9px;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--panel-2);
}

.video-scope-figure figcaption {
  margin-bottom: 7px;
  color: var(--text);
  font-size: 12px;
  font-weight: 650;
}

.video-scope-canvas {
  display: block;
  width: 100%;
  height: 180px;
  border: 1px solid var(--border-strong);
  border-radius: 5px;
  background: #070a0f;
}

.video-scope-canvas.is-vectorscope {
  height: auto;
  aspect-ratio: 1;
}

.scope-summary {
  width: 100%;
  margin-top: 7px;
  border-collapse: collapse;
  color: var(--muted);
  font-size: 10px;
  font-variant-numeric: tabular-nums;
}

.scope-summary th,
.scope-summary td {
  padding: 3px 4px;
  border-bottom: 1px solid var(--border);
  text-align: right;
}

.scope-summary th:first-child,
.scope-summary td:first-child {
  text-align: left;
}

.scope-summary tbody tr:last-child > * {
  border-bottom: 0;
}

.scope-summary thead th {
  color: var(--faint);
  font-weight: 600;
}

.scope-summary tbody th {
  color: var(--text);
}

@container (max-width: 620px) {
  .video-scopes-grid {
    grid-template-columns: 1fr;
  }

  .video-scope-canvas {
    height: 200px;
  }
}

@media (max-width: 560px) {
  .video-scopes-header {
    align-items: flex-start;
  }

  .video-scopes-toolbar {
    align-items: stretch;
    flex-direction: column;
  }

  .scope-stage,
  .scope-visibility {
    align-items: flex-start;
  }

  .scope-intensity {
    width: 100%;
  }

}

@media (forced-colors: active) {
  .scope-status,
  .video-scopes-toolbar,
  .video-scope-figure,
  .video-scope-canvas {
    border-color: CanvasText;
  }

  .scope-status {
    color: CanvasText;
  }
}

@media (prefers-reduced-motion: reduce) {
  .video-scopes-panel * {
    scroll-behavior: auto;
    transition: none !important;
  }
}
</style>
