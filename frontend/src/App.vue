<script setup lang="ts">
import { state } from './store'
import UrlImport from './components/UrlImport.vue'
import VideoPreview from './components/VideoPreview.vue'
import EditPanel from './components/EditPanel.vue'
import ResultPanel from './components/ResultPanel.vue'
</script>

<template>
  <div class="app">
    <header class="topbar">
      <h1>🎬 Видеоредактор</h1>
      <p class="sub">Вставь ссылку на видео (например, VK Видео), обрежь и скачай результат.</p>
    </header>

    <UrlImport />

    <main v-if="state.video" class="editor">
      <section class="left">
        <VideoPreview />
      </section>
      <section class="right">
        <EditPanel />
        <ResultPanel v-if="state.result || state.exporting || state.exportError" />
      </section>
    </main>

    <footer class="foot">
      Локальный MVP · скачивание через yt-dlp · обработка через ffmpeg
    </footer>
  </div>
</template>
