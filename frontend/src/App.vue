<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import {
  state,
  ui,
  togglePlay,
  seekRelative,
  setTrimStartFromPlayer,
  setTrimEndFromPlayer,
  loadLibrary,
  loadPresets,
  initTheme,
  toggleTheme,
  undo,
  redo,
} from './store'
import UrlImport from './components/UrlImport.vue'
import VideoPreview from './components/VideoPreview.vue'
import EditPanel from './components/EditPanel.vue'
import ResultPanel from './components/ResultPanel.vue'
import MediaLibrary from './components/MediaLibrary.vue'
import Toasts from './components/Toasts.vue'

function isTyping(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null
  if (!el) return false
  const tag = el.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || el.isContentEditable
}

function onKey(e: KeyboardEvent) {
  if (!state.video || isTyping(e.target)) return
  // Undo / redo (Cmd/Ctrl+Z, Cmd/Ctrl+Shift+Z, Ctrl+Y).
  if ((e.metaKey || e.ctrlKey) && (e.key === 'z' || e.key === 'Z')) {
    e.preventDefault()
    if (e.shiftKey) redo()
    else undo()
    return
  }
  if ((e.metaKey || e.ctrlKey) && (e.key === 'y' || e.key === 'Y')) {
    e.preventDefault()
    redo()
    return
  }
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

onMounted(() => {
  window.addEventListener('keydown', onKey)
  initTheme()
  loadPresets()
  void loadLibrary()
})
onUnmounted(() => window.removeEventListener('keydown', onKey))
</script>

<template>
  <div class="app">
    <header class="topbar">
      <div class="topbar-row">
        <h1>🎬 Видеоредактор</h1>
        <button
          class="btn ghost sm theme-toggle"
          :title="ui.theme === 'dark' ? 'Переключить на светлую тему' : 'Переключить на тёмную тему'"
          @click="toggleTheme"
        >
          {{ ui.theme === 'dark' ? '☀️ Светлая' : '🌙 Тёмная' }}
        </button>
      </div>
      <p class="sub">Вставь ссылку на видео (например, VK Видео), обрежь и скачай результат.</p>
    </header>

    <UrlImport />

    <MediaLibrary />

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
