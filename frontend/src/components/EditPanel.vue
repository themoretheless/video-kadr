<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  state,
  doExport,
  parseTime,
  setTrimStartFromPlayer,
  setTrimEndFromPlayer,
} from '../store'
import TrimSlider from './TrimSlider.vue'

const duration = computed(() => state.video?.duration ?? 0)
const selected = computed(() => Math.max(0, state.edit.trimEnd - state.edit.trimStart))

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
</script>

<template>
  <div class="card edit">
    <h2>Редактирование</h2>

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
            <label>X <input type="number" min="0" v-model.number="state.edit.crop.x" /></label>
            <label>Y <input type="number" min="0" v-model.number="state.edit.crop.y" /></label>
            <label>Ширина <input type="number" min="2" v-model.number="state.edit.crop.w" /></label>
            <label>Высота <input type="number" min="2" v-model.number="state.edit.crop.h" /></label>
          </div>
        </template>
      </div>
    </section>

    <!-- Audio -->
    <section class="group">
      <div class="group-title">Звук</div>
      <div class="field inline">
        <label class="toggle"><input type="checkbox" v-model="state.edit.mute" /> Без звука</label>
      </div>
    </section>

    <button class="btn primary big" :disabled="state.exporting" @click="doExport">
      {{ state.exporting ? 'Обработка…' : 'Экспортировать' }}
    </button>
  </div>
</template>
