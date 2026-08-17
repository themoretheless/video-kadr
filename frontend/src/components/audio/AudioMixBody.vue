<script setup lang="ts">
// Audio mixing: extra tracks (music, voiceover, sfx), auto-ducking, the voice
// cleanup chain, the volume envelope and the export bitrate. Every control here
// maps onto a filter the render actually emits (see
// backend/src/render/graph/audio_mix.rs); the one exception is the bitrate
// selector, which carries a visible note until args.rs consumes it.
//
// AudioMixPanel.vue loads this body lazily, so the waveform decoding and the
// envelope editor never reach a user who does not open the section.
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  assetUrl,
  assetsOfKind,
  beginEditTransaction,
  endEditTransaction,
  findAsset,
  state,
} from '../../store'
import {
  addAudioTrack,
  addEnvelopePoint,
  audioMixState,
  AUDIO_BITRATE_OPTIONS,
  AUDIO_ROLE_LABELS,
  BITRATE_NOTE,
  defaultCompressor,
  defaultDucking,
  defaultGate,
  defaultLimiter,
  ENVELOPE_MAX_GAIN,
  ENVELOPE_TIME_STEP,
  envelopeInterpolation,
  envelopeTime,
  moveEnvelopePoint,
  NORMALIZE_VOLUME_NOTE,
  removeAudioTrack,
  removeEnvelopePoint,
  sampleEnvelope,
  withEnvelopeInterpolation,
} from '../../store/audioMix'
import { drawWaveform, loadWaveform, type WaveformPeaks } from '../../domain/waveform'
import { MAX_AUDIO_TRACKS, MAX_KEYFRAMES } from '../../store/validation'
import type { AudioRole, AudioTrackSpec, Interpolation } from '../../types'

const ROLES: readonly AudioRole[] = ['music', 'voiceover', 'sfx']
const INTERPOLATIONS: { value: Interpolation; label: string }[] = [
  { value: 'hold', label: 'Ступенькой' },
  { value: 'linear', label: 'Линейно' },
  { value: 'smooth', label: 'Плавно' },
]

/** Envelope plot geometry, in the SVG user space. */
const PLOT_WIDTH = 320
const PLOT_HEIGHT = 120
const PLOT_PADDING = 8
const PLOT_INNER_WIDTH = PLOT_WIDTH - PLOT_PADDING * 2
const PLOT_INNER_HEIGHT = PLOT_HEIGHT - PLOT_PADDING * 2

const pickedAsset = ref('')
const pickedRole = ref<AudioRole>('music')
const selectedPoint = ref(0)
const plot = ref<SVGSVGElement | null>(null)
const peaks = ref<Record<string, WaveformPeaks | null>>({})
const canvases = new Map<string, HTMLCanvasElement>()

let drag: { index: number; pointerId: number } | null = null

const audioAssets = computed(() => assetsOfKind('audio'))
const tracks = computed(() => audioMixState.tracks)
const dynamics = computed(() => audioMixState.dynamics)
const envelope = computed(() => audioMixState.dynamics.volumeEnvelope)

/** Output-timeline length: the trimmed range after the speed change. */
const outputDuration = computed(() => {
  const trimmed = Math.max(0, state.edit.trimEnd - state.edit.trimStart)
  const speed = state.edit.speed > 0 ? state.edit.speed : 1
  const seconds = trimmed > 0 ? trimmed / speed : (state.video?.duration ?? 0)
  return Number.isFinite(seconds) && seconds > 0 ? seconds : 0
})

/** Player time mapped onto the output timeline the envelope is drawn over. */
const playhead = computed(() => {
  const speed = state.edit.speed > 0 ? state.edit.speed : 1
  const seconds = (state.playerTime - state.edit.trimStart) / speed
  return Math.max(0, Math.min(outputDuration.value, Number.isFinite(seconds) ? seconds : 0))
})

const playheadGain = computed(() => sampleEnvelope(envelope.value, playhead.value))
const envelopeInterp = computed(() => envelopeInterpolation(envelope.value))

function percent(value: number): string {
  return `${Math.round(value * 100)}%`
}

function seconds(value: number): string {
  return `${value.toFixed(2)} с`
}

function assetLabel(assetId: string): string {
  return findAsset(assetId)?.filename || assetId
}

/** Write a clamped, finite number back into a reactive object. */
function setNumber(
  target: object,
  key: string,
  event: Event,
  min: number,
  max: number,
  fallback: number,
): void {
  const input = event.currentTarget as HTMLInputElement
  const raw = input.valueAsNumber
  const value = Number.isFinite(raw) ? Math.max(min, Math.min(max, raw)) : fallback
  ;(target as Record<string, unknown>)[key] = value
  input.value = String(value)
}

/** Same, for a nullable field: an empty input means "not set". */
function setNullable(target: object, key: string, event: Event, min: number, max: number): void {
  const input = event.currentTarget as HTMLInputElement
  const raw = input.valueAsNumber
  const value = Number.isFinite(raw) ? Math.max(min, Math.min(max, raw)) : null
  ;(target as Record<string, unknown>)[key] = value
  input.value = value === null ? '' : String(value)
}

function onAddTrack(): void {
  if (!addAudioTrack(pickedAsset.value, pickedRole.value)) return
  pickedAsset.value = ''
}

function toggleDucking(track: AudioTrackSpec, event: Event): void {
  const checked = (event.currentTarget as HTMLInputElement).checked
  track.ducking = checked ? (track.ducking ?? defaultDucking()) : null
  if (track.ducking) track.ducking.enabled = checked
}

// --- voice cleanup blocks: a checkbox owns the whole optional object ---

function toggleGate(event: Event): void {
  dynamics.value.gate = (event.currentTarget as HTMLInputElement).checked ? defaultGate() : null
}

function toggleCompressor(event: Event): void {
  dynamics.value.compressor = (event.currentTarget as HTMLInputElement).checked
    ? defaultCompressor()
    : null
}

function toggleLimiter(event: Event): void {
  dynamics.value.limiter = (event.currentTarget as HTMLInputElement).checked
    ? defaultLimiter()
    : null
}

function toggleHighpass(event: Event): void {
  dynamics.value.highpassHz = (event.currentTarget as HTMLInputElement).checked ? 80 : null
}

function toggleLowpass(event: Event): void {
  dynamics.value.lowpassHz = (event.currentTarget as HTMLInputElement).checked ? 12000 : null
}

// --- volume envelope ---

function toPlotX(time: number): number {
  const span = outputDuration.value || 1
  return PLOT_PADDING + Math.max(0, Math.min(1, time / span)) * PLOT_INNER_WIDTH
}

function toPlotY(gain: number): number {
  return PLOT_PADDING + (1 - Math.max(0, Math.min(1, gain / ENVELOPE_MAX_GAIN))) * PLOT_INNER_HEIGHT
}

const envelopePath = computed(() =>
  envelope.value
    .map((point, index) => {
      const previous = envelope.value[index - 1]
      const step =
        previous && envelopeInterp.value === 'hold'
          ? `L${toPlotX(point.t)},${toPlotY(previous.v)} `
          : ''
      return `${index === 0 ? 'M' : `${step}L`}${toPlotX(point.t)},${toPlotY(point.v)}`
    })
    .join(' '),
)

function pointFromEvent(event: PointerEvent): { t: number; v: number } | null {
  const element = plot.value
  if (!element) return null
  const rect = element.getBoundingClientRect()
  if (rect.width <= 0 || rect.height <= 0) return null
  const x = ((event.clientX - rect.left) / rect.width) * PLOT_WIDTH
  const y = ((event.clientY - rect.top) / rect.height) * PLOT_HEIGHT
  return {
    t: ((x - PLOT_PADDING) / PLOT_INNER_WIDTH) * (outputDuration.value || 1),
    v: (1 - (y - PLOT_PADDING) / PLOT_INNER_HEIGHT) * ENVELOPE_MAX_GAIN,
  }
}

/** Insert a point at the playhead, keeping the current value of the envelope. */
function addPointAtPlayhead(): void {
  if (envelope.value.length >= MAX_KEYFRAMES) return
  const time = playhead.value
  const next = addEnvelopePoint(envelope.value, time, playheadGain.value, outputDuration.value)
  dynamics.value.volumeEnvelope = next
  selectedPoint.value = Math.max(
    0,
    next.findIndex((point) => point.t >= time - ENVELOPE_TIME_STEP),
  )
}

function deleteSelectedPoint(): void {
  const index = selectedPoint.value
  if (!envelope.value[index]) return
  dynamics.value.volumeEnvelope = removeEnvelopePoint(envelope.value, index)
  selectedPoint.value = Math.max(0, Math.min(index, envelope.value.length - 1))
}

function clearEnvelope(): void {
  dynamics.value.volumeEnvelope = []
  selectedPoint.value = 0
}

function setInterpolation(event: Event): void {
  const value = (event.currentTarget as HTMLSelectElement).value as Interpolation
  dynamics.value.volumeEnvelope = withEnvelopeInterpolation(envelope.value, value)
}

/** A click on empty plot area adds a point and starts dragging it right away. */
function onPlotPointerDown(event: PointerEvent): void {
  if (event.button !== 0 || envelope.value.length >= MAX_KEYFRAMES) return
  const requested = pointFromEvent(event)
  if (!requested) return
  // One transaction covers both the insert and the drag that follows.
  beginEditTransaction('audio-envelope')
  const next = addEnvelopePoint(envelope.value, requested.t, requested.v, outputDuration.value)
  dynamics.value.volumeEnvelope = next
  const time = envelopeTime(requested.t, outputDuration.value)
  const index = next.findIndex((point) => point.t === time)
  startDrag(Math.max(0, index), event, false)
}

function startDrag(index: number, event: PointerEvent, openTransaction = true): void {
  if (event.button !== 0) return
  if (openTransaction) beginEditTransaction('audio-envelope')
  selectedPoint.value = index
  drag = { index, pointerId: event.pointerId }
  plot.value?.setPointerCapture?.(event.pointerId)
  window.addEventListener('pointermove', onPointerMove)
  window.addEventListener('pointerup', stopDrag)
  window.addEventListener('pointercancel', stopDrag)
  event.preventDefault()
}

function onPointerMove(event: PointerEvent): void {
  if (!drag || event.pointerId !== drag.pointerId) return
  const requested = pointFromEvent(event)
  if (!requested) return
  dynamics.value.volumeEnvelope = moveEnvelopePoint(
    envelope.value,
    drag.index,
    requested.t,
    requested.v,
    outputDuration.value,
  )
}

function stopDrag(): void {
  if (!drag) return
  if (plot.value?.hasPointerCapture?.(drag.pointerId)) {
    plot.value.releasePointerCapture(drag.pointerId)
  }
  drag = null
  window.removeEventListener('pointermove', onPointerMove)
  window.removeEventListener('pointerup', stopDrag)
  window.removeEventListener('pointercancel', stopDrag)
  endEditTransaction()
}

/** Arrows nudge the selected point; Delete removes it. */
function onPointKeydown(index: number, event: KeyboardEvent): void {
  selectedPoint.value = index
  const point = envelope.value[index]
  if (!point) return
  if (event.key === 'Delete' || event.key === 'Backspace') {
    event.preventDefault()
    deleteSelectedPoint()
    return
  }
  const gainStep = event.shiftKey ? 0.01 : 0.05
  const timeStep = event.shiftKey ? ENVELOPE_TIME_STEP : 0.1
  let time = point.t
  let gain = point.v
  if (event.key === 'ArrowUp') gain += gainStep
  else if (event.key === 'ArrowDown') gain -= gainStep
  else if (event.key === 'ArrowRight') time += timeStep
  else if (event.key === 'ArrowLeft') time -= timeStep
  else if (event.key === 'Home') time = 0
  else if (event.key === 'End') time = outputDuration.value
  else return
  event.preventDefault()
  dynamics.value.volumeEnvelope = moveEnvelopePoint(
    envelope.value,
    index,
    time,
    gain,
    outputDuration.value,
  )
}

// --- waveforms ---

function registerCanvas(assetId: string, element: unknown): void {
  if (element instanceof HTMLCanvasElement) {
    canvases.set(assetId, element)
    paint(assetId)
  } else {
    canvases.delete(assetId)
  }
}

function paint(assetId: string): void {
  const canvas = canvases.get(assetId)
  if (!canvas) return
  const style = getComputedStyle(canvas)
  drawWaveform(canvas, peaks.value[assetId] ?? null, { color: style.color })
}

function paintAll(): void {
  for (const assetId of canvases.keys()) paint(assetId)
}

async function ensurePeaks(assetId: string): Promise<void> {
  if (assetId in peaks.value) return
  const asset = findAsset(assetId)
  if (!asset) return
  peaks.value = { ...peaks.value, [assetId]: null }
  const decoded = await loadWaveform(assetUrl(asset))
  peaks.value = { ...peaks.value, [assetId]: decoded }
  paint(assetId)
}

watch(
  () => tracks.value.map((track) => track.assetId),
  (ids) => {
    for (const id of ids) void ensurePeaks(id)
  },
  { immediate: true },
)

watch(
  () => envelope.value.length,
  (length) => {
    selectedPoint.value = Math.max(0, Math.min(selectedPoint.value, length - 1))
  },
)

onMounted(() => window.addEventListener('resize', paintAll))
onBeforeUnmount(() => {
  stopDrag()
  window.removeEventListener('resize', paintAll)
})
</script>

<template>
  <div class="audio-mix">
    <p v-if="!audioAssets.length" class="hint">
      Загрузите аудиофайл в библиотеку ассетов, чтобы добавить музыку, озвучку или эффект.
    </p>

    <div class="field">
      <label for="audio-add-asset">Добавить дорожку</label>
      <div class="audio-add">
        <select id="audio-add-asset" v-model="pickedAsset" :disabled="!audioAssets.length">
          <option value="">Выберите файл</option>
          <option v-for="asset in audioAssets" :key="asset.id" :value="asset.id">
            {{ asset.filename || asset.id }}
          </option>
        </select>
        <select v-model="pickedRole" aria-label="Роль дорожки">
          <option v-for="role in ROLES" :key="role" :value="role">
            {{ AUDIO_ROLE_LABELS[role] }}
          </option>
        </select>
        <button
          type="button"
          class="btn sm"
          :disabled="!pickedAsset || tracks.length >= MAX_AUDIO_TRACKS"
          @click="onAddTrack"
        >
          Добавить
        </button>
      </div>
      <p class="hint">Не больше {{ MAX_AUDIO_TRACKS }} дорожек. Сейчас: {{ tracks.length }}.</p>
    </div>

    <ul class="audio-tracks">
      <li v-for="(track, index) in tracks" :key="`${track.assetId}-${index}`" class="audio-track">
        <div class="audio-track-head">
          <strong class="audio-track-name">{{ assetLabel(track.assetId) }}</strong>
          <select v-model="track.role" :aria-label="`Роль дорожки ${index + 1}`">
            <option v-for="role in ROLES" :key="role" :value="role">
              {{ AUDIO_ROLE_LABELS[role] }}
            </option>
          </select>
          <button
            type="button"
            class="btn sm danger"
            :aria-label="`Удалить дорожку ${assetLabel(track.assetId)}`"
            @click="removeAudioTrack(index)"
          >
            Удалить
          </button>
        </div>

        <canvas
          :ref="(element) => registerCanvas(track.assetId, element)"
          class="audio-wave"
          aria-hidden="true"
        ></canvas>

        <label class="audio-gain">
          <span>Громкость: {{ percent(track.gain ?? 1) }}</span>
          <input
            v-model.number="track.gain"
            type="range"
            min="0"
            max="4"
            step="0.05"
            :aria-valuetext="percent(track.gain ?? 1)"
          />
        </label>

        <div class="grid2">
          <label>
            <span>Начало на таймлайне, с</span>
            <input
              type="number"
              min="0"
              step="0.1"
              :value="track.start ?? 0"
              @change="setNumber(track, 'start', $event, 0, 86400, 0)"
            />
          </label>
          <label>
            <span>Точка входа в файле, с</span>
            <input
              type="number"
              min="0"
              step="0.1"
              :value="track.sourceStart ?? 0"
              @change="setNumber(track, 'sourceStart', $event, 0, 86400, 0)"
            />
          </label>
          <label>
            <span>Конец на таймлайне, с</span>
            <input
              type="number"
              min="0"
              step="0.1"
              placeholder="до конца"
              :value="track.end ?? ''"
              @change="setNullable(track, 'end', $event, 0, 86400)"
            />
          </label>
          <label>
            <span>Нарастание, с</span>
            <input
              type="number"
              min="0"
              max="60"
              step="0.1"
              :value="track.fadeIn ?? 0"
              @change="setNumber(track, 'fadeIn', $event, 0, 60, 0)"
            />
          </label>
          <label>
            <span>Затухание, с</span>
            <input
              type="number"
              min="0"
              max="60"
              step="0.1"
              :value="track.fadeOut ?? 0"
              @change="setNumber(track, 'fadeOut', $event, 0, 60, 0)"
            />
          </label>
        </div>

        <label class="toggle">
          <input v-model="track.loop" type="checkbox" />
          Зациклить до конца отрезка
        </label>

        <label class="toggle">
          <input
            type="checkbox"
            :checked="track.ducking?.enabled === true"
            @change="toggleDucking(track, $event)"
          />
          Приглушать под голос
        </label>
        <p class="hint">Дорожка автоматически тише, когда в основном звуке есть речь.</p>

        <details v-if="track.ducking?.enabled" class="audio-advanced">
          <summary>Настройки приглушения</summary>
          <div class="grid2">
            <label>
              <span>Порог срабатывания: {{ percent(track.ducking.threshold) }}</span>
              <input
                v-model.number="track.ducking.threshold"
                type="range"
                min="0"
                max="1"
                step="0.01"
                :aria-valuetext="percent(track.ducking.threshold)"
              />
            </label>
            <label>
              <span>Сила приглушения: {{ track.ducking.ratio.toFixed(1) }}:1</span>
              <input
                v-model.number="track.ducking.ratio"
                type="range"
                min="1"
                max="20"
                step="0.5"
                :aria-valuetext="`${track.ducking.ratio.toFixed(1)} к одному`"
              />
            </label>
            <label>
              <span>Время срабатывания, мс</span>
              <input
                type="number"
                min="1"
                max="2000"
                step="1"
                :value="track.ducking.attack"
                @change="setNumber(track.ducking!, 'attack', $event, 0.01, 2000, 20)"
              />
            </label>
            <label>
              <span>Время восстановления, мс</span>
              <input
                type="number"
                min="1"
                max="9000"
                step="1"
                :value="track.ducking.release"
                @change="setNumber(track.ducking!, 'release', $event, 0.01, 9000, 300)"
              />
            </label>
          </div>
        </details>
      </li>
    </ul>

    <h4 class="audio-heading">Чистка голоса</h4>

    <label class="audio-gain">
      <span>Шумоподавление: {{ percent(dynamics.denoise) }}</span>
      <input
        v-model.number="dynamics.denoise"
        type="range"
        min="0"
        max="1"
        step="0.05"
        :aria-valuetext="percent(dynamics.denoise)"
      />
    </label>

    <label class="toggle">
      <input v-model="dynamics.dereverb" type="checkbox" />
      Убрать эхо комнаты
    </label>
    <label class="toggle">
      <input v-model="dynamics.deesser" type="checkbox" />
      Смягчить свистящие «с» и «ш»
    </label>
    <label class="toggle">
      <input type="checkbox" :checked="dynamics.gate !== null" @change="toggleGate" />
      Убрать тихий фон в паузах
    </label>
    <label class="toggle">
      <input type="checkbox" :checked="dynamics.compressor !== null" @change="toggleCompressor" />
      Выровнять громкие и тихие места
    </label>
    <label class="toggle">
      <input type="checkbox" :checked="dynamics.limiter !== null" @change="toggleLimiter" />
      Не давать звуку перегружаться
    </label>
    <label class="toggle">
      <input type="checkbox" :checked="dynamics.highpassHz !== null" @change="toggleHighpass" />
      Убрать низкий гул
    </label>
    <label class="toggle">
      <input type="checkbox" :checked="dynamics.lowpassHz !== null" @change="toggleLowpass" />
      Убрать верхнее шипение
    </label>

    <details class="audio-advanced">
      <summary>Точные значения</summary>
      <div class="grid2">
        <label v-if="dynamics.gate">
          <span>Порог шумоподавителя, дБ</span>
          <input
            type="number"
            min="-90"
            max="0"
            step="1"
            :value="dynamics.gate.threshold"
            @change="setNumber(dynamics.gate!, 'threshold', $event, -90, 0, -45)"
          />
        </label>
        <label v-if="dynamics.gate">
          <span>Сила шумоподавителя</span>
          <input
            type="number"
            min="1"
            max="20"
            step="0.5"
            :value="dynamics.gate.ratio"
            @change="setNumber(dynamics.gate!, 'ratio', $event, 1, 20, 2)"
          />
        </label>
        <label v-if="dynamics.compressor">
          <span>Порог компрессора, дБ</span>
          <input
            type="number"
            min="-60"
            max="0"
            step="1"
            :value="dynamics.compressor.threshold"
            @change="setNumber(dynamics.compressor!, 'threshold', $event, -60, 0, -18)"
          />
        </label>
        <label v-if="dynamics.compressor">
          <span>Степень сжатия</span>
          <input
            type="number"
            min="1"
            max="20"
            step="0.5"
            :value="dynamics.compressor.ratio"
            @change="setNumber(dynamics.compressor!, 'ratio', $event, 1, 20, 3)"
          />
        </label>
        <label v-if="dynamics.compressor">
          <span>Атака, мс</span>
          <input
            type="number"
            min="1"
            max="2000"
            step="1"
            :value="dynamics.compressor.attack"
            @change="setNumber(dynamics.compressor!, 'attack', $event, 0.01, 2000, 20)"
          />
        </label>
        <label v-if="dynamics.compressor">
          <span>Восстановление, мс</span>
          <input
            type="number"
            min="1"
            max="9000"
            step="1"
            :value="dynamics.compressor.release"
            @change="setNumber(dynamics.compressor!, 'release', $event, 0.01, 9000, 250)"
          />
        </label>
        <label v-if="dynamics.compressor">
          <span>Добавка громкости</span>
          <input
            type="number"
            min="1"
            max="64"
            step="0.1"
            :value="dynamics.compressor.makeup"
            @change="setNumber(dynamics.compressor!, 'makeup', $event, 1, 64, 1)"
          />
        </label>
        <label v-if="dynamics.limiter">
          <span>Потолок, дБ</span>
          <input
            type="number"
            min="-30"
            max="0"
            step="0.1"
            :value="dynamics.limiter.ceiling"
            @change="setNumber(dynamics.limiter!, 'ceiling', $event, -30, 0, -1)"
          />
        </label>
        <label v-if="dynamics.highpassHz !== null">
          <span>Срез снизу, Гц</span>
          <input
            type="number"
            min="10"
            max="20000"
            step="10"
            :value="dynamics.highpassHz"
            @change="setNumber(dynamics, 'highpassHz', $event, 10, 20000, 80)"
          />
        </label>
        <label v-if="dynamics.lowpassHz !== null">
          <span>Срез сверху, Гц</span>
          <input
            type="number"
            min="10"
            max="20000"
            step="100"
            :value="dynamics.lowpassHz"
            @change="setNumber(dynamics, 'lowpassHz', $event, 10, 20000, 12000)"
          />
        </label>
      </div>
    </details>

    <p class="hint">{{ NORMALIZE_VOLUME_NOTE }}</p>

    <h4 class="audio-heading">Огибающая громкости</h4>
    <p id="audio-envelope-help" class="hint">
      Нажмите на график, чтобы добавить точку. Стрелки двигают выбранную точку, Delete удаляет её.
      Точки хранятся с шагом 1 мс.
    </p>

    <div class="audio-envelope-toolbar">
      <span class="audio-readout" aria-live="polite">
        На позиции {{ seconds(playhead) }}: {{ percent(playheadGain) }}
      </span>
      <button
        type="button"
        class="btn sm"
        :disabled="envelope.length >= MAX_KEYFRAMES || outputDuration <= 0"
        @click="addPointAtPlayhead"
      >
        Точка на позиции
      </button>
      <button
        type="button"
        class="btn sm"
        :disabled="!envelope.length"
        @click="deleteSelectedPoint"
      >
        Удалить точку
      </button>
      <button type="button" class="btn sm" :disabled="!envelope.length" @click="clearEnvelope">
        Сбросить
      </button>
    </div>

    <svg
      ref="plot"
      class="audio-envelope"
      :viewBox="`0 0 ${PLOT_WIDTH} ${PLOT_HEIGHT}`"
      role="group"
      aria-label="Огибающая громкости"
      aria-describedby="audio-envelope-help"
      @pointerdown="onPlotPointerDown"
    >
      <rect
        class="envelope-background"
        :x="PLOT_PADDING"
        :y="PLOT_PADDING"
        :width="PLOT_INNER_WIDTH"
        :height="PLOT_INNER_HEIGHT"
        rx="2"
      />
      <line
        class="envelope-unity"
        :x1="PLOT_PADDING"
        :x2="PLOT_PADDING + PLOT_INNER_WIDTH"
        :y1="toPlotY(1)"
        :y2="toPlotY(1)"
      />
      <line
        class="envelope-playhead"
        :x1="toPlotX(playhead)"
        :x2="toPlotX(playhead)"
        :y1="PLOT_PADDING"
        :y2="PLOT_PADDING + PLOT_INNER_HEIGHT"
      />
      <path class="envelope-line" :d="envelopePath" />
      <g
        v-for="(point, index) in envelope"
        :key="`${point.t}-${index}`"
        class="envelope-point"
        :class="{ selected: selectedPoint === index }"
        :transform="`translate(${toPlotX(point.t)} ${toPlotY(point.v)})`"
        role="slider"
        tabindex="0"
        :aria-valuemin="0"
        :aria-valuemax="ENVELOPE_MAX_GAIN"
        :aria-valuenow="point.v"
        :aria-valuetext="`${seconds(point.t)}, ${percent(point.v)}`"
        :aria-label="`Точка ${index + 1} огибающей`"
        @focus="selectedPoint = index"
        @pointerdown.stop="startDrag(index, $event)"
        @keydown="onPointKeydown(index, $event)"
      >
        <circle class="point-hit" r="10" />
        <circle class="point-ring" r="7" />
        <circle class="point-dot" r="4" />
      </g>
    </svg>

    <div class="grid2">
      <label>
        <span>Переход между точками</span>
        <select :value="envelopeInterp" @change="setInterpolation">
          <option v-for="option in INTERPOLATIONS" :key="option.value" :value="option.value">
            {{ option.label }}
          </option>
        </select>
      </label>
      <label>
        <span>Битрейт звука, кбит/с</span>
        <select v-model.number="dynamics.bitrateKbps" aria-describedby="audio-bitrate-note">
          <option v-for="rate in AUDIO_BITRATE_OPTIONS" :key="rate" :value="rate">
            {{ rate }}
          </option>
        </select>
      </label>
    </div>
    <p class="hint">Переход применяется ко всей огибающей: сервер берёт его из первой точки.</p>
    <p id="audio-bitrate-note" class="hint">{{ BITRATE_NOTE }}</p>
  </div>
</template>

<style scoped>
.audio-add {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.audio-add select {
  flex: 1 1 140px;
  min-width: 0;
}

select,
.audio-tracks input[type='number'] {
  min-height: 34px;
  padding: 6px 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--panel-2);
  color: var(--text);
  font: inherit;
}

.audio-tracks {
  list-style: none;
  margin: 0 0 14px;
  padding: 0;
  display: grid;
  gap: 10px;
}

.audio-track {
  display: grid;
  gap: 8px;
  padding: 10px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--panel-3);
}

.audio-track-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.audio-track-name {
  flex: 1 1 120px;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
}

.audio-wave {
  display: block;
  width: 100%;
  height: 38px;
  color: var(--accent);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  background: var(--panel-2);
}

.audio-gain {
  display: grid;
  gap: 4px;
  font-size: 13px;
  color: var(--muted);
}

.audio-gain input[type='range'] {
  width: 100%;
  accent-color: var(--accent);
}

.audio-advanced {
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: 8px 10px;
}

.audio-advanced summary {
  cursor: pointer;
  font-size: 13px;
  color: var(--muted);
}

.audio-heading {
  margin: 16px 0 8px;
  font-size: 12px;
  text-transform: uppercase;
  color: var(--faint);
}

.audio-envelope-toolbar {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
  margin-bottom: 8px;
}

.audio-readout {
  margin-right: auto;
  font-size: 12px;
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}

.audio-envelope {
  display: block;
  width: 100%;
  height: auto;
  aspect-ratio: 8 / 3;
  touch-action: none;
  cursor: crosshair;
}

.envelope-background {
  fill: var(--panel-2);
  stroke: var(--border-strong);
  vector-effect: non-scaling-stroke;
}

.envelope-unity,
.envelope-playhead {
  stroke: var(--faint);
  stroke-dasharray: 3 3;
  vector-effect: non-scaling-stroke;
}

.envelope-playhead {
  stroke: var(--accent);
  stroke-dasharray: none;
}

.envelope-line {
  fill: none;
  stroke: var(--accent);
  stroke-width: 2;
  pointer-events: none;
  vector-effect: non-scaling-stroke;
}

.envelope-point {
  outline: none;
  cursor: grab;
}

.point-hit {
  fill: transparent;
}

.point-ring {
  fill: none;
  stroke: var(--accent);
  stroke-width: 2;
  opacity: 0;
  vector-effect: non-scaling-stroke;
}

.point-dot {
  fill: var(--panel-2);
  stroke: var(--accent);
  stroke-width: 2;
  pointer-events: none;
  vector-effect: non-scaling-stroke;
}

.envelope-point.selected .point-dot {
  fill: var(--accent);
}

.envelope-point:focus-visible .point-ring {
  opacity: 1;
}
</style>
