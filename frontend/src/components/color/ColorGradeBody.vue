<script setup lang="ts">
// Body of the colour grading panel. Lazy-loaded by ColorGradePanel.vue so the
// wheels, the scope math and the canvas work stay out of the initial bundle.
import { computed, ref, toRef } from 'vue'
import type { ScopeGrade } from '../../domain/scopes'
import {
  beginEditTransaction,
  endEditTransaction,
} from '../../store'
import { colorAdvancedState, resetColorAdvanced } from '../../store/colorAdvanced'
import type { HslBand } from '../../types'
import ColorWheel from './ColorWheel.vue'
import CompareWipe from './CompareWipe.vue'
import ScopesView from './ScopesView.vue'
import { useFrameGrabber } from './frameSource'
import {
  adjustmentOf,
  isNeutralAdjustment,
  withBandField,
  withoutBand,
  HSL_HUE_LIMIT,
  HSL_SCALE_MAX,
  HSL_UI_BANDS,
  type HslField,
} from './hsl'
import { WHEEL_MODES } from './wheel'

const props = defineProps<{ active: boolean }>()

const { frame, tick, error } = useFrameGrabber(toRef(props, 'active'))

const showScopes = ref(true)
const showCompare = ref(false)
const scopesGraded = ref(true)

const TONE_SLIDERS = [
  { key: 'temperature', label: 'Температура', min: -1, max: 1, step: 0.01, digits: 2 },
  { key: 'tint', label: 'Оттенок', min: -1, max: 1, step: 0.01, digits: 2 },
  { key: 'exposure', label: 'Экспозиция, стопы', min: -2, max: 2, step: 0.05, digits: 2 },
  { key: 'highlights', label: 'Света', min: -1, max: 1, step: 0.01, digits: 2 },
  { key: 'shadows', label: 'Тени', min: -1, max: 1, step: 0.01, digits: 2 },
] as const

type ToneKey = (typeof TONE_SLIDERS)[number]['key']

const band = ref<HslBand>('red')

const grade = computed<ScopeGrade>(() => ({
  temperature: colorAdvancedState.temperature,
  tint: colorAdvancedState.tint,
  exposure: colorAdvancedState.exposure,
  highlights: colorAdvancedState.highlights,
  shadows: colorAdvancedState.shadows,
  lift: colorAdvancedState.lift,
  gamma: colorAdvancedState.gamma,
  gain: colorAdvancedState.gain,
}))

let transactionOpen = false

function begin(key: string): void {
  if (transactionOpen) return
  transactionOpen = true
  beginEditTransaction(key)
}

function end(): void {
  if (!transactionOpen) return
  transactionOpen = false
  endEditTransaction()
}

function setTone(key: ToneKey, raw: string): void {
  const parsed = Number.parseFloat(raw)
  const slider = TONE_SLIDERS.find((entry) => entry.key === key)
  if (!slider) return
  // Zero is the neutral value for all five of these, so it is also the safe
  // landing spot for input the browser handed us as something unparseable.
  const value = Number.isFinite(parsed)
    ? Math.max(slider.min, Math.min(slider.max, parsed))
    : 0
  colorAdvancedState[key] = value
}

const current = computed(() => adjustmentOf(colorAdvancedState.hsl, band.value))

function bandTouched(target: HslBand): boolean {
  return !isNeutralAdjustment(adjustmentOf(colorAdvancedState.hsl, target))
}

function setHsl(field: HslField, raw: string): void {
  colorAdvancedState.hsl = withBandField(
    colorAdvancedState.hsl,
    band.value,
    field,
    Number.parseFloat(raw),
  )
}

function resetBand(): void {
  begin('hsl-reset')
  colorAdvancedState.hsl = withoutBand(colorAdvancedState.hsl, band.value)
  end()
}

function resetAll(): void {
  begin('color-advanced-reset')
  resetColorAdvanced()
  end()
}
</script>

<template>
  <div class="grade">
    <div class="field">
      <div class="grid2">
        <label v-for="slider in TONE_SLIDERS" :key="slider.key">
          {{ slider.label }}: {{ colorAdvancedState[slider.key].toFixed(slider.digits) }}
          <input
            type="range"
            :min="slider.min"
            :max="slider.max"
            :step="slider.step"
            :value="colorAdvancedState[slider.key]"
            @pointerdown="begin(`grade-${slider.key}`)"
            @pointerup="end"
            @pointercancel="end"
            @keydown="begin(`grade-${slider.key}`)"
            @keyup="end"
            @blur="end"
            @input="setTone(slider.key, ($event.target as HTMLInputElement).value)"
          />
        </label>
        <button type="button" class="btn ghost sm reset-color" @click="resetAll">
          Сбросить коррекцию
        </button>
      </div>
    </div>

    <div class="field">
      <label>Цветовые колёса</label>
      <div class="grade-wheels">
        <ColorWheel
          v-for="mode in WHEEL_MODES"
          :key="mode"
          :mode="mode"
          :model-value="colorAdvancedState[mode]"
          @update:model-value="colorAdvancedState[mode] = $event"
        />
      </div>
      <p class="hint">
        Тяни диск мышью или стрелками, кольцо вокруг него меняет яркость. Двойной клик сбрасывает
        колесо.
      </p>
    </div>

    <div class="field">
      <label>HSL по диапазонам</label>
      <div class="chips">
        <button
          v-for="entry in HSL_UI_BANDS"
          :key="entry.band"
          type="button"
          class="chip"
          :class="{ active: band === entry.band }"
          @click="band = entry.band"
        >
          {{ entry.label }}{{ bandTouched(entry.band) ? ' •' : '' }}
        </button>
      </div>
      <div class="grid2">
        <label>
          Оттенок: {{ current.hue.toFixed(0) }}°
          <input
            type="range"
            :min="-HSL_HUE_LIMIT"
            :max="HSL_HUE_LIMIT"
            step="1"
            :value="current.hue"
            @pointerdown="begin('hsl-hue')"
            @pointerup="end"
            @pointercancel="end"
            @input="setHsl('hue', ($event.target as HTMLInputElement).value)"
          />
        </label>
        <label>
          Насыщенность: {{ current.saturation.toFixed(2) }}
          <input
            type="range"
            min="0"
            :max="HSL_SCALE_MAX"
            step="0.01"
            :value="current.saturation"
            @pointerdown="begin('hsl-saturation')"
            @pointerup="end"
            @pointercancel="end"
            @input="setHsl('saturation', ($event.target as HTMLInputElement).value)"
          />
        </label>
        <label>
          Яркость: {{ current.luminance.toFixed(2) }}
          <input
            type="range"
            min="0"
            :max="HSL_SCALE_MAX"
            step="0.01"
            :value="current.luminance"
            @pointerdown="begin('hsl-luminance')"
            @pointerup="end"
            @pointercancel="end"
            @input="setHsl('luminance', ($event.target as HTMLInputElement).value)"
          />
        </label>
        <button type="button" class="btn ghost sm reset-color" @click="resetBand">
          Сбросить диапазон
        </button>
      </div>
      <p class="hint">
        Оранжевого диапазона нет: FFmpeg не умеет выделять его отдельно, поэтому такой ползунок
        не предлагается. Значения выше 2 экспорт всё равно ограничивает.
      </p>
    </div>

    <div class="field">
      <label class="toggle"><input type="checkbox" v-model="showScopes" /> Осциллограммы</label>
      <template v-if="showScopes">
        <label class="toggle">
          <input type="checkbox" v-model="scopesGraded" /> Показывать с коррекцией
        </label>
        <ScopesView :frame="frame" :grade="grade" :graded="scopesGraded" />
        <p v-if="error" class="error" role="alert">{{ error }}</p>
        <p class="hint">
          Замер идёт по кадру плеера примерно пять раз в секунду и останавливается, когда панель
          свёрнута или вкладка неактивна.
        </p>
      </template>
    </div>

    <div class="field">
      <label class="toggle">
        <input type="checkbox" v-model="showCompare" /> Сравнение до и после
      </label>
      <CompareWipe v-if="showCompare" :tick="tick" :grade="grade" />
    </div>
  </div>
</template>

<style scoped>
.grade-wheels {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
  gap: 12px;
}
</style>
