<script setup lang="ts">
import { state, doImport } from '../store'
</script>

<template>
  <div class="card import">
    <div class="row">
      <input
        v-model="state.url"
        class="url-input"
        type="url"
        placeholder="https://vkvideo.ru/video-220018529_456248395"
        :disabled="state.importing"
        @keyup.enter="doImport"
      />
      <button
        class="btn primary"
        :disabled="state.importing || !state.url.trim()"
        @click="doImport"
      >
        {{ state.importing ? 'Загрузка…' : 'Импорт' }}
      </button>
    </div>
    <div class="range-row">
      <label>с <input v-model="state.importStart" class="time-input" placeholder="0:30" :disabled="state.importing" /></label>
      <label>по <input v-model="state.importEnd" class="time-input" placeholder="2:00" :disabled="state.importing" /></label>
      <span class="hint">диапазон импорта (мм:сс). Пусто = всё видео — для длинных роликов укажи отрезок</span>
    </div>
    <p v-if="state.importStatus" class="status">{{ state.importStatus }}</p>
    <p v-if="state.importError" class="error">Ошибка: {{ state.importError }}</p>
  </div>
</template>
