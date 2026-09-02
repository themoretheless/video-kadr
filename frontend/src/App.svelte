<script lang="ts">
  import { onMount } from 'svelte'
  import EditPanel from '$lib/components/EditPanel.svelte'
  import MediaLibrary from '$lib/components/MediaLibrary.svelte'
  import ShortcutSettings from '$lib/components/ShortcutSettings.svelte'
  import LazyCompositionWorkspace from '$lib/components/composition/LazyCompositionWorkspace.svelte'
  import ResultPanel from '$lib/components/ResultPanel.svelte'
  import Toasts from '$lib/components/Toasts.svelte'
  import UrlImport from '$lib/components/UrlImport.svelte'
  import VideoPreview from '$lib/components/VideoPreview.svelte'
  import { executeCompositionCommand } from '$lib/composition/commandRegistry.js'
  import type { ShortcutCommandId } from '$lib/shortcuts.js'
  import {
    editorMode,
    setEditorMode,
  } from '$lib/state/composition.svelte.js'
  import {
    openShortcutSettings,
    shortcutCommandForEvent,
    shortcutLabel,
    shortcutState,
  } from '$lib/state/shortcuts.svelte.js'
  import {
    deleteTimelineSegment,
    duplicateTimelineSegment,
    initTheme,
    loadCapabilities,
    loadLibrary,
    loadPresets,
    moveTimelineSegment,
    redo,
    seekRelative,
    setTrimEndFromPlayer,
    setTrimStartFromPlayer,
    splitTimelineSegment,
    state,
    togglePlay,
    toggleTheme,
    ui,
    undo,
  } from '$lib/state/store.svelte.js'

  function isTyping(target: EventTarget | null): boolean {
    const element = target instanceof Element ? target : null
    if (!element) return false
    const interactive = element.closest('input, textarea, select, button, a, [role="textbox"], [contenteditable]:not([contenteditable="false"])')
    return Boolean(interactive) || (element instanceof HTMLElement && element.isContentEditable)
  }

  function runLegacyCommand(command: ShortcutCommandId): boolean {
    if (!state.video) return false
    const sourceFps = state.video.fps
    const fps = typeof sourceFps === 'number' && Number.isFinite(sourceFps) && sourceFps > 0 ? sourceFps : 30
    switch (command) {
      case 'legacy.playPause': togglePlay(); return true
      case 'legacy.framePrevious': seekRelative(-1 / fps); return true
      case 'legacy.frameNext': seekRelative(1 / fps); return true
      case 'legacy.seekPrevious': seekRelative(-1); return true
      case 'legacy.seekNext': seekRelative(1); return true
      case 'legacy.seekPreviousLarge': seekRelative(-5); return true
      case 'legacy.seekNextLarge': seekRelative(5); return true
      case 'legacy.trimStart': setTrimStartFromPlayer(); return true
      case 'legacy.trimEnd': setTrimEndFromPlayer(); return true
      case 'legacy.undo': undo(); return true
      case 'legacy.redo': redo(); return true
      case 'legacy.timelineSplit': return runLegacyTimelineSplit()
      case 'legacy.timelineDuplicate': return runLegacyTimelineDuplicate()
      case 'legacy.timelineDelete': return runLegacyTimelineDelete()
      case 'legacy.timelineMovePrevious': return runLegacyTimelineMove(-1)
      case 'legacy.timelineMoveNext': return runLegacyTimelineMove(1)
      default: return false
    }
  }

  function selectedLegacyTimelineId(): string | null {
    if (!state.edit.timelineEnabled) return null
    const selected = state.timelineSelectedSegmentId
    return state.edit.timelineSegments.some((segment) => segment.id === selected)
      ? selected
      : state.edit.timelineSegments[0]?.id ?? null
  }

  function runLegacyTimelineSplit(): boolean {
    const selected = selectedLegacyTimelineId()
    if (!selected) return false
    const next = splitTimelineSegment(selected)
    if (next) state.timelineSelectedSegmentId = next
    return true
  }

  function runLegacyTimelineDuplicate(): boolean {
    const selected = selectedLegacyTimelineId()
    if (!selected) return false
    const next = duplicateTimelineSegment(selected)
    if (next) state.timelineSelectedSegmentId = next
    return true
  }

  function runLegacyTimelineDelete(): boolean {
    const selected = selectedLegacyTimelineId()
    if (!selected) return false
    const next = deleteTimelineSegment(selected)
    if (next) state.timelineSelectedSegmentId = next
    return true
  }

  function runLegacyTimelineMove(direction: -1 | 1): boolean {
    const selected = selectedLegacyTimelineId()
    return selected ? moveTimelineSegment(selected, direction) : false
  }

  function onKey(event: KeyboardEvent): void {
    if (event.defaultPrevented || shortcutState.capturingCommandId || isTyping(event.target)) return
    const command = shortcutCommandForEvent(editorMode.value, event)
    if (!command) return
    const handled = editorMode.value === 'composition'
      ? executeCompositionCommand(command as Extract<ShortcutCommandId, `composition.${string}`>)
      : runLegacyCommand(command)
    if (handled) event.preventDefault()
  }

  onMount(() => {
    initTheme()
    loadPresets()
    void loadLibrary()
    void loadCapabilities()
  })
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <header class="topbar">
    <div class="topbar-row">
      <h1>🎬 Видеоредактор</h1>
      <div class="topbar-actions">
        {#if state.backendStatus === 'offline'}
          <span class="backend-status offline" role="status">Сервер недоступен</span>
        {/if}
        <button
          class="btn ghost sm theme-toggle"
          type="button"
          title={ui.theme === 'dark' ? 'Переключить на светлую тему' : 'Переключить на тёмную тему'}
          onclick={toggleTheme}
        >
          {ui.theme === 'dark' ? '☀️ Светлая' : '🌙 Тёмная'}
        </button>
        <button
          class="btn ghost sm"
          type="button"
          aria-haspopup="dialog"
          aria-expanded={shortcutState.dialogOpen}
          onclick={openShortcutSettings}
        >⌨ Клавиши</button>
      </div>
    </div>
    <p class="sub">Вставь ссылку на видео (например, VK Видео), обрежь и скачай результат.</p>
    <div class="editor-mode-switch" role="group" aria-label="Режим редактора">
      <button class:active={editorMode.value === 'legacy'} class="btn ghost sm" type="button" aria-pressed={editorMode.value === 'legacy'} onclick={() => setEditorMode('legacy')}>Legacy</button>
      <button class:active={editorMode.value === 'composition'} class="btn ghost sm" type="button" aria-pressed={editorMode.value === 'composition'} onclick={() => setEditorMode('composition')}>Multitrack</button>
    </div>
  </header>

  <UrlImport />
  <MediaLibrary />

  {#if editorMode.value === 'composition'}
    <LazyCompositionWorkspace />
  {:else if state.video}
    <main class="editor">
      <section class="left"><VideoPreview /></section>
      <section class="right">
        <EditPanel />
        {#if state.result || state.exporting || state.exportError}<ResultPanel />{/if}
      </section>
    </main>
  {/if}

  <footer class="foot">
    Локальный MVP · скачивание через yt-dlp · обработка через ffmpeg ·
    <span class="kbd-hint">
      {#if editorMode.value === 'composition'}
        горячие клавиши: {shortcutLabel('composition.playPause')}, {shortcutLabel('composition.framePrevious')}/{shortcutLabel('composition.frameNext')}, {shortcutLabel('composition.split')}, {shortcutLabel('composition.delete')}
      {:else}
        горячие клавиши: {shortcutLabel('legacy.playPause')}, {shortcutLabel('legacy.trimStart')}, {shortcutLabel('legacy.trimEnd')}, {shortcutLabel('legacy.framePrevious')}/{shortcutLabel('legacy.frameNext')}
      {/if}
    </span>
  </footer>

  <ShortcutSettings />
  <Toasts />
</div>
