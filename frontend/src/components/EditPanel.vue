<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  state,
  history,
  presets,
  parseTime,
  setTrimStartFromPlayer,
  setTrimEndFromPlayer,
  normalizeCrop,
  undo,
  redo,
  savePreset,
  applyPreset,
  deletePreset,
  resetColor,
  beginEditTransaction,
  endEditTransaction,
} from '../store'
import type { Preset } from '../store'
import TrimSlider from './TrimSlider.vue'
import CurvesEditor from './edit/CurvesEditor.vue'
import ExportControls from './edit/ExportControls.vue'
import LutControl from './edit/LutControl.vue'
// Feature panels. Each one owns its own store module and collapsible section;
// this file is the only place they are registered.
import TimelinePanel from './edit/TimelinePanel.vue'
import TextOverlayPanel from './edit/TextOverlayPanel.vue'
import AudioMixPanel from './edit/AudioMixPanel.vue'
import MotionPanel from './edit/MotionPanel.vue'
import SpatialPanel from './edit/SpatialPanel.vue'
import ColorGradePanel from './edit/ColorGradePanel.vue'

const canUndo = computed(() => history.past.length > 0)
const canRedo = computed(() => history.future.length > 0)

const curvesCapability = computed(() =>
  state.capabilities?.filters?.find((option) =>
    ['curves', 'color-curves', 'custom-curves'].includes(option.id.toLowerCase()),
  ),
)
const curvesUnavailableReason = computed(() => {
  if (!state.capabilities) return ''
  const option = curvesCapability.value
  if (!option) return 'Нужен обновлённый сервер с поддержкой кривых'
  return option.available ? '' : option.reason || 'Кривые недоступны в текущей сборке сервера'
})

const presetName = ref('')
function onSavePreset() {
  savePreset(presetName.value)
  presetName.value = ''
}
function onApplyPreset(p: Preset) {
  void applyPreset(p)
}

const duration = computed(() => state.video?.duration ?? 0)
const selected = computed(() => Math.max(0, state.edit.trimEnd - state.edit.trimStart))

// Initialise the cut range to the middle third when the feature is enabled.
watch(
  () => state.edit.cutEnabled,
  (on) => {
    if (!on) return
    const a = state.edit.trimStart
    const b = state.edit.trimEnd
    const c = state.edit.cut
    if (!(c.end > c.start) || c.start < a || c.end > b) {
      const span = b - a
      state.edit.cut = { start: a + span / 3, end: a + (2 * span) / 3 }
    }
  },
)

const keptDuration = computed(() => {
  const e = state.edit
  const cs = Math.max(e.trimStart, Math.min(e.cut.start, e.trimEnd))
  const ce = Math.max(e.trimStart, Math.min(e.cut.end, e.trimEnd))
  return Math.max(0, selected.value - Math.max(0, ce - cs))
})

function fmt(t: number): string {
  if (!isFinite(t)) return '0:00.0'
  const m = Math.floor(t / 60)
  const rem = t - m * 60
  let s = Math.floor(rem)
  let d = Math.round((rem - s) * 10)
  if (d === 10) {
    d = 0
    s += 1
  }
  return `${m}:${String(s).padStart(2, '0')}.${d}`
}

// --- editable time inputs synced to the slider ---
const startStr = ref('')
const endStr = ref('')
const startInvalid = ref(false)
const endInvalid = ref(false)
let editingStart = false
let editingEnd = false

function onStartFocus() {
  editingStart = true
  startInvalid.value = false
}

function onEndFocus() {
  editingEnd = true
  endInvalid.value = false
}

watch(
  () => state.edit.trimStart,
  (v) => {
    if (!editingStart) startStr.value = fmt(v)
  },
  { immediate: true },
)
watch(
  () => state.edit.trimEnd,
  (v) => {
    if (!editingEnd) endStr.value = fmt(v)
  },
  { immediate: true },
)

function commitStart() {
  editingStart = false
  const v = parseTime(startStr.value)
  if (v === null) {
    startInvalid.value = true
    startStr.value = fmt(state.edit.trimStart)
    return
  }
  startInvalid.value = false
  state.edit.trimStart = Math.max(0, Math.min(v, state.edit.trimEnd - 0.1))
  startStr.value = fmt(state.edit.trimStart)
}

function commitEnd() {
  editingEnd = false
  const v = parseTime(endStr.value)
  if (v === null) {
    endInvalid.value = true
    endStr.value = fmt(state.edit.trimEnd)
    return
  }
  endInvalid.value = false
  state.edit.trimEnd = Math.min(duration.value, Math.max(v, state.edit.trimStart + 0.1))
  endStr.value = fmt(state.edit.trimEnd)
}

const speeds = [0.5, 0.75, 1, 1.25, 1.5, 2]

const widthPresets = [
  { label: '1080p', w: 1920 },
  { label: '720p', w: 1280 },
  { label: '480p', w: 854 },
  { label: '360p', w: 640 },
]

function setWidth(w: number) {
  state.edit.scaleEnabled = true
  state.edit.scale = { w, h: -2 }
}

const aspects = [
  { label: '9:16', rw: 9, rh: 16 },
  { label: '1:1', rw: 1, rh: 1 },
  { label: '4:5', rw: 4, rh: 5 },
  { label: '4:3', rw: 4, rh: 3 },
  { label: '16:9', rw: 16, rh: 9 },
]

function setAspect(rw: number, rh: number) {
  const v = state.video
  if (!v) return
  state.edit.cropEnabled = true
  const r = rw / rh
  let w = v.width
  let h = Math.round(w / r)
  if (h > v.height) {
    h = v.height
    w = Math.round(h * r)
  }
  w -= w % 2
  h -= h % 2
  state.edit.crop = {
    x: Math.floor((v.width - w) / 2),
    y: Math.floor((v.height - h) / 2),
    w,
    h,
  }
}

function aspectActive(rw: number, rh: number): boolean {
  const c = state.edit.crop
  if (!c.h) return false
  return Math.abs(c.w / c.h - rw / rh) < 0.02
}

function resetCrop() {
  const v = state.video
  if (!v) return
  state.edit.crop = { x: 0, y: 0, w: v.width, h: v.height }
}

const rotations = [0, 90, 180, 270]

const filters = [
  { v: '', label: 'Нет' },
  { v: 'grayscale', label: 'Ч/Б' },
  { v: 'sepia', label: 'Сепия' },
  { v: 'warm', label: 'Тёплый' },
  { v: 'cold', label: 'Холодный' },
  { v: 'teal-orange', label: 'Teal-Orange' },
  { v: 'faded', label: 'Выцветший' },
  { v: 'noir', label: 'Нуар' },
  { v: 'vintage', label: 'Винтаж' },
]

function filterCapability(id: string) {
  if (!id) return undefined
  return state.capabilities?.filters.find((option) => option.id === id)
}

function filterUnavailableReason(id: string): string | undefined {
  const option = filterCapability(id)
  return option && !option.available ? option.reason || 'Недоступно в текущей сборке' : undefined
}

function selectFilter(id: string): void {
  if (filterUnavailableReason(id)) return
  state.edit.filter = id
}

const fpsPresets = [
  { v: null as number | null, label: 'ориг.' },
  { v: 60, label: '60' },
  { v: 30, label: '30' },
  { v: 24, label: '24' },
  { v: 15, label: '15' },
]

const censorColors = [
  { v: 'black', label: 'Чёрный' },
  { v: 'white', label: 'Белый' },
  { v: 'gray', label: 'Серый' },
]

const padAspects = [
  { v: '', label: 'Нет' },
  { v: '9:16', label: '9:16' },
  { v: '1:1', label: '1:1' },
  { v: '4:5', label: '4:5' },
  { v: '16:9', label: '16:9' },
]

// Seed the censor box to a centred rectangle when first enabled.
watch(
  () => state.edit.censorEnabled,
  (on) => {
    if (!on) return
    const v = state.video
    if (!v) return
    const c = state.edit.censor
    if (!(c.w > 1 && c.h > 1)) {
      state.edit.censor = {
        x: Math.round(v.width * 0.3),
        y: Math.round(v.height * 0.3),
        w: Math.round(v.width * 0.4),
        h: Math.round(v.height * 0.25),
      }
    }
  },
)

function applyPlatform(name: string) {
  if (!state.video) return
  state.edit.format = 'mp4'
  state.edit.codec = 'h264'
  state.edit.fps = null
  if (name === 'shorts' || name === 'reels') {
    // Vertical 9:16, 1080 wide, 30 fps.
    setAspect(9, 16)
    state.edit.scaleEnabled = true
    state.edit.scale = { w: 1080, h: -2 }
    state.edit.fps = 30
  } else {
    // telegram / youtube: keep frame, just cap width.
    state.edit.cropEnabled = false
    state.edit.scaleEnabled = true
    state.edit.scale = { w: name === 'youtube' ? 1920 : 1280, h: -2 }
  }
}
</script>

<template>
  <div class="card edit">
    <div class="edit-head">
      <h2>Редактирование</h2>
      <div class="history-btns">
        <button
          class="btn ghost sm"
          :disabled="!canUndo"
          title="Отменить (Cmd/Ctrl+Z)"
          @click="undo"
        >
          ↶ Отменить
        </button>
        <button
          class="btn ghost sm"
          :disabled="!canRedo"
          title="Повторить (Cmd/Ctrl+Shift+Z)"
          @click="redo"
        >
          ↷ Повторить
        </button>
      </div>
    </div>

    <!-- Presets -->
    <section class="group">
      <div class="group-title">Пресеты эффектов</div>
      <div class="field">
        <div class="preset-row">
          <input
            class="time-input preset-name"
            v-model="presetName"
            placeholder="Имя пресета"
            @keyup.enter="onSavePreset"
          />
          <button class="btn ghost sm" :disabled="!presetName.trim()" @click="onSavePreset">
            Сохранить
          </button>
        </div>
        <div v-if="presets.list.length" class="chips preset-chips">
          <span v-for="p in presets.list" :key="p.name" class="preset-chip">
            <button class="chip" :title="`Применить «${p.name}»`" @click="onApplyPreset(p)">
              {{ p.name }}
            </button>
            <button class="preset-del" :title="`Удалить «${p.name}»`" @click="deletePreset(p.name)">
              ×
            </button>
          </span>
        </div>
        <p v-else class="hint">
          Сохрани текущий образ (цвет, скорость и звук) и применяй к другим клипам.
        </p>
      </div>
    </section>

    <!-- Timing -->
    <section class="group">
      <div class="group-title">Время</div>

      <div class="field">
        <div class="trim-head">
          <span class="trim-label">Обрезка</span>
          <span class="trim-dur">выбрано {{ fmt(selected) }}</span>
        </div>
        <TrimSlider
          :min="0"
          :max="duration"
          :start="state.edit.trimStart"
          :end="state.edit.trimEnd"
          @update:start="state.edit.trimStart = $event"
          @update:end="state.edit.trimEnd = $event"
        />
        <div class="trim-times">
          <label class="tt">
            <span>Начало</span>
            <input
              class="time-input"
              :class="{ invalid: startInvalid }"
              v-model="startStr"
              @focus="onStartFocus"
              @blur="commitStart"
              @keyup.enter="commitStart"
            />
          </label>
          <div class="pos-btns">
            <button class="btn ghost sm" title="Начало от позиции плеера" @click="setTrimStartFromPlayer">
              ⟦ от позиции
            </button>
            <button class="btn ghost sm" title="Конец от позиции плеера" @click="setTrimEndFromPlayer">
              до позиции ⟧
            </button>
          </div>
          <label class="tt right">
            <span>Конец</span>
            <input
              class="time-input"
              :class="{ invalid: endInvalid }"
              v-model="endStr"
              @focus="onEndFocus"
              @blur="commitEnd"
              @keyup.enter="commitEnd"
            />
          </label>
        </div>
      </div>

      <div class="field">
        <label class="toggle">
          <input type="checkbox" v-model="state.edit.cutEnabled" /> Вырезать кусок из середины
        </label>
        <template v-if="state.edit.cutEnabled">
          <TrimSlider
            :min="state.edit.trimStart"
            :max="state.edit.trimEnd"
            :start="state.edit.cut.start"
            :end="state.edit.cut.end"
            @update:start="state.edit.cut.start = $event"
            @update:end="state.edit.cut.end = $event"
          />
          <p class="hint">
            Удаляем {{ fmt(Math.max(0, state.edit.cut.end - state.edit.cut.start)) }}, останется
            {{ fmt(keptDuration) }}. Доступно для MP4/WebM.
          </p>
        </template>
      </div>

      <div class="field">
        <label>Скорость</label>
        <div class="chips">
          <button
            v-for="s in speeds"
            :key="s"
            class="chip"
            :class="{ active: state.edit.speed === s }"
            @click="state.edit.speed = s"
          >
            {{ s }}×
          </button>
        </div>
      </div>

      <div class="field inline">
        <label class="toggle"><input type="checkbox" v-model="state.edit.reverse" /> Реверс</label>
        <span v-if="state.edit.reverse" class="hint">короткие отрезки: реверс грузит весь клип в память</span>
      </div>

      <div class="field">
        <div class="grid2">
          <label>Появление: {{ state.edit.fadeIn.toFixed(1) }} c
            <input type="range" min="0" max="5" step="0.1" v-model.number="state.edit.fadeIn" />
          </label>
          <label>Затухание: {{ state.edit.fadeOut.toFixed(1) }} c
            <input type="range" min="0" max="5" step="0.1" v-model.number="state.edit.fadeOut" />
          </label>
        </div>
      </div>
    </section>

    <!-- Frame -->
    <section class="group">
      <div class="group-title">Кадр</div>

      <div class="field">
        <label class="toggle">
          <input type="checkbox" v-model="state.edit.scaleEnabled" /> Изменить размер
        </label>
        <div v-if="state.edit.scaleEnabled" class="chips">
          <button
            v-for="p in widthPresets"
            :key="p.w"
            class="chip"
            :class="{ active: state.edit.scale.w === p.w }"
            @click="setWidth(p.w)"
          >
            {{ p.label }}
          </button>
        </div>
        <p v-if="state.edit.scaleEnabled" class="hint">
          Ширина {{ state.edit.scale.w }}px, высота пропорционально.
        </p>
      </div>

      <div class="field">
        <label class="toggle">
          <input type="checkbox" v-model="state.edit.cropEnabled" /> Кадрировать
        </label>
        <template v-if="state.edit.cropEnabled">
          <div class="chips">
            <button
              v-for="a in aspects"
              :key="a.label"
              class="chip"
              :class="{ active: aspectActive(a.rw, a.rh) }"
              @click="setAspect(a.rw, a.rh)"
            >
              {{ a.label }}
            </button>
            <button class="chip" @click="resetCrop">сброс</button>
          </div>
          <div class="grid2">
            <label>X <input type="number" min="0" v-model.number="state.edit.crop.x" @blur="normalizeCrop" /></label>
            <label>Y <input type="number" min="0" v-model.number="state.edit.crop.y" @blur="normalizeCrop" /></label>
            <label>Ширина <input type="number" min="2" v-model.number="state.edit.crop.w" @blur="normalizeCrop" /></label>
            <label>Высота <input type="number" min="2" v-model.number="state.edit.crop.h" @blur="normalizeCrop" /></label>
          </div>
        </template>
      </div>

      <div class="field">
        <label>Поворот</label>
        <div class="chips">
          <button
            v-for="r in rotations"
            :key="r"
            class="chip"
            :class="{ active: state.edit.rotate === r }"
            @click="state.edit.rotate = r"
          >
            {{ r }}°
          </button>
        </div>
      </div>

      <div class="field inline">
        <label class="toggle"><input type="checkbox" v-model="state.edit.flipH" /> Отразить ↔</label>
        <label class="toggle"><input type="checkbox" v-model="state.edit.flipV" /> Отразить ↕</label>
      </div>

      <div class="field">
        <label>Частота кадров</label>
        <div class="chips">
          <button
            v-for="f in fpsPresets"
            :key="String(f.v)"
            class="chip"
            :class="{ active: state.edit.fps === f.v }"
            @click="state.edit.fps = f.v"
          >
            {{ f.label }}
          </button>
        </div>
      </div>

      <div class="field">
        <label class="toggle">
          <input type="checkbox" v-model="state.edit.censorEnabled" /> Замазать область
        </label>
        <template v-if="state.edit.censorEnabled">
          <div class="chips">
            <button
              v-for="c in censorColors"
              :key="c.v"
              class="chip"
              :class="{ active: state.edit.censorColor === c.v }"
              @click="state.edit.censorColor = c.v"
            >
              {{ c.label }}
            </button>
          </div>
          <p class="hint">Выдели красный прямоугольник прямо на видео.</p>
        </template>
      </div>

      <div class="field">
        <label>Поля под пропорции (letterbox)</label>
        <div class="chips">
          <button
            v-for="p in padAspects"
            :key="p.v"
            class="chip"
            :class="{ active: state.edit.pad === p.v }"
            @click="state.edit.pad = p.v"
          >
            {{ p.label }}
          </button>
        </div>
      </div>
    </section>

    <!-- Colour -->
    <section class="group">
      <div class="group-title">Цвет</div>
      <div class="field">
        <div class="chips">
          <button
            v-for="f in filters"
            :key="f.v"
            class="chip"
            :class="{ active: state.edit.filter === f.v }"
            :aria-disabled="filterCapability(f.v)?.available === false"
            :aria-label="filterUnavailableReason(f.v) ? `${f.label}. ${filterUnavailableReason(f.v)}` : f.label"
            :title="filterUnavailableReason(f.v)"
            @click="selectFilter(f.v)"
          >
            {{ f.label }}
          </button>
        </div>
      </div>
      <div class="field">
        <div class="grid2">
          <label>Яркость: {{ state.edit.brightness.toFixed(2) }}
            <input type="range" min="-1" max="1" step="0.05" v-model.number="state.edit.brightness" />
          </label>
          <label>Контраст: {{ state.edit.contrast.toFixed(2) }}
            <input type="range" min="0" max="2" step="0.05" v-model.number="state.edit.contrast" />
          </label>
          <label>Насыщенность: {{ state.edit.saturation.toFixed(2) }}
            <input type="range" min="0" max="3" step="0.05" v-model.number="state.edit.saturation" />
          </label>
          <button class="btn ghost sm reset-color" @click="resetColor">Сбросить цвет</button>
        </div>
      </div>
      <div class="advanced-color-stack">
        <LutControl />
        <template v-if="!curvesUnavailableReason">
          <CurvesEditor
            v-model="state.edit.curves"
            @interaction-start="beginEditTransaction('curves')"
            @interaction-end="endEditTransaction"
          />
          <p class="advanced-color-note">
            Кривые не отображаются в предпросмотре; точный результат виден после экспорта.
          </p>
        </template>
        <div v-else class="color-tool is-unavailable curves-unavailable" role="status">
          <strong>Кривые недоступны</strong>
          <span>{{ curvesUnavailableReason }}</span>
        </div>
      </div>
      <div class="field inline">
        <label class="toggle"><input type="checkbox" v-model="state.edit.vignette" /> Виньетка</label>
        <label class="toggle"><input type="checkbox" v-model="state.edit.denoise" /> Шумодав</label>
      </div>
      <div class="field">
        <div class="grid2">
          <label>Резкость: {{ state.edit.sharpen.toFixed(1) }}
            <input type="range" min="0" max="3" step="0.1" v-model.number="state.edit.sharpen" />
          </label>
          <label>Зерно: {{ Math.round(state.edit.grain) }}
            <input type="range" min="0" max="60" step="1" v-model.number="state.edit.grain" />
          </label>
        </div>
      </div>
    </section>

    <!-- Audio -->
    <section class="group">
      <div class="group-title">Звук</div>
      <div class="field inline">
        <label class="toggle"><input type="checkbox" v-model="state.edit.mute" /> Без звука</label>
      </div>
      <template v-if="!state.edit.mute">
        <div class="field">
          <label>Громкость: {{ Math.round(state.edit.volume * 100) }}%</label>
          <input type="range" min="0" max="2" step="0.05" v-model.number="state.edit.volume" />
        </div>
        <div class="field inline">
          <label class="toggle">
            <input type="checkbox" v-model="state.edit.normalizeAudio" /> Нормализация громкости
          </label>
          <label class="toggle">
            <input type="checkbox" v-model="state.edit.highpass" /> Убрать гул (highpass)
          </label>
        </div>
      </template>
    </section>

    <!-- Feature panels -->
    <section class="group">
      <div class="group-title">Расширенные возможности</div>
      <TimelinePanel />
      <TextOverlayPanel />
      <AudioMixPanel />
      <MotionPanel />
      <SpatialPanel />
      <ColorGradePanel />
    </section>

    <ExportControls @platform="applyPlatform" />
  </div>
</template>
