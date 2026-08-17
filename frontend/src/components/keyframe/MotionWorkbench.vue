<script setup lang="ts">
// Motion workbench: Ken Burns framing, the four keyframed transform tracks and
// the speed ramp editor. Every control here maps onto something
// `backend/src/render/graph/motion.rs` actually renders; nothing is offered
// that the export would silently drop.
//
// Loaded on demand by MotionPanel, so the keyframe lane and the second decoder
// only reach the browser once the section is open.

import { computed, ref } from 'vue'
import {
  clamp,
  MAX_RAMP_SPEED,
  MIN_RAMP_SPEED,
  putKeyframe,
  rampOutputDuration,
  sampleMotion,
  setTrackInterpolation,
  type MotionSample,
} from '../../domain/keyframes'
import { beginEditTransaction, endEditTransaction, state } from '../../store'
import {
  MOTION_RANGES,
  motionState,
  resetMotion,
  setMotionTrack,
  type MotionTrackKey,
} from '../../store/motion'
import type { Interpolation, KeyframeTrack } from '../../types'
import KeyframeTrackEditor from './KeyframeTrackEditor.vue'
import KenBurnsStage from './KenBurnsStage.vue'

interface TrackTab {
  key: MotionTrackKey
  label: string
  full: string
  unit: string
  precision: number
}

const TRANSFORM_TABS: TrackTab[] = [
  { key: 'zoom', label: 'Зум', full: 'Зум кадра', unit: '×', precision: 2 },
  { key: 'panX', label: 'Панорама X', full: 'Панорама по горизонтали', unit: '', precision: 2 },
  { key: 'panY', label: 'Панорама Y', full: 'Панорама по вертикали', unit: '', precision: 2 },
  { key: 'rotation', label: 'Поворот', full: 'Поворот кадра', unit: '°', precision: 1 },
]

/** The tracks the framing rectangles drive; rotation is keyframed only. */
type FrameKey = 'zoom' | 'panX' | 'panY'
const FRAME_KEYS: FrameKey[] = ['zoom', 'panX', 'panY']

const activeTab = ref<MotionTrackKey>('zoom')
const showResult = ref(false)

const video = computed(() => state.video)
const sourceSpan = computed(() => Math.max(0.05, state.edit.trimEnd - state.edit.trimStart))
const editSpeed = computed(() => (state.edit.speed > 0 ? state.edit.speed : 1))
/** Position of the playhead inside the trimmed source, in seconds. */
const rampTime = computed(() =>
  clamp(state.playerTime - state.edit.trimStart, 0, sourceSpan.value),
)
/** Output duration after the ramp, which is the transform tracks' timeline. */
const outputSpan = computed(
  () => rampOutputDuration(motionState.speedRamps, sourceSpan.value) / editSpeed.value,
)
const motionTime = computed(
  () => rampOutputDuration(motionState.speedRamps, rampTime.value) / editSpeed.value,
)

const tracks = computed(() => ({
  zoom: motionState.zoom,
  panX: motionState.panX,
  panY: motionState.panY,
  rotation: motionState.rotation,
}))

const currentSample = computed(() => sampleMotion(tracks.value, motionTime.value))
const startSample = computed(() => sampleMotion(tracks.value, 0))
const endSample = computed(() => sampleMotion(tracks.value, outputSpan.value))

const activeTrackCount = computed(
  () =>
    [
      motionState.zoom,
      motionState.panX,
      motionState.panY,
      motionState.rotation,
      motionState.speedRamps,
    ].filter((track) => track.length > 0).length,
)
const panWithoutZoom = computed(
  () =>
    (motionState.panX.length > 0 || motionState.panY.length > 0) &&
    endSample.value.zoom <= 1 + 1e-6 &&
    startSample.value.zoom <= 1 + 1e-6,
)

function transaction(update: () => void): void {
  beginEditTransaction('motion')
  update()
  endEditTransaction()
}

function formatSeconds(value: number): string {
  return `${value.toFixed(2).replace('.', ',')} с`
}

/**
 * Write a start/end pair into one track. A pair that sits on the neutral value
 * at both ends leaves the track empty instead: the render would drop it anyway,
 * and an empty track is what keeps the panel summary honest.
 */
function writePair(key: MotionTrackKey, startValue: number, endValue: number): void {
  const range = MOTION_RANGES[key]
  const start = clamp(startValue, range.min, range.max)
  const end = clamp(endValue, range.min, range.max)
  if (Math.abs(start - range.neutral) < 1e-6 && Math.abs(end - range.neutral) < 1e-6) {
    setMotionTrack(key, [])
    return
  }
  let track = putKeyframe(motionState[key], 0, start)
  track = putKeyframe(track, outputSpan.value, end)
  setMotionTrack(key, track)
}

/** A rectangle drag writes zoom and pan at the end of the timeline it edits. */
function onFrameUpdate(which: 'start' | 'end', sample: MotionSample): void {
  if (outputSpan.value <= 0.05) return
  const start = which === 'start' ? sample : startSample.value
  const end = which === 'end' ? sample : endSample.value
  for (const key of FRAME_KEYS) setMotionTrack(key, [])
  for (const key of FRAME_KEYS) writePair(key, start[key], end[key])
}

interface KenBurnsPreset {
  label: string
  start: Record<FrameKey, number>
  end: Record<FrameKey, number>
}

const KEN_BURNS_PRESETS: KenBurnsPreset[] = [
  {
    label: 'Наезд',
    start: { zoom: 1, panX: 0, panY: 0 },
    end: { zoom: 1.4, panX: 0, panY: 0 },
  },
  {
    label: 'Отъезд',
    start: { zoom: 1.4, panX: 0, panY: 0 },
    end: { zoom: 1, panX: 0, panY: 0 },
  },
  {
    label: 'Панорама вправо',
    start: { zoom: 1.25, panX: -1, panY: 0 },
    end: { zoom: 1.25, panX: 1, panY: 0 },
  },
  {
    label: 'Панорама влево',
    start: { zoom: 1.25, panX: 1, panY: 0 },
    end: { zoom: 1.25, panX: -1, panY: 0 },
  },
]

function applyKenBurns(preset: KenBurnsPreset): void {
  transaction(() => {
    for (const key of FRAME_KEYS) {
      setMotionTrack(key, [])
      writePair(key, preset.start[key], preset.end[key])
      setMotionTrack(key, setTrackInterpolation(motionState[key], 'smooth'))
    }
  })
}

function clearTransform(): void {
  transaction(() => {
    for (const key of [...FRAME_KEYS, 'rotation' as const]) setMotionTrack(key, [])
  })
}

interface RampPreset {
  label: string
  title: string
  speed: number
  interp: Interpolation
  /** Half-width of the ramp around the playhead, in source seconds. */
  reach: number
}

const RAMP_PRESETS: RampPreset[] = [
  {
    label: 'Замедление',
    title: 'Плавный уход в 0,5× у курсора и возврат',
    speed: 0.5,
    interp: 'linear',
    reach: 0.75,
  },
  {
    label: 'Ускорение',
    title: 'Плавный разгон до 2× у курсора и возврат',
    speed: 2,
    interp: 'linear',
    reach: 0.75,
  },
  {
    label: 'Почти стоп-кадр',
    title: 'Ступенька 0,25× длиной секунда: меньше рендер не умеет',
    speed: MIN_RAMP_SPEED,
    interp: 'hold',
    reach: 1,
  },
]

function applyRamp(preset: RampPreset): void {
  const span = sourceSpan.value
  const at = clamp(rampTime.value, 0, span)
  let track: KeyframeTrack = []
  if (preset.interp === 'hold') {
    // A hold track is stepwise, so two points already make one slow section.
    track = putKeyframe(track, Math.max(0, at), preset.speed)
    track = putKeyframe(track, Math.min(span, at + preset.reach), 1)
    if (at > 0) track = putKeyframe(track, 0, 1)
  } else {
    track = putKeyframe(track, Math.max(0, at - preset.reach), 1)
    track = putKeyframe(track, at, preset.speed)
    track = putKeyframe(track, Math.min(span, at + preset.reach), 1)
  }
  transaction(() => setMotionTrack('speedRamps', setTrackInterpolation(track, preset.interp)))
}

function onTrackUpdate(key: MotionTrackKey, track: KeyframeTrack): void {
  setMotionTrack(key, track)
}

function resetAll(): void {
  transaction(resetMotion)
}
</script>

<template>
  <div v-if="video" class="motion">
    <div class="motion-block">
      <div class="motion-head">
        <span class="motion-title">Кадрирование (Ken Burns)</span>
        <label class="motion-switch">
          <input v-model="showResult" type="checkbox" />
          <span>Результат</span>
        </label>
      </div>

      <KenBurnsStage
        :src="video.url"
        :width="video.width"
        :height="video.height"
        :time="state.playerTime"
        :current="currentSample"
        :start="startSample"
        :end="endSample"
        :show-result="showResult"
        @update:start="onFrameUpdate('start', $event)"
        @update:end="onFrameUpdate('end', $event)"
        @interaction-start="beginEditTransaction('motion-frame')"
        @interaction-end="endEditTransaction"
      />

      <p class="hint">
        Перетащите рамки «Старт» и «Финиш», угол рамки меняет крупность. Белая рамка показывает
        кадр на текущей позиции курсора. Движение задаётся двумя ключами, поэтому перетаскивание
        рамок заменяет остальные ключи зума и панорамы.
      </p>

      <div class="motion-chips">
        <button
          v-for="preset in KEN_BURNS_PRESETS"
          :key="preset.label"
          type="button"
          class="motion-chip"
          @click="applyKenBurns(preset)"
        >
          {{ preset.label }}
        </button>
        <button type="button" class="motion-chip" @click="clearTransform">Без движения</button>
      </div>

      <p v-if="panWithoutZoom" class="motion-note" role="status">
        Панорама действует только при зуме больше 1: на полном кадре двигать нечего.
      </p>
    </div>

    <div class="motion-block">
      <div class="motion-tabs" role="group" aria-label="Параметр трансформации">
        <button
          v-for="tab in TRANSFORM_TABS"
          :key="tab.key"
          type="button"
          class="motion-chip"
          :aria-pressed="activeTab === tab.key"
          @click="activeTab = tab.key"
        >
          {{ tab.label }}
          <span v-if="motionState[tab.key].length" class="motion-dot" aria-hidden="true"></span>
        </button>
      </div>

      <template v-for="tab in TRANSFORM_TABS" :key="tab.key">
        <KeyframeTrackEditor
          v-if="activeTab === tab.key"
          :model-value="motionState[tab.key]"
          :duration="outputSpan"
          :min="MOTION_RANGES[tab.key].min"
          :max="MOTION_RANGES[tab.key].max"
          :neutral="MOTION_RANGES[tab.key].neutral"
          :label="tab.full"
          :unit="tab.unit"
          :precision="tab.precision"
          :playhead="motionTime"
          @update:model-value="onTrackUpdate(tab.key, $event)"
          @interaction-start="beginEditTransaction('motion-track')"
          @interaction-end="endEditTransaction"
        />
      </template>
      <p class="hint">Время дорожек считается от начала обрезанного клипа на выходе.</p>
    </div>

    <div class="motion-block">
      <div class="motion-head">
        <span class="motion-title">Рампы скорости</span>
        <span class="motion-readout">
          {{ formatSeconds(sourceSpan) }} → {{ formatSeconds(outputSpan) }}
        </span>
      </div>

      <div class="motion-chips">
        <button
          v-for="preset in RAMP_PRESETS"
          :key="preset.label"
          type="button"
          class="motion-chip"
          :title="preset.title"
          @click="applyRamp(preset)"
        >
          {{ preset.label }}
        </button>
        <button
          type="button"
          class="motion-chip"
          :disabled="!motionState.speedRamps.length"
          @click="transaction(() => setMotionTrack('speedRamps', []))"
        >
          Ровная скорость
        </button>
      </div>

      <KeyframeTrackEditor
        :model-value="motionState.speedRamps"
        :duration="sourceSpan"
        :min="MIN_RAMP_SPEED"
        :max="MAX_RAMP_SPEED"
        :neutral="1"
        label="Скорость по времени исходника"
        unit="×"
        :precision="2"
        :playhead="rampTime"
        @update:model-value="onTrackUpdate('speedRamps', $event)"
        @interaction-start="beginEditTransaction('motion-ramp')"
        @interaction-end="endEditTransaction"
      />

      <p class="hint">
        Минимум 0,25×, максимум 4×. Звук внутри участка идёт с постоянным темпом: точную рампу
        звука ffmpeg не умеет.
      </p>
    </div>

    <div class="motion-chips">
      <button type="button" class="motion-chip" :disabled="!activeTrackCount" @click="resetAll">
        Сбросить движение
      </button>
    </div>
  </div>
</template>

<style scoped>
.motion {
  display: grid;
  gap: 12px;
}

.motion-block {
  display: grid;
  gap: 8px;
  min-width: 0;
}

.motion-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.motion-title {
  font-size: 0.86rem;
  font-weight: 600;
}

.motion-readout {
  color: var(--muted);
  font-size: 0.76rem;
  font-variant-numeric: tabular-nums;
}

.motion-switch {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  color: var(--muted);
  font-size: 0.76rem;
  cursor: pointer;
}

.motion-tabs,
.motion-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.motion-chip {
  display: inline-flex;
  align-items: center;
  gap: 5px;
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

.motion-chip:hover:not(:disabled) {
  border-color: var(--border-strong);
}

.motion-chip[aria-pressed='true'] {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.motion-chip:disabled {
  cursor: not-allowed;
  opacity: 0.45;
}

.motion-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--accent);
}

.motion-note {
  margin: 0;
  color: var(--warn);
  font-size: 0.76rem;
}
</style>
