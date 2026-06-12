<script setup lang="ts">
import { state } from '../store'
</script>

<template>
  <div class="card result">
    <h2>Результат</h2>
    <p v-if="state.exportStatus" class="status">{{ state.exportStatus }}</p>
    <p v-if="state.exportError" class="error">Ошибка: {{ state.exportError }}</p>
    <template v-if="state.result">
      <video class="player" :src="state.result.url" controls playsinline></video>
      <a class="btn primary" :href="state.result.url" :download="state.result.filename">
        Скачать результат
      </a>
    </template>
  </div>
</template>
