<script setup lang="ts">
// The multi-clip timeline: zoomable ruler, playhead, a video track of clip
// blocks with filmstrip thumbnails, an audio lane drawn from decoded peaks, and
// markers. Off-screen clips are culled, so a 200 clip project only ever mounts
// the handful of blocks the viewport actually shows.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  addMarker,
  clipDuration,
  compositionState,
  ensureClips,
  expectedOutputSeconds,
  MIN_CLIP_SECONDS,
  moveClip,
  nextMarker,
  removeClip,
  removeMarker,
  rippleDelete,
  setClipMuted,
  setClipTransition,
  splitAt,
  timelineSlots,
  timelineToSource,
  TRANSITION_KINDS,
  type TimelineSlot,
} from '../../store/composition'
import { beginEditTransaction, endEditTransaction, seekTo, state } from '../../store'
import type { ClipSpec } from '../../types'
import ClipBlock from './ClipBlock.vue'
import { bucketOf, releaseFilmstrip, requestThumbnails } from './filmstrip'
import { snapTime, type SnapTarget } from './snapping'
import type { WaveformPeaks } from '../../domain/waveform'
import { cachedPeaks, loadPeaks, peakBetween } from './waveform'

const MIN_ZOOM = 2
const MAX_ZOOM = 400
const SNAP_PIXELS = 8
/** Thumbnail bucket ladder: quantizing keeps the cache warm across zoom steps. */
const BUCKETS = [0.25, 0.5, 1, 2, 5, 10, 30, 60, 120]
const LANE_HEIGHT = 36
const SHUTTLE_RATES = [1, 2, 4, 8]

const root = ref<HTMLElement | null>(null)
const scroller = ref<HTMLElement | null>(null)
const lane = ref<HTMLCanvasElement | null>(null)

const pxPerSecond = ref(40)
const scrollLeft = ref(0)
const viewport = ref(640)
const selected = ref(-1)
const playhead = ref(0)
const thumbVersion = ref(0)
const announcement = ref('')
const snapAt = ref<number | null>(null)
const dropIndex = ref(-1)
const range = ref<{ from: number; to: number } | null>(null)
const peaks = ref<WaveformPeaks | null>(null)

const sourceKey = computed(() => state.video?.id ?? '')
const sourceDuration = computed(() => Math.max(0, state.video?.duration ?? 0))
const frameStep = computed(() => 1 / Math.max(1, state.video?.fps || 30))

/** Live clip list, or one virtual block covering the untouched source. */
const slots = computed<TimelineSlot[]>(() => {
  if (compositionState.clips.length) return timelineSlots()
  const id = sourceKey.value
  if (!id || sourceDuration.value <= 0) return []
  const clip: ClipSpec = {
    sourceId: id,
    start: 0,
    end: sourceDuration.value,
    speed: 1,
    volume: 1,
    muted: false,
    transitionIn: null,
  }
  return [{ index: 0, clip, start: 0, end: sourceDuration.value }]
})

const total = computed(() => slots.value[slots.value.length - 1]?.end ?? 0)
const totalPx = computed(() => Math.max(viewport.value, total.value * pxPerSecond.value))
const tolerance = computed(() => SNAP_PIXELS / pxPerSecond.value)

const bucketSeconds = computed(() => {
  const wanted = 96 / pxPerSecond.value
  return BUCKETS.find((step) => step >= wanted) ?? BUCKETS[BUCKETS.length - 1]!
})

const visibleSlots = computed(() => {
  const from = scrollLeft.value / pxPerSecond.value - 2
  const to = (scrollLeft.value + viewport.value) / pxPerSecond.value + 2
  return slots.value.filter((slot) => slot.end >= from && slot.start <= to)
})

/** Ruler ticks for the visible window only. */
const ticks = computed(() => {
  const steps = [0.5, 1, 2, 5, 10, 15, 30, 60, 300, 600]
  const step = steps.find((value) => value * pxPerSecond.value >= 70) ?? 900
  const from = Math.floor(scrollLeft.value / pxPerSecond.value / step) * step
  const to = (scrollLeft.value + viewport.value) / pxPerSecond.value
  const out: { t: number; label: string }[] = []
  for (let t = from; t <= to && out.length < 64; t += step) {
    out.push({ t, label: clock(t) })
  }
  return out
})

const summary = computed(() => {
  const count = compositionState.clips.length
  if (!count) return 'один фрагмент'
  return `${count} клипов, ${expectedOutputSeconds().toFixed(1)} с на выходе`
})

function clock(seconds: number): string {
  const value = Math.max(0, seconds)
  const minutes = Math.floor(value / 60)
  const rest = value % 60
  return `${minutes}:${rest < 10 ? `0${rest.toFixed(1)}` : rest.toFixed(0)}`
}

function announce(text: string): void {
  announcement.value = text
}

/** Materialize the implicit single clip before any mutation touches it. */
function ensure(): boolean {
  const video = state.video
  if (!video) return false
  return ensureClips(video.id, video.duration)
}

// --- playhead and player bridge ---

function sourceAt(t: number): number {
  const mapped = timelineToSource(t)
  return mapped ? mapped.seconds : Math.min(t, sourceDuration.value)
}

function movePlayhead(t: number): void {
  playhead.value = Math.max(0, Math.min(total.value, Number.isFinite(t) ? t : 0))
  seekTo(sourceAt(playhead.value))
}

// The preview plays the untouched source, so a player tick is mapped back onto
// the composed timeline instead of being trusted as a timeline position.
watch(
  () => state.playerTime,
  (time) => {
    if (dragging) return
    if (!compositionState.clips.length) {
      playhead.value = Math.min(time, total.value)
      return
    }
    for (const slot of slots.value) {
      if (time >= slot.clip.start - 1e-6 && time <= slot.clip.end + 1e-6) {
        const speed = slot.clip.speed ?? 1
        playhead.value = slot.start + (time - slot.clip.start) / speed
        return
      }
    }
  },
)

// --- zoom, scroll and viewport ---

function zoomBy(factor: number): void {
  const anchor = playhead.value
  pxPerSecond.value = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, pxPerSecond.value * factor))
  const element = scroller.value
  if (element) element.scrollLeft = Math.max(0, anchor * pxPerSecond.value - viewport.value / 2)
}

function zoomToFit(): void {
  if (total.value <= 0) return
  pxPerSecond.value = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, viewport.value / total.value))
  if (scroller.value) scroller.value.scrollLeft = 0
}

function onScroll(): void {
  scrollLeft.value = scroller.value?.scrollLeft ?? 0
}

let observer: ResizeObserver | null = null

onMounted(() => {
  const element = scroller.value
  if (!element) return
  viewport.value = element.clientWidth || 640
  if (typeof ResizeObserver !== 'undefined') {
    observer = new ResizeObserver(() => {
      viewport.value = element.clientWidth || viewport.value
      drawLane()
    })
    observer.observe(element)
  }
  zoomToFit()
})

onBeforeUnmount(() => {
  observer?.disconnect()
  observer = null
  stopShuttle()
  releaseFilmstrip()
})

// --- filmstrip and waveform ---

watch(
  [visibleSlots, bucketSeconds, sourceKey],
  () => {
    const video = state.video
    if (!video) return
    const wanted: number[] = []
    for (const slot of visibleSlots.value) {
      const speed = slot.clip.speed ?? 1
      const width = clipDuration(slot.clip) * pxPerSecond.value
      const count = Math.min(64, Math.ceil(width / 96))
      for (let index = 0; index < count && wanted.length < 200; index += 1) {
        const source = slot.clip.start + ((index * 96) / pxPerSecond.value) * speed
        wanted.push(bucketOf(source, bucketSeconds.value))
      }
    }
    requestThumbnails(video.id, video.url, wanted, bucketSeconds.value, () => {
      thumbVersion.value += 1
    })
  },
  { immediate: true },
)

watch(
  sourceKey,
  (id) => {
    releaseFilmstrip()
    peaks.value = null
    const video = state.video
    if (!id || !video) return
    const cached = cachedPeaks(id)
    if (cached !== undefined) {
      peaks.value = cached
      return
    }
    void loadPeaks(id, video.url).then((result) => {
      if (sourceKey.value === id) peaks.value = result
    })
  },
  { immediate: true },
)

/** Repaint the visible slice of the audio lane from the cached peaks. */
function drawLane(): void {
  const canvas = lane.value
  if (!canvas) return
  const ratio = Math.min(2, window.devicePixelRatio || 1)
  const width = Math.max(1, Math.round(viewport.value * ratio))
  const height = Math.round(LANE_HEIGHT * ratio)
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width
    canvas.height = height
  }
  const context = canvas.getContext('2d')
  if (!context) return
  context.clearRect(0, 0, width, height)
  const data = peaks.value
  const duration = sourceDuration.value
  if (!data || duration <= 0) return
  const middle = height / 2
  const columns = visibleSlots.value
  context.fillStyle = getComputedStyle(canvas).color
  for (let x = 0; x < width; x += 1) {
    const t = (scrollLeft.value + x / ratio) / pxPerSecond.value
    const slot = columns.find((candidate) => t >= candidate.start && t < candidate.end)
    if (!slot || slot.clip.muted) continue
    const speed = slot.clip.speed ?? 1
    const from = (slot.clip.start + (t - slot.start) * speed) / duration
    const step = speed / (pxPerSecond.value * duration * ratio)
    const peak = peakBetween(data, from, from + step) * (slot.clip.volume ?? 1)
    const half = Math.max(1, Math.min(middle, peak * middle))
    context.fillRect(x, middle - half, 1, half * 2)
  }
}

watch([visibleSlots, peaks, scrollLeft, pxPerSecond, viewport], drawLane, { flush: 'post' })

// --- snapping ---

function snapTargets(exclude: number): SnapTarget[] {
  const targets: SnapTarget[] = [
    { t: 0, kind: 'edge' },
    { t: total.value, kind: 'edge' },
    { t: playhead.value, kind: 'playhead' },
  ]
  for (const slot of visibleSlots.value) {
    if (slot.index === exclude) continue
    targets.push({ t: slot.start, kind: 'clip' }, { t: slot.end, kind: 'clip' })
  }
  for (const marker of compositionState.markers) targets.push({ t: marker.t, kind: 'marker' })
  return targets
}

// --- pointer gestures ---

interface Drag {
  mode: 'move' | 'trim-start' | 'trim-end' | 'scrub' | 'range'
  index: number
  originX: number
  originT: number
  clip: ClipSpec
}

let dragging: Drag | null = null

function timeAt(event: PointerEvent): number {
  const element = scroller.value
  if (!element) return 0
  const rect = element.getBoundingClientRect()
  const x = event.clientX - rect.left + element.scrollLeft
  return Math.max(0, Math.min(total.value, x / pxPerSecond.value))
}

function onRulerDown(event: PointerEvent): void {
  const t = timeAt(event)
  if (event.shiftKey) {
    dragging = { mode: 'range', index: -1, originX: event.clientX, originT: t, clip: emptyClip() }
    range.value = { from: t, to: t }
  } else {
    dragging = { mode: 'scrub', index: -1, originX: event.clientX, originT: t, clip: emptyClip() }
    movePlayhead(t)
  }
  attach()
}

function emptyClip(): ClipSpec {
  return { sourceId: '', start: 0, end: 0, speed: 1, volume: 1, muted: false, transitionIn: null }
}

function onGrab(slot: TimelineSlot, mode: 'move' | 'trim-start' | 'trim-end', event: PointerEvent): void {
  event.preventDefault()
  selected.value = slot.index
  if (!ensure()) return
  const clip = compositionState.clips[slot.index]
  if (!clip) return
  beginEditTransaction(`timeline-${mode}`)
  dragging = { mode, index: slot.index, originX: event.clientX, originT: slot.start, clip: { ...clip } }
  attach()
}

function attach(): void {
  window.addEventListener('pointermove', onPointerMove)
  window.addEventListener('pointerup', onPointerUp)
}

function onPointerMove(event: PointerEvent): void {
  const drag = dragging
  if (!drag) return
  const delta = (event.clientX - drag.originX) / pxPerSecond.value
  if (drag.mode === 'scrub') {
    movePlayhead(timeAt(event))
    return
  }
  if (drag.mode === 'range') {
    range.value = { from: drag.originT, to: timeAt(event) }
    return
  }
  const slot = slots.value[drag.index]
  if (!slot) return
  const snapped = snapTime(drag.originT + delta, snapTargets(drag.index), tolerance.value)
  snapAt.value = snapped.hit ? snapped.t : null
  const shift = snapped.t - slot.start
  const speed = drag.clip.speed ?? 1
  const clip = compositionState.clips[drag.index]
  if (!clip) return
  if (drag.mode === 'move') {
    dropIndex.value = dropTarget(snapped.t)
    return
  }
  if (drag.mode === 'trim-start') {
    const next = Math.max(0, Math.min(drag.clip.end - MIN_CLIP_SECONDS, drag.clip.start + shift * speed))
    clip.start = next
  } else {
    const length = Math.max(MIN_CLIP_SECONDS, shift)
    // Only the open source has a known length to clamp the out-point against.
    const limit =
      drag.clip.sourceId === sourceKey.value ? sourceDuration.value : Number.POSITIVE_INFINITY
    clip.end = Math.max(
      clip.start + MIN_CLIP_SECONDS,
      Math.min(limit, drag.clip.start + length * speed),
    )
  }
}

/** Index the dragged block would land on, by the slot its left edge is over. */
function dropTarget(t: number): number {
  const list = slots.value
  for (const slot of list) {
    if (t < (slot.start + slot.end) / 2) return slot.index
  }
  return Math.max(0, list.length - 1)
}

function onPointerUp(): void {
  const drag = dragging
  window.removeEventListener('pointermove', onPointerMove)
  window.removeEventListener('pointerup', onPointerUp)
  dragging = null
  snapAt.value = null
  if (!drag) return
  if (drag.mode === 'move' && dropIndex.value >= 0 && dropIndex.value !== drag.index) {
    if (moveClip(drag.index, dropIndex.value)) {
      selected.value = dropIndex.value
      announce(`Клип перемещён на позицию ${dropIndex.value + 1}`)
    }
  }
  dropIndex.value = -1
  if (drag.mode === 'move' || drag.mode === 'trim-start' || drag.mode === 'trim-end') {
    endEditTransaction()
  }
}

// --- commands ---

function doSplit(): void {
  if (!ensure()) return
  beginEditTransaction('timeline-split')
  const done = splitAt(playhead.value)
  endEditTransaction()
  announce(done ? `Разрез на ${clock(playhead.value)}` : 'Разрез невозможен в этой точке')
}

function doRippleDelete(): void {
  if (!ensure()) return
  const selection = range.value
  beginEditTransaction('timeline-ripple')
  let done = false
  if (selection) {
    done = rippleDelete(selection.from, selection.to)
    if (done) range.value = null
  } else if (selected.value >= 0) {
    done = removeClip(selected.value)
  }
  endEditTransaction()
  announce(done ? 'Фрагмент удалён, остальные сдвинуты' : 'Нечего удалять')
}

/** Multi-range keep: everything outside the selection is rippled away. */
function doKeepRange(): void {
  const selection = range.value
  if (!selection || !ensure()) return
  const from = Math.min(selection.from, selection.to)
  const to = Math.max(selection.from, selection.to)
  beginEditTransaction('timeline-keep')
  rippleDelete(to, total.value)
  const done = rippleDelete(0, from)
  endEditTransaction()
  range.value = null
  announce(done ? 'Оставлен только выбранный диапазон' : 'Диапазон уже занимает весь таймлайн')
}

function doAddMarker(): void {
  beginEditTransaction('timeline-marker')
  const done = addMarker(playhead.value)
  endEditTransaction()
  announce(done ? `Маркер на ${clock(playhead.value)}` : 'Маркер уже есть в этой точке')
}

function doRemoveMarker(index: number): void {
  beginEditTransaction('timeline-marker-remove')
  removeMarker(index)
  endEditTransaction()
  announce('Маркер удалён')
}

function gotoMarker(direction: 1 | -1): void {
  const marker = nextMarker(playhead.value, direction)
  if (marker) movePlayhead(marker.t)
}

function onClipCommand(slot: TimelineSlot, command: string, fine: boolean): void {
  if (command === 'seek') {
    movePlayhead(slot.start)
    return
  }
  if (!ensure()) return
  const step = fine ? frameStep.value : 0.5
  const clip = compositionState.clips[slot.index]
  if (!clip) return
  beginEditTransaction('timeline-key')
  switch (command) {
    case 'move-left':
      if (moveClip(slot.index, slot.index - 1)) selected.value = slot.index - 1
      break
    case 'move-right':
      if (moveClip(slot.index, slot.index + 1)) selected.value = slot.index + 1
      break
    case 'trim-start-back':
      clip.start = Math.max(0, clip.start - step)
      break
    case 'trim-start-forward':
      clip.start = Math.min(clip.end - MIN_CLIP_SECONDS, clip.start + step)
      break
    case 'trim-end-back':
      clip.end = Math.max(clip.start + MIN_CLIP_SECONDS, clip.end - step)
      break
    case 'trim-end-forward':
      clip.end = clip.end + step
      break
    case 'remove':
      if (removeClip(slot.index)) announce(`Клип ${slot.index + 1} удалён`)
      break
  }
  endEditTransaction()
}

// --- J/K/L shuttle ---
// The preview element belongs to VideoPreview, so the shuttle scrubs the
// playhead instead of driving playbackRate. K stops it.

let shuttleRate = 0
let shuttleFrame = 0
let shuttleLast = 0

function stopShuttle(): void {
  shuttleRate = 0
  if (shuttleFrame) cancelAnimationFrame(shuttleFrame)
  shuttleFrame = 0
}

function shuttleStep(direction: 1 | -1): void {
  const current = Math.abs(shuttleRate)
  const sameWay = Math.sign(shuttleRate) === direction
  const next = sameWay ? SHUTTLE_RATES[Math.min(SHUTTLE_RATES.indexOf(current) + 1, 3)]! : 1
  shuttleRate = next * direction
  announce(`Перемотка ${shuttleRate > 0 ? 'вперёд' : 'назад'} ${Math.abs(shuttleRate)}x`)
  if (shuttleFrame) return
  shuttleLast = performance.now()
  shuttleFrame = requestAnimationFrame(tick)
}

function tick(now: number): void {
  const elapsed = Math.min(0.1, (now - shuttleLast) / 1000)
  shuttleLast = now
  if (!shuttleRate) {
    shuttleFrame = 0
    return
  }
  movePlayhead(playhead.value + shuttleRate * elapsed)
  shuttleFrame = requestAnimationFrame(tick)
}

// --- keyboard ---

function isField(target: EventTarget | null): boolean {
  const element = target as HTMLElement | null
  if (!element) return false
  const tag = element.tagName
  return tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA' || element.isContentEditable
}

function onKey(event: KeyboardEvent): void {
  if (event.metaKey || event.ctrlKey || event.altKey || isField(event.target)) return
  let handled = true
  switch (event.code) {
    case 'KeyS':
      doSplit()
      break
    case 'KeyM':
      doAddMarker()
      break
    case 'KeyJ':
      shuttleStep(-1)
      break
    case 'KeyK':
      stopShuttle()
      break
    case 'KeyL':
      shuttleStep(1)
      break
    case 'Comma':
      movePlayhead(playhead.value - frameStep.value)
      break
    case 'Period':
      movePlayhead(playhead.value + frameStep.value)
      break
    case 'Equal':
    case 'NumpadAdd':
      zoomBy(1.5)
      break
    case 'Minus':
    case 'NumpadSubtract':
      zoomBy(1 / 1.5)
      break
    case 'Home':
      movePlayhead(0)
      break
    case 'End':
      movePlayhead(total.value)
      break
    case 'Delete':
    case 'Backspace':
      doRippleDelete()
      break
    default:
      handled = false
  }
  if (!handled) return
  event.preventDefault()
  // App.vue listens on window; stop here so a timeline key never fires twice.
  event.stopPropagation()
}

// --- selected clip inspector ---

const activeClip = computed(() => compositionState.clips[selected.value] ?? null)

function onTransitionKind(value: string): void {
  beginEditTransaction('timeline-transition')
  setClipTransition(selected.value, value ? { kind: value, duration: 0.5 } : null)
  endEditTransaction()
}

function onTransitionDuration(value: string): void {
  const clip = activeClip.value
  if (!clip?.transitionIn) return
  beginEditTransaction('timeline-transition')
  setClipTransition(selected.value, { kind: clip.transitionIn.kind, duration: Number(value) })
  endEditTransaction()
}

function onMute(value: boolean): void {
  beginEditTransaction('timeline-mute')
  setClipMuted(selected.value, value)
  endEditTransaction()
}

/** Pixel position of the drop marker while a clip is dragged, else null. */
const dropAt = computed(() => {
  const slot = slots.value[dropIndex.value]
  return slot ? slot.start * pxPerSecond.value : null
})

const rangeBox = computed(() => {
  const selection = range.value
  if (!selection) return null
  const from = Math.min(selection.from, selection.to)
  const to = Math.max(selection.from, selection.to)
  return { left: from * pxPerSecond.value, width: Math.max(1, (to - from) * pxPerSecond.value) }
})
</script>

<template>
  <div
    ref="root"
    class="tl"
    tabindex="0"
    role="application"
    aria-label="Таймлайн: S разрез, M маркер, J K L перемотка, плюс и минус масштаб"
    @keydown="onKey"
  >
    <div class="tl-bar">
      <button type="button" class="btn sm" @click="doSplit">Разрезать (S)</button>
      <button type="button" class="btn sm" @click="doRippleDelete">Удалить (Del)</button>
      <button type="button" class="btn sm" :disabled="!range" @click="doKeepRange">
        Оставить диапазон
      </button>
      <button type="button" class="btn sm" @click="doAddMarker">Маркер (M)</button>
      <span class="tl-gap"></span>
      <button type="button" class="btn sm" aria-label="Уменьшить масштаб" @click="zoomBy(1 / 1.5)">
        −
      </button>
      <button type="button" class="btn sm" aria-label="Увеличить масштаб" @click="zoomBy(1.5)">
        +
      </button>
      <button type="button" class="btn sm" @click="zoomToFit">Вписать</button>
    </div>

    <div ref="scroller" class="tl-scroll" @scroll="onScroll">
      <div class="tl-inner" :style="{ width: `${totalPx}px` }">
        <div class="tl-ruler" @pointerdown="onRulerDown">
          <span
            v-for="tick in ticks"
            :key="tick.t"
            class="tl-tick"
            :style="{ left: `${tick.t * pxPerSecond}px` }"
            >{{ tick.label }}</span
          >
        </div>

        <div v-if="rangeBox" class="tl-range" :style="{ left: `${rangeBox.left}px`, width: `${rangeBox.width}px` }"></div>

        <div class="tl-track" role="listbox" aria-label="Дорожка клипов">
          <ClipBlock
            v-for="slot in visibleSlots"
            :key="`${slot.index}-${slot.clip.start}`"
            :entry="slot"
            :total="slots.length"
            :px-per-second="pxPerSecond"
            :bucket-seconds="bucketSeconds"
            :source-key="sourceKey"
            :selected="slot.index === selected"
            :version="thumbVersion"
            @grab="(mode, pointer) => onGrab(slot, mode, pointer)"
            @select="selected = slot.index"
            @command="(command, fine) => onClipCommand(slot, command, fine)"
          />
          <div v-if="dropAt !== null" class="tl-drop" :style="{ left: `${dropAt}px` }"></div>
        </div>

        <div class="tl-lane">
          <canvas ref="lane" class="tl-wave" :style="{ left: `${scrollLeft}px`, width: `${viewport}px` }"></canvas>
        </div>

        <button
          v-for="(marker, index) in compositionState.markers"
          :key="`${marker.t}-${index}`"
          type="button"
          class="tl-marker"
          :style="{ left: `${marker.t * pxPerSecond}px` }"
          :aria-label="`Маркер ${marker.label || index + 1} на ${clock(marker.t)}, Enter удаляет`"
          @click="doRemoveMarker(index)"
        ></button>

        <div class="tl-playhead" :style="{ left: `${playhead * pxPerSecond}px` }" aria-hidden="true"></div>
        <div v-if="snapAt !== null" class="tl-snap" :style="{ left: `${snapAt * pxPerSecond}px` }" aria-hidden="true"></div>
      </div>
    </div>

    <div class="tl-foot">
      <button type="button" class="btn sm" @click="gotoMarker(-1)">← маркер</button>
      <button type="button" class="btn sm" @click="gotoMarker(1)">маркер →</button>
      <span class="tl-gap"></span>
      <span class="hint">{{ summary }}</span>
    </div>

    <div v-if="activeClip" class="tl-inspector">
      <label class="tl-field">
        <span>Переход в клип {{ selected + 1 }}</span>
        <select
          :value="activeClip.transitionIn?.kind ?? ''"
          :disabled="selected === 0"
          @change="onTransitionKind(($event.target as HTMLSelectElement).value)"
        >
          <option value="">без перехода</option>
          <option v-for="kind in TRANSITION_KINDS" :key="kind" :value="kind">{{ kind }}</option>
        </select>
      </label>
      <label v-if="activeClip.transitionIn" class="tl-field">
        <span>Длительность, с</span>
        <input
          type="number"
          min="0.05"
          max="3"
          step="0.05"
          :value="activeClip.transitionIn.duration"
          @change="onTransitionDuration(($event.target as HTMLInputElement).value)"
        />
      </label>
      <label class="tl-field tl-check">
        <input
          type="checkbox"
          :checked="activeClip.muted === true"
          @change="onMute(($event.target as HTMLInputElement).checked)"
        />
        <span>Без звука</span>
      </label>
    </div>

    <p class="hint">
      Shift + перетаскивание по линейке выделяет диапазон. Маркеры помогают в монтаже и не
      влияют на экспорт.
    </p>
    <p class="tl-live" role="status" aria-live="polite">{{ announcement }}</p>
  </div>
</template>

<style scoped>
.tl {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.tl:focus-visible {
  outline: none;
  box-shadow: var(--ring);
}

.tl-bar,
.tl-foot,
.tl-inspector {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.tl-gap {
  flex: 1;
}

.tl-scroll {
  position: relative;
  overflow-x: auto;
  overflow-y: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--panel);
}

.tl-inner {
  position: relative;
  height: 152px;
}

.tl-ruler {
  position: relative;
  height: 24px;
  border-bottom: 1px solid var(--border);
  color: var(--faint);
  font-size: 11px;
  cursor: col-resize;
  touch-action: none;
}

.tl-tick {
  position: absolute;
  top: 4px;
  padding-left: 4px;
  border-left: 1px solid var(--border-strong);
}

.tl-track {
  position: relative;
  height: 72px;
  margin: 4px 0;
}

.tl-lane {
  position: relative;
  height: 36px;
  border-top: 1px solid var(--border);
  color: var(--accent);
}

.tl-wave {
  position: absolute;
  top: 0;
  height: 36px;
}

.tl-playhead,
.tl-snap,
.tl-drop {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 2px;
  pointer-events: none;
}

.tl-playhead {
  background: var(--danger);
}

.tl-snap {
  background: var(--ok);
  box-shadow: 0 0 0 1px var(--ok);
}

.tl-drop {
  background: var(--accent);
}

.tl-range {
  position: absolute;
  top: 24px;
  bottom: 0;
  background: var(--accent-soft);
  pointer-events: none;
}

.tl-marker {
  position: absolute;
  top: 24px;
  width: 24px;
  height: 24px;
  margin-left: -12px;
  padding: 0;
  border: 0;
  background: none;
  cursor: pointer;
}

.tl-marker::before {
  content: '';
  position: absolute;
  left: 9px;
  top: 4px;
  width: 6px;
  height: 16px;
  border-radius: 3px;
  background: var(--warn);
}

.tl-field {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--muted);
}

.tl-field select,
.tl-field input[type='number'] {
  min-height: 24px;
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  color: var(--text);
  font: inherit;
}

.tl-check input {
  width: 24px;
  height: 24px;
}

.tl-live {
  margin: 0;
  min-height: 1em;
  color: var(--muted);
  font-size: 12px;
}

@media (prefers-reduced-motion: reduce) {
  .tl-scroll {
    scroll-behavior: auto;
  }
}
</style>
