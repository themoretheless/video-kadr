<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import {
  state,
  togglePlay,
  seekRelative,
  setTrimStartFromPlayer,
  setTrimEndFromPlayer,
} from './store'
import UrlImport from './components/UrlImport.vue'
import VideoPreview from './components/VideoPreview.vue'
import EditPanel from './components/EditPanel.vue'
import ResultPanel from './components/ResultPanel.vue'
import Toasts from './components/Toasts.vue'

function isTyping(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null
  if (!el) return false
  const tag = el.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || el.isContentEditable
}

function onKey(e: KeyboardEvent) {
  if (!state.video || isTyping(e.target)) return
  const fps = state.video.fps || 30
  switch (e.key) {
    case ' ':
      e.preventDefault()
      togglePlay()
      break
    case 'i':
    case 'I':
      e.preventDefault()
      setTrimStartFromPlayer()
      break
    case 'o':
    case 'O':
      e.preventDefault()
      setTrimEndFromPlayer()
      break
    case 'ArrowLeft':
      e.preventDefault()
      seekRelative(e.shiftKey ? -5 : -1)
      break
    case 'ArrowRight':
      e.preventDefault()
      seekRelative(e.shiftKey ? 5 : 1)
      break
    case ',':
      e.preventDefault()
      seekRelative(-1 / fps)
      break
    case '.':
      e.preventDefault()
      seekRelative(1 / fps)
      break
  }
}

onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => window.removeEventListener('keydown', onKey))
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
      Локальный MVP · скачивание через yt-dlp · обработка через ffmpeg ·
      <span class="kbd-hint">горячие клавиши: Space, I, O, ←/→, , .</span>
    </footer>

    <Toasts />
  </div>
</template>
