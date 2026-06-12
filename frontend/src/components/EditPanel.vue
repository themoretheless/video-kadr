<script setup lang="ts">
import { computed } from 'vue'
import { state, doExport } from '../store'

const duration = computed(() => state.video?.duration ?? 0)

function fmt(t: number): string {
  if (!isFinite(t)) return '0:00'
  const m = Math.floor(t / 60)
  const s = Math.floor(t % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

function onTrimStart(e: Event) {
  const v = parseFloat((e.target as HTMLInputElement).value)
  state.edit.trimStart = Math.min(v, state.edit.trimEnd - 0.1)
}

function onTrimEnd(e: Event) {
  const v = parseFloat((e.target as HTMLInputElement).value)
  state.edit.trimEnd = Math.max(v, state.edit.trimStart + 0.1)
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
</script>

<template>
  <div class="card edit">
    <h2>Редактирование</h2>

    <!-- Trim -->
    <div class="field">
      <label>Обрезка: {{ fmt(state.edit.trimStart) }} – {{ fmt(state.edit.trimEnd) }}</label>
      <input
        type="range"
        min="0"
        :max="duration"
        step="0.1"
        :value="state.edit.trimStart"
        @input="onTrimStart"
      />
      <input
        type="range"
        min="0"
        :max="duration"
        step="0.1"
        :value="state.edit.trimEnd"
        @input="onTrimEnd"
      />
    </div>

    <!-- Speed -->
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

    <!-- Mute -->
    <div class="field inline">
      <label><input type="checkbox" v-model="state.edit.mute" /> Без звука</label>
    </div>

    <!-- Scale -->
    <div class="field">
      <label><input type="checkbox" v-model="state.edit.scaleEnabled" /> Изменить размер</label>
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
        Ширина {{ state.edit.scale.w }}px, высота — пропорционально.
      </p>
    </div>

    <!-- Crop -->
    <div class="field">
      <label><input type="checkbox" v-model="state.edit.cropEnabled" /> Кадрировать</label>
      <div v-if="state.edit.cropEnabled" class="grid2">
        <label>X <input type="number" min="0" v-model.number="state.edit.crop.x" /></label>
        <label>Y <input type="number" min="0" v-model.number="state.edit.crop.y" /></label>
        <label>Ширина <input type="number" min="2" v-model.number="state.edit.crop.w" /></label>
        <label>Высота <input type="number" min="2" v-model.number="state.edit.crop.h" /></label>
      </div>
    </div>

    <button class="btn primary big" :disabled="state.exporting" @click="doExport">
      {{ state.exporting ? 'Обработка…' : 'Экспортировать' }}
    </button>
  </div>
</template>
