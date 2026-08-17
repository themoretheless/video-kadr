<script setup lang="ts">
// Histogram, waveform, RGB parade and vectorscope, painted from the reduced
// resolution frame sample the grabber produces. Only the selected scope is
// computed, so switching tabs is what costs, not the loop.
import { computed, ref, watch } from 'vue'
import {
  computeHistogram,
  computeVectorscope,
  computeWaveform,
  gradeFrame,
  type FrameSample,
  type ScopeGrade,
} from '../../domain/scopes'

const props = defineProps<{ frame: FrameSample | null; grade: ScopeGrade; graded: boolean }>()

type ScopeKind = 'histogram' | 'waveform' | 'parade' | 'vector'

const SCOPES: { id: ScopeKind; label: string }[] = [
  { id: 'histogram', label: 'Гистограмма' },
  { id: 'waveform', label: 'Вейвформа' },
  { id: 'parade', label: 'Парад RGB' },
  { id: 'vector', label: 'Вектороскоп' },
]

/** Canvas backing store. Fixed, so the scopes cost the same on any layout. */
const WIDTH = 512
const HEIGHT = 200

const kind = ref<ScopeKind>('histogram')
const canvas = ref<HTMLCanvasElement | null>(null)

const source = computed<FrameSample | null>(() => {
  if (!props.frame) return null
  return props.graded ? gradeFrame(props.frame, props.grade) : props.frame
})

const CHANNEL_COLORS = ['#ff5f5f', '#54d68a', '#5f9dff']

function context(): CanvasRenderingContext2D | null {
  const element = canvas.value
  // `getContext` is missing in the test environment and can return null when a
  // browser refuses a 2D context; either way there is nothing to paint on.
  if (!element || typeof element.getContext !== 'function') return null
  element.width = WIDTH
  element.height = HEIGHT
  return element.getContext('2d')
}

/** Grey background plus the IRE-style guide lines every scope shares. */
function background(ctx: CanvasRenderingContext2D): void {
  ctx.fillStyle = '#0b0d11'
  ctx.fillRect(0, 0, WIDTH, HEIGHT)
  ctx.strokeStyle = 'rgba(255, 255, 255, 0.12)'
  ctx.lineWidth = 1
  for (let step = 1; step < 4; step++) {
    const y = Math.round((HEIGHT * step) / 4) + 0.5
    ctx.beginPath()
    ctx.moveTo(0, y)
    ctx.lineTo(WIDTH, y)
    ctx.stroke()
  }
}

function drawHistogram(ctx: CanvasRenderingContext2D, frame: FrameSample): void {
  const histogram = computeHistogram(frame, 256)
  const peak = Math.max(1, histogram.peak)
  const channels = [histogram.red, histogram.green, histogram.blue]
  ctx.globalCompositeOperation = 'lighter'
  channels.forEach((counts, index) => {
    ctx.fillStyle = CHANNEL_COLORS[index]
    ctx.globalAlpha = 0.55
    ctx.beginPath()
    ctx.moveTo(0, HEIGHT)
    for (let bin = 0; bin < counts.length; bin++) {
      const x = (bin / (counts.length - 1)) * WIDTH
      ctx.lineTo(x, HEIGHT - (counts[bin] / peak) * HEIGHT)
    }
    ctx.lineTo(WIDTH, HEIGHT)
    ctx.closePath()
    ctx.fill()
  })
  ctx.globalCompositeOperation = 'source-over'
  ctx.globalAlpha = 1
  ctx.strokeStyle = '#e8ebf1'
  ctx.beginPath()
  for (let bin = 0; bin < histogram.luma.length; bin++) {
    const x = (bin / (histogram.luma.length - 1)) * WIDTH
    const y = HEIGHT - (histogram.luma[bin] / peak) * HEIGHT
    if (bin === 0) ctx.moveTo(x, y)
    else ctx.lineTo(x, y)
  }
  ctx.stroke()
}

/** Blit a counts grid through an ImageData, then scale it onto the canvas. */
function blit(
  ctx: CanvasRenderingContext2D,
  width: number,
  height: number,
  paint: (image: ImageData) => void,
  target: { x: number; width: number },
): void {
  const image = ctx.createImageData(width, height)
  paint(image)
  const buffer = document.createElement('canvas')
  buffer.width = width
  buffer.height = height
  const bufferContext = buffer.getContext('2d')
  if (!bufferContext) return
  bufferContext.putImageData(image, 0, 0)
  ctx.drawImage(buffer, target.x, 0, target.width, HEIGHT)
}

/** Counts to pixels: a soft log-ish ramp so a single trace stays visible. */
function intensity(count: number, peak: number): number {
  if (count <= 0) return 0
  return Math.min(255, 40 + (Math.log1p(count) / Math.log1p(peak)) * 215)
}

function drawWaveformInto(
  image: ImageData,
  counts: Uint32Array,
  columns: number,
  levels: number,
  peak: number,
  tint: [number, number, number],
): void {
  for (let column = 0; column < columns; column++) {
    for (let level = 0; level < levels; level++) {
      const value = intensity(counts[column * levels + level], peak)
      if (!value) continue
      // Level 0 is black, so it belongs at the bottom of the scope.
      const at = ((levels - 1 - level) * columns + column) * 4
      image.data[at] = (value * tint[0]) / 255
      image.data[at + 1] = (value * tint[1]) / 255
      image.data[at + 2] = (value * tint[2]) / 255
      image.data[at + 3] = 255
    }
  }
}

function drawWaveform(ctx: CanvasRenderingContext2D, frame: FrameSample, parade: boolean): void {
  const columns = 256
  const levels = 200
  const waveform = computeWaveform(frame, columns, levels)
  const peak = Math.max(1, waveform.peak)
  if (!parade) {
    blit(
      ctx,
      columns,
      levels,
      (image) =>
        drawWaveformInto(image, waveform.luma, columns, levels, peak, [255, 255, 255]),
      { x: 0, width: WIDTH },
    )
    return
  }
  const tints: [number, number, number][] = [
    [255, 95, 95],
    [84, 214, 138],
    [95, 157, 255],
  ]
  const lanes = [waveform.red, waveform.green, waveform.blue]
  const laneWidth = WIDTH / 3
  lanes.forEach((counts, index) => {
    blit(
      ctx,
      columns,
      levels,
      (image) => drawWaveformInto(image, counts, columns, levels, peak, tints[index]),
      { x: index * laneWidth, width: laneWidth },
    )
  })
}

function drawVectorscope(ctx: CanvasRenderingContext2D, frame: FrameSample): void {
  const size = 200
  const scope = computeVectorscope(frame, size)
  const peak = Math.max(1, scope.peak)
  const left = (WIDTH - HEIGHT) / 2
  blit(
    ctx,
    size,
    size,
    (image) => {
      for (let bin = 0; bin < scope.bins.length; bin++) {
        const value = intensity(scope.bins[bin], peak)
        if (!value) continue
        const at = bin * 4
        image.data[at] = value * 0.4
        image.data[at + 1] = value
        image.data[at + 2] = value * 0.6
        image.data[at + 3] = 255
      }
    },
    { x: left, width: HEIGHT },
  )
  // Neutral axis cross, so a colour cast is obvious at a glance.
  ctx.strokeStyle = 'rgba(255, 255, 255, 0.2)'
  ctx.beginPath()
  ctx.moveTo(left + HEIGHT / 2, 0)
  ctx.lineTo(left + HEIGHT / 2, HEIGHT)
  ctx.moveTo(left, HEIGHT / 2)
  ctx.lineTo(left + HEIGHT, HEIGHT / 2)
  ctx.stroke()
}

function draw(): void {
  const ctx = context()
  if (!ctx) return
  background(ctx)
  const frame = source.value
  if (!frame) return
  if (kind.value === 'histogram') drawHistogram(ctx, frame)
  else if (kind.value === 'vector') drawVectorscope(ctx, frame)
  else drawWaveform(ctx, frame, kind.value === 'parade')
}

watch([source, kind], draw, { immediate: true, flush: 'post' })
</script>

<template>
  <div class="scopes">
    <div class="chips">
      <button
        v-for="scope in SCOPES"
        :key="scope.id"
        type="button"
        class="chip"
        :class="{ active: kind === scope.id }"
        @click="kind = scope.id"
      >
        {{ scope.label }}
      </button>
    </div>
    <canvas ref="canvas" class="scope-canvas" :aria-label="`Осциллограмма: ${kind}`" role="img" />
    <p v-if="!frame" class="hint">Открой видео и запусти плеер, чтобы увидеть осциллограммы.</p>
  </div>
</template>

<style scoped>
.scopes {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.scope-canvas {
  width: 100%;
  height: auto;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: #0b0d11;
}
</style>
