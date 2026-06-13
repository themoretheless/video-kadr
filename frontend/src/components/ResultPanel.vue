<script setup lang="ts">
import { computed } from 'vue'
import { state, cancelExport } from '../store'
import ProgressBar from './ProgressBar.vue'

// Build a friendly download filename from the source title, falling back to a
// generic name. Strips characters that are awkward in filenames.
const downloadName = computed(() => {
  const title = state.video?.title?.trim()
  if (!title) return 'edited.mp4'
  const safe = title
    .replace(/[\\/:*?"<>|]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim()
    .slice(0, 60)
  return `${safe || 'edited'}.mp4`
})
</script>

<template>
  <div class="card result">
    <h2>Результат</h2>

    <ProgressBar
      v-if="state.exporting"
      :progress="state.exportProgress"
      :stage="state.exportStage"
      cancellable
      @cancel="cancelExport"
    />
    <p v-if="state.exportError" class="error">Ошибка: {{ state.exportError }}</p>

    <template v-if="state.result && !state.exporting">
      <video class="player" :src="state.result.url" controls playsinline></video>
      <a class="btn primary big" :href="state.result.url" :download="downloadName">
        Скачать результат
      </a>
    </template>
  </div>
</template>
