<script lang="ts">
  import { onMount } from 'svelte'
  import EditPanel from '$lib/components/EditPanel.svelte'
  import MediaLibrary from '$lib/components/MediaLibrary.svelte'
  import ResultPanel from '$lib/components/ResultPanel.svelte'
  import Toasts from '$lib/components/Toasts.svelte'
  import UrlImport from '$lib/components/UrlImport.svelte'
  import VideoPreview from '$lib/components/VideoPreview.svelte'
  import {
    initTheme,
    loadCapabilities,
    loadLibrary,
    loadPresets,
    redo,
    seekRelative,
    setTrimEndFromPlayer,
    setTrimStartFromPlayer,
    state,
    togglePlay,
    toggleTheme,
    ui,
    undo,
  } from '$lib/state/store.svelte.js'

  function isTyping(target: EventTarget | null): boolean {
    const element = target as HTMLElement | null
    if (!element) return false
    return element.tagName === 'INPUT' || element.tagName === 'TEXTAREA' || element.isContentEditable
  }

  function onKey(event: KeyboardEvent): void {
    if (!state.video || isTyping(event.target)) return
    if ((event.metaKey || event.ctrlKey) && (event.key === 'z' || event.key === 'Z')) {
      event.preventDefault()
      if (event.shiftKey) redo()
      else undo()
      return
    }
    if ((event.metaKey || event.ctrlKey) && (event.key === 'y' || event.key === 'Y')) {
      event.preventDefault()
      redo()
      return
    }
    const fps = state.video.fps || 30
    switch (event.key) {
      case ' ':
        event.preventDefault()
        togglePlay()
        break
      case 'i':
      case 'I':
        event.preventDefault()
        setTrimStartFromPlayer()
        break
      case 'o':
      case 'O':
        event.preventDefault()
        setTrimEndFromPlayer()
        break
      case 'ArrowLeft':
        event.preventDefault()
        seekRelative(event.shiftKey ? -5 : -1)
        break
      case 'ArrowRight':
        event.preventDefault()
        seekRelative(event.shiftKey ? 5 : 1)
        break
      case ',':
        event.preventDefault()
        seekRelative(-1 / fps)
        break
      case '.':
        event.preventDefault()
        seekRelative(1 / fps)
        break
    }
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
          title={ui.theme === 'dark' ? 'Переключить на светлую тему' : 'Переключить на тёмную тему'}
          onclick={toggleTheme}
        >
          {ui.theme === 'dark' ? '☀️ Светлая' : '🌙 Тёмная'}
        </button>
      </div>
    </div>
    <p class="sub">Вставь ссылку на видео (например, VK Видео), обрежь и скачай результат.</p>
  </header>

  <UrlImport />
  <MediaLibrary />

  {#if state.video}
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
    <span class="kbd-hint">горячие клавиши: Space, I, O, ←/→, , .</span>
  </footer>

  <Toasts />
</div>
