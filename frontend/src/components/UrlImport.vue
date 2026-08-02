<script setup lang="ts">
import { ref } from 'vue'
import { state, doImport, doUpload, cancelImport, clientOnlyMode } from '../store'
import ProgressBar from './ProgressBar.vue'

const picker = ref<HTMLInputElement | null>(null)
const dragover = ref(false)

function onPick(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0]
  if (file) void doUpload(file)
  ;(e.target as HTMLInputElement).value = ''
}

function onDrop(e: DragEvent) {
  dragover.value = false
  const file = e.dataTransfer?.files?.[0]
  if (file) void doUpload(file)
}
</script>

<template>
  <div
    class="card import"
    :class="{ dragover }"
    @dragover.prevent="dragover = true"
    @dragleave.prevent="dragover = false"
    @drop.prevent="onDrop"
  >
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
      <span class="hint">диапазон импорта (мм:сс). Пусто = всё видео, для длинных роликов укажи отрезок</span>
    </div>
    <p v-if="clientOnlyMode" class="hint link-mode-hint">
      Импорт по ссылке сохранён для будущей полноценной версии. Сейчас выбери локальный файл — обработка выполнится прямо в браузере.
    </p>

    <div class="import-or"><span>или</span></div>

    <button type="button" class="dropzone" :disabled="state.importing" @click="picker?.click()">
      <input ref="picker" type="file" accept="video/*" class="hidden-file" @change="onPick" />
      <span class="dropzone-icon">📁</span>
      <span>Перетащи видеофайл сюда или нажми, чтобы выбрать — загрузки на сервер не будет</span>
    </button>

    <ProgressBar
      v-if="state.importing"
      class="import-progress"
      :progress="state.importProgress"
      :stage="state.importStage"
      :cancellable="state.importStage !== 'uploading'"
      @cancel="cancelImport"
    />
    <p v-if="state.importError" class="error">Ошибка: {{ state.importError }}</p>
  </div>
</template>
