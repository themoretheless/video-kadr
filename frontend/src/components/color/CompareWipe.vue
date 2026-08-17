<script setup lang="ts">
// Before/after split screen for the primary grade.
//
// Both halves are the same frame copied out of the player, so the two sides can
// never drift apart the way two <video> elements would, and the source is
// decoded once. The graded half runs through an SVG `feComponentTransfer` built
// from the very transfer tables the scopes use, so what the wipe shows and what
// the scopes measure agree by construction.
import { computed, ref, watch } from 'vue'
import { gradeTransferTables, isNeutralGrade, type ScopeGrade } from '../../domain/scopes'
import { playerElement, playerHasFrame } from './frameSource'

const props = defineProps<{ tick: number; grade: ScopeGrade }>()

/** Module counter: two mounted wipes must not share a filter id. */
let instances = 0
const filterId = `grade-wipe-${(instances += 1)}`

/** Preview resolution. Wide enough to judge a grade, cheap enough to redraw. */
const WIDTH = 640

const before = ref<HTMLCanvasElement | null>(null)
const after = ref<HTMLCanvasElement | null>(null)
const position = ref(0.5)
const ready = ref(false)

const tables = computed(() => gradeTransferTables(props.grade, 33))
const neutral = computed(() => isNeutralGrade(props.grade))

const beforeStyle = computed(() => ({
  clipPath: `inset(0 ${(100 - position.value * 100).toFixed(2)}% 0 0)`,
}))
const handleStyle = computed(() => ({ left: `${(position.value * 100).toFixed(2)}%` }))

function paint(): void {
  const video = playerElement()
  const targets = [before.value, after.value]
  if (!playerHasFrame(video) || targets.some((target) => !target)) {
    ready.value = false
    return
  }
  const height = Math.max(1, Math.round((WIDTH * video.videoHeight) / video.videoWidth))
  for (const target of targets) {
    if (!target) continue
    target.width = WIDTH
    target.height = height
    const context = target.getContext('2d')
    if (!context) continue
    context.drawImage(video, 0, 0, WIDTH, height)
  }
  ready.value = true
}

watch(() => props.tick, paint, { immediate: true, flush: 'post' })

function setFromPointer(event: PointerEvent): void {
  const box = (event.currentTarget as HTMLElement).getBoundingClientRect()
  if (box.width <= 0) return
  position.value = Math.max(0, Math.min(1, (event.clientX - box.left) / box.width))
}

function onDown(event: PointerEvent): void {
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  setFromPointer(event)
}

function onMove(event: PointerEvent): void {
  const target = event.currentTarget as HTMLElement
  if (!target.hasPointerCapture(event.pointerId)) return
  setFromPointer(event)
}

function onKey(event: KeyboardEvent): void {
  const step = event.shiftKey ? 0.01 : 0.05
  const deltas: Record<string, number> = { ArrowLeft: -step, ArrowRight: step }
  const delta = deltas[event.key]
  if (delta !== undefined) {
    event.preventDefault()
    position.value = Math.max(0, Math.min(1, position.value + delta))
    return
  }
  if (event.key === 'Home') {
    event.preventDefault()
    position.value = 0.5
  }
}
</script>

<template>
  <div class="compare">
    <div
      class="compare-stage"
      @pointerdown="onDown"
      @pointermove="onMove"
    >
      <canvas ref="after" class="compare-layer" :style="{ filter: `url(#${filterId})` }" />
      <canvas ref="before" class="compare-layer" :style="beforeStyle" />
      <span class="compare-tag compare-tag-left">До</span>
      <span class="compare-tag compare-tag-right">После</span>
      <div
        class="compare-handle"
        :style="handleStyle"
        role="slider"
        tabindex="0"
        aria-label="Граница сравнения до и после"
        aria-valuemin="0"
        aria-valuemax="100"
        :aria-valuenow="Math.round(position * 100)"
        :aria-valuetext="`${Math.round(position * 100)}% исходника`"
        @keydown="onKey"
      ></div>
      <svg class="compare-defs" aria-hidden="true">
        <filter :id="filterId" color-interpolation-filters="sRGB">
          <feComponentTransfer>
            <feFuncR type="table" :tableValues="tables[0]" />
            <feFuncG type="table" :tableValues="tables[1]" />
            <feFuncB type="table" :tableValues="tables[2]" />
          </feComponentTransfer>
        </filter>
      </svg>
    </div>
    <p v-if="!ready" class="hint">Кадр появится, как только плеер покажет видео.</p>
    <p v-else-if="neutral" class="hint">Коррекция нейтральна: обе половины пока одинаковые.</p>
    <p v-else class="hint">
      Приблизительный предпросмотр первичной коррекции; HSL и LUT здесь не учитываются.
    </p>
  </div>
</template>

<style scoped>
.compare-stage {
  position: relative;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--panel-3);
  touch-action: none;
  cursor: ew-resize;
}

.compare-layer {
  display: block;
  width: 100%;
  height: auto;
}

.compare-layer + .compare-layer {
  position: absolute;
  inset: 0;
}

.compare-handle {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 3px;
  margin-left: -1px;
  background: var(--text);
  cursor: ew-resize;
}

.compare-handle:focus-visible {
  outline: none;
  box-shadow: var(--ring);
}

.compare-tag {
  position: absolute;
  top: 6px;
  padding: 1px 6px;
  border-radius: var(--radius-sm);
  background: rgba(0, 0, 0, 0.55);
  color: #fff;
  font-size: 11px;
  pointer-events: none;
}

.compare-tag-left {
  left: 6px;
}

.compare-tag-right {
  right: 6px;
}

.compare-defs {
  position: absolute;
  width: 0;
  height: 0;
}
</style>
