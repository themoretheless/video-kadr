<script setup lang="ts">
// Insta360-style reframing, stabilization and lens correction. Every control
// here maps onto something `render/graph/spatial.rs` actually emits; the notes
// spell out the three places where the render stage deliberately ignores a
// setting (roll under horizon lock, field of view for an equirectangular
// output, the stabilizer horizon lock in fast mode).

import { computed, ref, watch } from 'vue'
import { beginEditTransaction, endEditTransaction, state } from '../../store'
import {
  axisRange,
  clearReframeKeyframes,
  defaultFov,
  fovRange,
  hasReframeAnimation,
  isViewOffKeyframes,
  MAX_PITCH,
  REFRAME_AXES,
  reframeKeyframeTimes,
  resetLensCorrection,
  setReframeKeyframe,
  setReframeTrack,
  setView,
  spatialActive,
  spatialState,
  syncViewToPlayhead,
  type ReframeAxis,
} from '../../store/spatial'
import type { InputProjection, KeyframeTrack, OutputProjection, StabilizeMode } from '../../types'
// The shared keyframe lane from the motion feature: one editor per reframe axis
// instead of a second, weaker lane of our own.
import KeyframeTrackEditor from '../keyframe/KeyframeTrackEditor.vue'
import ReframeViewport from './ReframeViewport.vue'

const reframe = computed(() => spatialState.reframe360)
const stabilize = computed(() => spatialState.stabilize)
const lens = computed(() => spatialState.lensCorrection)

const inputProjections: { value: InputProjection; label: string }[] = [
  { value: 'equirect', label: 'Равнопромежуточная 360' },
  { value: 'fisheye', label: 'Fisheye' },
  { value: 'dfisheye', label: 'Двойной fisheye' },
]

const outputProjections: { value: OutputProjection; label: string }[] = [
  { value: 'flat', label: 'Плоская' },
  { value: 'equirect', label: 'Равнопромежуточная' },
  { value: 'fisheye', label: 'Fisheye' },
  { value: 'stereographic', label: 'Стереографическая' },
  { value: 'pannini', label: 'Паннини' },
]

const axisLabels: Record<ReframeAxis, string> = {
  yaw: 'Рыскание',
  pitch: 'Тангаж',
  roll: 'Крен',
  fov: 'Поле зрения',
}

const stabilizeModes: { value: StabilizeMode; label: string }[] = [
  { value: 'off', label: 'Выкл' },
  { value: 'fast', label: 'Быстрая' },
  { value: 'precise', label: 'Точная' },
]

// Keyframe times live on the OUTPUT timeline, the player runs on the source
// one. Speed ramps from the motion feature are not folded in here: this maps
// the plain trim and speed, which is what the reframe stage sees.
const editSpeed = computed(() => (state.edit.speed > 0 ? state.edit.speed : 1))
const outputSpan = computed(() =>
  Math.max(0.05, (state.edit.trimEnd - state.edit.trimStart) / editSpeed.value),
)
const outputTime = computed(() =>
  Math.max(0, Math.min(outputSpan.value, (state.playerTime - state.edit.trimStart) / editSpeed.value)),
)

const keyframeCount = computed(() => reframeKeyframeTimes().length)
const fovLimits = computed(() => fovRange(reframe.value.outputProjection))
const fovUsed = computed(() => defaultFov(reframe.value.outputProjection) !== null)
const viewDirty = computed(() => isViewOffKeyframes(outputTime.value))

/** Axes worth keyframing: roll and the field of view can both be inert. */
const editableAxes = computed(() =>
  REFRAME_AXES.filter((axis) => {
    if (axis === 'roll') return !reframe.value.horizonLock
    if (axis === 'fov') return fovUsed.value
    return true
  }),
)
const activeAxis = ref<ReframeAxis>('yaw')

watch(editableAxes, (axes) => {
  if (!axes.includes(activeAxis.value)) activeAxis.value = axes[0] ?? 'yaw'
})

/** The reframe stage refuses a source that is not plausibly a sphere. */
const projectionWarning = computed(() => {
  const video = state.video
  if (!reframe.value.enabled || reframe.value.inputProjection !== 'equirect') return ''
  if (!video || !video.width || !video.height) return ''
  const aspect = video.width / video.height
  if (Math.abs(aspect - 2) <= 0.2) return ''
  return `Источник ${video.width}×${video.height} не похож на сферу 2:1. Сервер откажет в перекадрировании 360, выберите другую входную проекцию.`
})

/** Two-pass stabilization is indexed by frame number, so a recut breaks it. */
const preciseTimelineWarning = computed(() =>
  stabilize.value.mode === 'precise' && state.edit.cutEnabled
    ? 'Точная стабилизация не работает вместе с вырезанием куска: сервер вернёт ошибку. Отключите вырез или выберите быструю стабилизацию.'
    : '',
)

// The camera follows the keyframes as the playhead moves, so the viewport and
// the readouts always show the framing the render would produce.
watch(outputTime, (time) => {
  if (reframe.value.enabled) syncViewToPlayhead(time)
})

const limitHit = ref(false)

function setInputProjection(value: InputProjection): void {
  reframe.value.inputProjection = value
}

function setOutputProjection(value: OutputProjection): void {
  reframe.value.outputProjection = value
  // Each projection has its own usable field of view band.
  setView({})
}

function setOutputSize(width: number, height: number): void {
  beginEditTransaction('reframe-size')
  reframe.value.outputWidth = width
  reframe.value.outputHeight = height
  endEditTransaction()
}

function addKeyframe(): void {
  beginEditTransaction('reframe-keyframe')
  const added = setReframeKeyframe(outputTime.value)
  endEditTransaction()
  limitHit.value = !added
}

function clearKeyframes(): void {
  beginEditTransaction('reframe-keyframe')
  clearReframeKeyframes(outputTime.value)
  endEditTransaction()
  limitHit.value = false
}

function onTrackUpdate(axis: ReframeAxis, track: KeyframeTrack): void {
  setReframeTrack(axis, track)
  limitHit.value = false
}

const outputSizes = [
  { label: '1920×1080', w: 1920, h: 1080 },
  { label: '1080×1920', w: 1080, h: 1920 },
  { label: '1080×1080', w: 1080, h: 1080 },
  { label: '2560×1440', w: 2560, h: 1440 },
]
</script>

<template>
  <div class="spatial-body">
    <!-- 360 reframe -->
    <div class="field">
      <label class="toggle">
        <input type="checkbox" v-model="reframe.enabled" /> Перекадрирование 360
      </label>
      <p class="hint">
        Разворачивает сферическое видео в обычный кадр. Тяните картинку мышью: по горизонтали
        рыскание, по вертикали тангаж, колесо или щипок меняют поле зрения.
      </p>
    </div>

    <template v-if="reframe.enabled">
      <div class="field">
        <label>Проекция источника</label>
        <div class="chips" role="group" aria-label="Проекция источника">
          <button
            v-for="projection in inputProjections"
            :key="projection.value"
            type="button"
            class="chip"
            :class="{ active: reframe.inputProjection === projection.value }"
            :aria-pressed="reframe.inputProjection === projection.value"
            @click="setInputProjection(projection.value)"
          >
            {{ projection.label }}
          </button>
        </div>
      </div>

      <p v-if="projectionWarning" class="hint spatial-warning" role="alert">
        {{ projectionWarning }}
      </p>

      <div class="field">
        <label>Проекция результата</label>
        <div class="chips" role="group" aria-label="Проекция результата">
          <button
            v-for="projection in outputProjections"
            :key="projection.value"
            type="button"
            class="chip"
            :class="{ active: reframe.outputProjection === projection.value }"
            :aria-pressed="reframe.outputProjection === projection.value"
            @click="setOutputProjection(projection.value)"
          >
            {{ projection.label }}
          </button>
        </div>
      </div>

      <div class="field">
        <ReframeViewport />
      </div>

      <div class="field">
        <div class="grid2">
          <label>
            Рыскание: {{ Math.round(reframe.view.yaw) }}°
            <input
              type="range"
              min="-180"
              max="180"
              step="1"
              :value="reframe.view.yaw"
              @input="setView({ yaw: Number(($event.target as HTMLInputElement).value) })"
            />
          </label>
          <label>
            Тангаж: {{ Math.round(reframe.view.pitch) }}°
            <input
              type="range"
              :min="-MAX_PITCH"
              :max="MAX_PITCH"
              step="1"
              :value="reframe.view.pitch"
              @input="setView({ pitch: Number(($event.target as HTMLInputElement).value) })"
            />
          </label>
          <label v-if="!reframe.horizonLock">
            Крен: {{ Math.round(reframe.view.roll) }}°
            <input
              type="range"
              min="-180"
              max="180"
              step="1"
              :value="reframe.view.roll"
              @input="setView({ roll: Number(($event.target as HTMLInputElement).value) })"
            />
          </label>
          <label v-if="fovUsed">
            Поле зрения: {{ Math.round(reframe.view.fov) }}°
            <input
              type="range"
              :min="fovLimits[0]"
              :max="fovLimits[1]"
              step="1"
              :value="reframe.view.fov"
              @input="setView({ fov: Number(($event.target as HTMLInputElement).value) })"
            />
          </label>
        </div>
        <p v-if="!fovUsed" class="hint">
          Равнопромежуточный результат показывает всю сферу: поле зрения не применяется.
        </p>
      </div>

      <div class="field">
        <label class="toggle">
          <input type="checkbox" v-model="reframe.horizonLock" /> Держать горизонт
        </label>
        <p class="hint">
          Горизонт удерживается обнулением крена: пока переключатель включён, крен не применяется.
        </p>
      </div>

      <div class="field">
        <label>Размер результата</label>
        <div class="chips" role="group" aria-label="Размер результата">
          <button
            v-for="size in outputSizes"
            :key="size.label"
            type="button"
            class="chip"
            :class="{ active: reframe.outputWidth === size.w && reframe.outputHeight === size.h }"
            :aria-pressed="reframe.outputWidth === size.w && reframe.outputHeight === size.h"
            @click="setOutputSize(size.w, size.h)"
          >
            {{ size.label }}
          </button>
        </div>
      </div>

      <div class="field">
        <label>Ключи вида</label>
        <div class="chips">
          <button type="button" class="btn ghost sm" @click="addKeyframe">Поставить ключ</button>
          <button
            type="button"
            class="btn ghost sm"
            :disabled="!keyframeCount"
            @click="clearKeyframes"
          >
            Убрать все ключи
          </button>
        </div>
        <div class="chips" role="group" aria-label="Ось ключей">
          <button
            v-for="axis in editableAxes"
            :key="axis"
            type="button"
            class="chip"
            :class="{ active: activeAxis === axis }"
            :aria-pressed="activeAxis === axis"
            @click="activeAxis = axis"
          >
            {{ axisLabels[axis] }}
          </button>
        </div>
        <template v-for="axis in editableAxes" :key="axis">
          <KeyframeTrackEditor
            v-if="activeAxis === axis"
            :model-value="reframe[axis]"
            :duration="outputSpan"
            :min="axisRange(axis).min"
            :max="axisRange(axis).max"
            :neutral="axisRange(axis).neutral"
            :label="axisLabels[axis]"
            unit="°"
            :precision="0"
            :playhead="outputTime"
            @update:model-value="onTrackUpdate(axis, $event)"
            @interaction-start="beginEditTransaction('reframe-track')"
            @interaction-end="endEditTransaction"
          />
        </template>
        <p class="hint">
          Ключ записывает текущий вид сразу на все оси в позиции плеера. Время дорожек считается
          от начала обрезанного клипа на выходе.
        </p>
        <p v-if="limitHit" class="hint spatial-warning" role="alert">
          Достигнут предел в 64 ключа на ось.
        </p>
        <p v-if="viewDirty" class="hint spatial-warning" role="status">
          Текущий вид отличается от ключей: нажмите «Поставить ключ», иначе он не попадёт в экспорт.
        </p>
        <p v-else-if="!hasReframeAnimation()" class="hint">
          Без ключей вид статичен и уходит в рендер как есть.
        </p>
      </div>
    </template>

    <!-- Stabilization -->
    <div class="field">
      <label>Стабилизация</label>
      <div class="chips" role="group" aria-label="Стабилизация">
        <button
          v-for="mode in stabilizeModes"
          :key="mode.value"
          type="button"
          class="chip"
          :class="{ active: stabilize.mode === mode.value }"
          :aria-pressed="stabilize.mode === mode.value"
          @click="stabilize.mode = mode.value"
        >
          {{ mode.label }}
        </button>
      </div>
    </div>

    <template v-if="stabilize.mode !== 'off'">
      <div class="field">
        <div class="grid2">
          <label>
            Сглаживание: {{ Math.round(stabilize.smoothing) }}
            <input type="range" min="1" max="100" step="1" v-model.number="stabilize.smoothing" />
          </label>
          <label>
            Запас по краям: {{ stabilize.zoom.toFixed(1) }}%
            <input type="range" min="0" max="20" step="0.5" v-model.number="stabilize.zoom" />
          </label>
        </div>
        <p class="hint">Запас подрезает кадр, чтобы не показывать пустые края после смещения.</p>
      </div>

      <div v-if="stabilize.mode === 'precise'" class="field">
        <label class="toggle">
          <input type="checkbox" v-model="stabilize.horizonLock" /> Выравнивать горизонт
        </label>
      </div>
      <p v-else class="hint">
        Быстрая стабилизация убирает только смещение, поэтому выравнивание горизонта здесь
        недоступно.
      </p>

      <p v-if="stabilize.mode === 'precise'" class="hint spatial-warning" role="status">
        «Точная» - это два прохода: сначала анализ всего клипа, потом рендер. Экспорт занимает
        примерно вдвое больше времени.
      </p>
      <p v-if="preciseTimelineWarning" class="hint spatial-warning" role="alert">
        {{ preciseTimelineWarning }}
      </p>
    </template>

    <!-- Lens correction -->
    <div class="field">
      <label>Коррекция объектива</label>
      <div class="grid2">
        <label>
          k1: {{ lens.k1.toFixed(3) }}
          <input type="range" min="-1" max="1" step="0.005" v-model.number="lens.k1" />
        </label>
        <label>
          k2: {{ lens.k2.toFixed(3) }}
          <input type="range" min="-1" max="1" step="0.005" v-model.number="lens.k2" />
        </label>
      </div>
      <div class="chips">
        <button
          type="button"
          class="btn ghost sm"
          :disabled="lens.k1 === 0 && lens.k2 === 0"
          @click="resetLensCorrection"
        >
          Сбросить объектив
        </button>
      </div>
      <p class="hint">
        Отрицательный k1 выпрямляет «бочку» широкоугольных объективов, положительный добавляет её.
      </p>
    </div>

    <p v-if="!spatialActive()" class="hint">
      Пока ничего не включено: раздел не попадёт в задание на экспорт.
    </p>
  </div>
</template>

<style scoped>
.spatial-warning {
  color: var(--warn);
}
</style>
