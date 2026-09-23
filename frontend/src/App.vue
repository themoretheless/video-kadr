<script setup lang="ts">
import { computed, defineAsyncComponent, onMounted, onUnmounted, ref } from 'vue'
import {
  state,
  ui,
  togglePlay,
  seekRelative,
  setTrimStartFromPlayer,
  setTrimEndFromPlayer,
  loadLibrary,
  loadCapabilities,
  loadPresets,
  initTheme,
  toggleTheme,
  undo,
  redo,
  clientOnlyMode,
  timelineState,
  undoTimeline,
  redoTimeline,
  splitSelectedClipAtPlayhead,
  rippleDeleteSelectedClip,
  removeSelectedClip,
  closeSelectedTimelineGap,
  addTimelineMarkerAtPlayhead,
  jumpTimelineMarker,
  removeSelectedTimelineMarker,
  flushProjectSave,
  projectRecovery,
} from './store'
import UrlImport from './components/UrlImport.vue'
import VideoPreview from './components/VideoPreview.vue'
import VideoScopesPanel from './components/VideoScopesPanel.vue'
import EditPanel from './components/EditPanel.vue'
import ResultPanel from './components/ResultPanel.vue'
import MediaLibrary from './components/MediaLibrary.vue'
import Toasts from './components/Toasts.vue'
import TimelineEditor from './components/TimelineEditor.vue'
import MulticamPanel from './components/MulticamPanel.vue'
import ProjectRecoveryDialog from './components/ProjectRecoveryDialog.vue'
import DerivedTaskCenter from './components/DerivedTaskCenter.vue'
import ExportQueuePanel from './components/ExportQueuePanel.vue'
import ProjectArchivePanel from './components/ProjectArchivePanel.vue'
import { initializeDerivedTasks, onDerivedVisibilityChange } from './derived-task-center'

const RecorderPanel = defineAsyncComponent(() => import('./components/RecorderPanel.vue'))
const DesignHub = defineAsyncComponent(() => import('./components/DesignHub.vue'))
const designHubOpen = ref(false)

const legacyInspectorAvailable = computed(() => {
  const document = timelineState.document
  if (!document || !timelineState.selectedClipId) return true
  const selected = document.sequences
    .flatMap((sequence) => sequence.tracks)
    .flatMap((track) => track.clips)
    .find((clip) => clip.id === timelineState.selectedClipId)
  return !selected || selected.effects.some((effect) => effect.kind === 'legacy_edit')
})

function isTyping(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null
  if (!el) return false
  const tag = el.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || el.isContentEditable
}

function onKey(e: KeyboardEvent) {
  if (!state.video || projectRecovery.candidate || projectRecovery.restoring || isTyping(e.target)) return
  // Undo / redo (Cmd/Ctrl+Z, Cmd/Ctrl+Shift+Z, Ctrl+Y).
  if ((e.metaKey || e.ctrlKey) && (e.key === 'z' || e.key === 'Z')) {
    e.preventDefault()
    const inTimeline = (e.target as HTMLElement | null)?.closest('.timeline-editor') !== null
    if (e.shiftKey) {
      if (inTimeline) redoTimeline()
      else redo()
    } else if (inTimeline) undoTimeline()
    else undo()
    return
  }
  if ((e.metaKey || e.ctrlKey) && (e.key === 'y' || e.key === 'Y')) {
    e.preventDefault()
    const inTimeline = (e.target as HTMLElement | null)?.closest('.timeline-editor') !== null
    if (inTimeline) redoTimeline()
    else redo()
    return
  }
  // Timeline blade and ripple delete (S, Delete/Backspace, Shift+Delete).
  if ((e.key === 's' || e.key === 'S') && !e.metaKey && !e.ctrlKey && !e.altKey) {
    e.preventDefault()
    splitSelectedClipAtPlayhead()
    return
  }
  if ((e.key === 'm' || e.key === 'M') && !e.metaKey && !e.ctrlKey && !e.altKey) {
    e.preventDefault()
    addTimelineMarkerAtPlayhead()
    return
  }
  if (e.key === '[' && !e.metaKey && !e.ctrlKey && !e.altKey) {
    e.preventDefault()
    jumpTimelineMarker(-1)
    return
  }
  if (e.key === ']' && !e.metaKey && !e.ctrlKey && !e.altKey) {
    e.preventDefault()
    jumpTimelineMarker(1)
    return
  }
  if (e.key === 'Delete' || e.key === 'Backspace') {
    const inTimeline = (e.target as HTMLElement | null)?.closest('.timeline-editor') !== null
    if (!inTimeline) return
    e.preventDefault()
    if (!timelineState.selectedClipId && timelineState.selectedMarkerId) {
      removeSelectedTimelineMarker()
      return
    }
    if (!timelineState.selectedClipId && timelineState.selectedGap) {
      closeSelectedTimelineGap()
      return
    }
    if (e.shiftKey || e.altKey) removeSelectedClip()
    else rippleDeleteSelectedClip()
    return
  }
  if (!legacyInspectorAvailable.value) return
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

function onPageHide() {
  void flushProjectSave()
}

onMounted(() => {
  window.addEventListener('keydown', onKey)
  window.addEventListener('pagehide', onPageHide)
  initTheme()
  loadPresets()
  void loadLibrary()
  void loadCapabilities()
  void initializeDerivedTasks()
  document.addEventListener('visibilitychange', onDerivedVisibilityChange)
})
onUnmounted(() => {
  window.removeEventListener('keydown', onKey)
  window.removeEventListener('pagehide', onPageHide)
  document.removeEventListener('visibilitychange', onDerivedVisibilityChange)
})
</script>

<template>
  <div class="app">
    <div :inert="projectRecovery.candidate || projectRecovery.restoring ? true : undefined">
    <header class="topbar">
      <div class="topbar-row">
        <h1>🎬 Video Kadr</h1>
        <div class="topbar-actions">
          <span
            v-if="state.backendStatus === 'offline'"
            class="backend-status offline"
            role="status"
          >
            Сервер недоступен
          </span>
          <span v-else-if="state.backendStatus === 'client'" class="backend-status" role="status">
            Обработка в браузере
          </span>
          <button
            class="btn ghost sm theme-toggle"
            :title="ui.theme === 'dark' ? 'Переключить на светлую тему' : 'Переключить на тёмную тему'"
            @click="toggleTheme"
          >
            {{ ui.theme === 'dark' ? '☀️ Светлая' : '🌙 Тёмная' }}
          </button>
        </div>
      </div>
      <p class="sub">
        {{ clientOnlyMode
          ? 'Выбери видео с устройства, отредактируй и экспортируй — файл никуда не загружается.'
          : 'Вставь ссылку на видео (например, VK Видео), обрежь и скачай результат.' }}
      </p>
    </header>

    <UrlImport />

    <RecorderPanel />

    <button type="button" class="btn ghost" :aria-expanded="designHubOpen" aria-controls="design-hub-lazy" @click="designHubOpen = !designHubOpen">{{ designHubOpen ? 'Скрыть шаблоны и Brand kit' : 'Открыть шаблоны и Brand kit' }}</button>
    <div v-if="designHubOpen" id="design-hub-lazy"><DesignHub :has-active-project="Boolean(state.video)" /></div>

    <ProjectArchivePanel />

    <MediaLibrary />
    <MulticamPanel v-if="state.video" />
    <DerivedTaskCenter />
    <ExportQueuePanel />

    <main v-if="state.video" class="editor">
      <section class="left">
        <div :class="{ 'preview-gated': !legacyInspectorAvailable }">
          <VideoPreview />
          <VideoScopesPanel v-if="legacyInspectorAvailable" />
        </div>
      </section>
      <section class="right">
        <p v-if="!legacyInspectorAvailable" class="timeline-error" role="status">
          Для secondary clip сейчас доступны перемещение и trim. Эффекты и preview будут подключены через render graph.
        </p>
        <EditPanel v-if="legacyInspectorAvailable" />
        <ResultPanel v-if="state.result || state.exporting || state.exportError" />
      </section>
    </main>

    <TimelineEditor v-if="state.video" />

    <footer class="foot">
      {{ clientOnlyMode ? 'Статическая версия · обработка через ffmpeg.wasm на этом устройстве' : 'Полная версия · скачивание через yt-dlp · обработка через ffmpeg' }} ·
      <span class="kbd-hint">горячие клавиши: Space, I, O, ←/→, , .</span>
    </footer>

    <Toasts />
    </div>
    <ProjectRecoveryDialog />
  </div>
</template>
