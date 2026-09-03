<script lang="ts">
  import {
    compositionProjectArchiveUrl,
    importCompositionProjectArchive,
    MAX_COMPOSITION_PROJECT_ARCHIVE_BYTES,
    subscribeCompositionProjectChanges,
  } from '$lib/api.js'
  import type { ReviewThreadDto } from '$lib/api.js'
  import CapturePanel from '$lib/capture/CapturePanel.svelte'
  import type { CaptureRuntime } from '$lib/capture/types.js'
  import VoiceoverPanel from '$lib/audio/VoiceoverPanel.svelte'
  import type { VoiceoverRuntime } from '$lib/audio/types.js'
  import { voiceoverFileValidationError } from '$lib/composition/voiceover.js'
  import type { MediaInfo } from '$lib/types.js'
  import {
    compositionState,
    addMediaInfoToComposition,
    addVoiceoverMediaInfoToComposition,
    deleteCompositionProject,
    loadCompositionProjects,
    newComposition,
    openCompositionProject,
    refreshOpenCompositionProject,
    refreshCompositionProjects,
    saveCompositionProject,
    setCompositionProjectName,
    syncCompositionLibrary,
  } from '$lib/state/composition.svelte.js'
  import { doUpload, loadLibrary, state as legacyState } from '$lib/state/store.svelte.js'
  import { authState, logout } from '$lib/state/auth.svelte.js'
  import { spacesState } from '$lib/state/spaces.svelte.js'
  import AuthPanel from '$lib/components/review/AuthPanel.svelte'
  import SpacePanel from '$lib/components/collaboration/SpacePanel.svelte'
  import CompositionLocalTools from './CompositionLocalTools.svelte'
  import CompositionInspector from './CompositionInspector.svelte'
  import CompositionPreview from './CompositionPreview.svelte'
  import CompositionReviewPanel from './CompositionReviewPanel.svelte'
  import MultiTrackTimeline from './MultiTrackTimeline.svelte'
  import YouTubePublishPanel from './YouTubePublishPanel.svelte'

  interface Props {
    captureRuntime?: CaptureRuntime
    captureCountdownSeconds?: number
    captureUploader?: (file: File, openLegacy?: boolean) => Promise<MediaInfo | null>
    voiceoverRuntime?: VoiceoverRuntime
    voiceoverCountdownSeconds?: number
    voiceoverUploader?: (file: File, openLegacy?: boolean) => Promise<MediaInfo | null>
  }

  let {
    captureRuntime,
    captureCountdownSeconds = 3,
    captureUploader = doUpload,
    voiceoverRuntime,
    voiceoverCountdownSeconds = 3,
    voiceoverUploader = doUpload,
  }: Props = $props()

  const missingSourceCount = $derived(
    Object.keys(compositionState.document.sources).filter((id) => !compositionState.media[id]).length,
  )
  let captureOpen = $state(false)
  let captureUploading = $state(false)
  let captureError = $state('')
  let voiceoverOpen = $state(false)
  let voiceoverUploading = $state(false)
  let voiceoverMessage = $state('')
  let voiceoverError = $state('')
  let pendingVoiceover = $state.raw<File | null>(null)
  let archivePicker = $state<HTMLInputElement>()
  let archiveBusy = $state(false)
  let archiveMessage = $state('')
  let archiveError = $state('')
  let reviewThreads = $state<ReviewThreadDto[]>([])

  $effect(() => {
    const projectId = compositionState.projectId
    const token = authState.token
    if (!projectId || !token) return
    const timer = window.setInterval(() => void refreshOpenCompositionProject(), 5_000)
    return () => window.clearInterval(timer)
  })

  $effect(() => {
    const token = authState.token
    if (!token) return
    const timer = window.setInterval(() => void refreshCompositionProjects(), 10_000)
    return () => window.clearInterval(timer)
  })

  $effect(() => {
    const token = authState.token
    if (!token || typeof EventSource === 'undefined') return
    return subscribeCompositionProjectChanges(() => {
      void refreshCompositionProjects()
      void refreshOpenCompositionProject()
    })
  })
  let loadedProjectsForToken = $state<string | null>(null)

  $effect(() => syncCompositionLibrary(legacyState.library, legacyState.librarySnapshotReady))
  $effect(() => {
    const token = authState.token
    if (token && loadedProjectsForToken !== token) {
      loadedProjectsForToken = token
      void loadCompositionProjects()
    } else if (!token) {
      loadedProjectsForToken = null
      compositionState.projects = []
    }
  })

  function openSelected(event: Event): void {
    const id = (event.currentTarget as HTMLSelectElement).value
    if (id) void openCompositionProject(id)
  }

  async function addCapture(file: File): Promise<void> {
    if (captureUploading) return
    captureUploading = true
    captureError = ''
    try {
      const media = await captureUploader(file, false)
      if (!media) throw new Error(legacyState.importError || 'Не удалось загрузить запись')
      addMediaInfoToComposition(media)
      captureOpen = false
    } catch (error) {
      captureError = error instanceof Error ? error.message : String(error)
    } finally {
      captureUploading = false
    }
  }

  async function addVoiceover(file: File): Promise<void> {
    if (voiceoverUploading) return
    if (pendingVoiceover && pendingVoiceover !== file) {
      voiceoverError = 'Сначала сохраните предыдущую голосовую запись'
      return
    }
    pendingVoiceover = file
    await uploadPendingVoiceover()
  }

  async function uploadPendingVoiceover(): Promise<void> {
    const file = pendingVoiceover
    if (!file || voiceoverUploading) return
    voiceoverError = ''
    voiceoverMessage = ''
    const validationError = voiceoverFileValidationError(file)
    if (validationError) {
      voiceoverError = validationError
      return
    }
    voiceoverUploading = true
    try {
      const media = await voiceoverUploader(file, false)
      if (!media) throw new Error(legacyState.importError || 'Не удалось загрузить голосовую запись')
      addVoiceoverMediaInfoToComposition(media)
      if (pendingVoiceover === file) pendingVoiceover = null
      voiceoverMessage = 'Голосовая запись добавлена на аудиодорожку'
      voiceoverOpen = false
    } catch (error) {
      voiceoverError = error instanceof Error ? error.message : String(error)
    } finally {
      voiceoverUploading = false
    }
  }

  function downloadPendingVoiceover(): void {
    const file = pendingVoiceover
    if (!file) return
    let url = ''
    try {
      url = URL.createObjectURL(file)
      downloadUrl(url, file.name)
    } catch (error) {
      voiceoverError = error instanceof Error ? error.message : String(error)
    } finally {
      if (url) window.setTimeout(() => URL.revokeObjectURL(url), 0)
    }
  }

  function downloadProjectArchive(): void {
    const projectId = compositionState.projectId
    if (!projectId || !authState.token || archiveBusy) return
    archiveError = ''
    archiveMessage = ''
    try {
      downloadUrl(compositionProjectArchiveUrl(projectId))
      archiveMessage = 'Загрузка переносимого архива началась'
    } catch (error) {
      archiveError = error instanceof Error ? error.message : String(error)
    }
  }

  async function importProjectArchive(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (!file || !authState.token || archiveBusy) return
    archiveBusy = true
    archiveError = ''
    archiveMessage = ''
    try {
      if (file.size > MAX_COMPOSITION_PROJECT_ARCHIVE_BYTES) {
        throw new Error('Архив проекта превышает лимит 2 ГиБ')
      }
      const imported = await importCompositionProjectArchive(file, authState.token, spacesState.selectedId || undefined)
      const [libraryLoaded] = await Promise.all([loadLibrary(), loadCompositionProjects()])
      await openCompositionProject(imported.project.id)
      syncCompositionLibrary(legacyState.library, libraryLoaded)
      archiveMessage = `Импортирован проект «${imported.project.name}» · ${Object.keys(imported.sourceMapping).length} медиа`
    } catch (error) {
      archiveError = error instanceof Error ? error.message : String(error)
    } finally {
      archiveBusy = false
    }
  }

  function downloadUrl(url: string, filename = ''): void {
    const anchor = document.createElement('a')
    anchor.href = url
    anchor.download = filename
    anchor.hidden = true
    document.body.append(anchor)
    anchor.click()
    anchor.remove()
  }
</script>

<main class="composition-workspace">
  {#if !authState.token}
    <AuthPanel />
  {:else}
    <SpacePanel onselectionchange={() => void loadLibrary()} />
  {/if}
  <section class="composition-projectbar card" aria-label="Проект композиции">
    <input
      class="composition-project-name"
      aria-label="Название композиции"
      value={compositionState.projectName}
      oninput={(event) => setCompositionProjectName(event.currentTarget.value)}
    />
    <button class="btn ghost sm" disabled={compositionState.save.busy} onclick={() => newComposition()}>Новый</button>
    <button class="btn primary sm" disabled={compositionState.save.busy || !authState.token} onclick={() => void saveCompositionProject()}>
      {compositionState.save.busy ? 'Сохраняю…' : compositionState.projectId ? 'Сохранить' : 'Создать проект'}
    </button>
    <select aria-label="Сохранённые композиции" disabled={compositionState.save.busy || !authState.token} value={compositionState.projectId ?? ''} onchange={openSelected}>
      <option value="">Открыть проект…</option>
      {#each compositionState.projects as project (project.id)}
        <option value={project.id}>{project.name}</option>
      {/each}
    </select>
    {#if compositionState.projectId}
      <button class="btn ghost sm" disabled={archiveBusy || !authState.token} onclick={downloadProjectArchive}>Экспорт .veproj</button>
      <button class="btn ghost sm danger" disabled={compositionState.save.busy || !authState.token} onclick={() => void deleteCompositionProject(compositionState.projectId!)}>Удалить проект</button>
    {/if}
    <button class="btn ghost sm" disabled={archiveBusy || !authState.token} onclick={() => archivePicker?.click()}>Импорт .veproj</button>
    {#if authState.user}
      <span class="composition-project-user">{authState.user.username}</span>
      <button class="btn ghost sm" onclick={() => void logout()}>Выйти</button>
    {/if}
    <input
      bind:this={archivePicker}
      class="composition-archive-picker"
      type="file"
      accept=".veproj,application/vnd.video-editor.project"
      onchange={(event) => void importProjectArchive(event)}
    />
  </section>

  {#if compositionState.save.error}<p class="composition-workspace-error error" role="alert">{compositionState.save.error}</p>{/if}
  {#if archiveMessage}<p class="composition-archive-message" role="status">{archiveMessage}</p>{/if}
  {#if archiveError}<p class="composition-workspace-error error" role="alert">{archiveError}</p>{/if}
  {#if missingSourceCount}
    <p class="composition-source-warning" role="status">
      Не найдено медиафайлов: {missingSourceCount}. Дорожки и клипы сохранены; восстановите файлы в медиатеке.
    </p>
  {/if}

  <details class="composition-capture card" bind:open={captureOpen}>
    <summary>Записать экран, окно или вкладку</summary>
    {#if captureOpen}
      <CapturePanel
        title="Запись для композиции"
        runtime={captureRuntime}
        countdownSeconds={captureCountdownSeconds}
        oncapture={addCapture}
        oncaptureerror={(message) => { captureError = message }}
      />
      {#if captureUploading}<p class="composition-capture-status" role="status">Загружаю запись в медиатеку и добавляю на timeline…</p>{/if}
      {#if captureError}<p class="error" role="alert">{captureError}</p>{/if}
    {/if}
  </details>

  <details class="composition-voiceover card" bind:open={voiceoverOpen}>
    <summary>Записать голос на аудиодорожку</summary>
    {#if voiceoverOpen}
      <VoiceoverPanel
        title="Voiceover для композиции"
        runtime={voiceoverRuntime}
        countdownSeconds={voiceoverCountdownSeconds}
        busy={voiceoverUploading}
        blocked={Boolean(pendingVoiceover)}
        oncapture={addVoiceover}
        onvoiceovererror={(message) => { voiceoverError = message }}
      />
      {#if voiceoverUploading}<p class="composition-voiceover-status" role="status">Загружаю голосовую запись…</p>{/if}
      {#if voiceoverError}<p class="error" role="alert">{voiceoverError}</p>{/if}
    {/if}
  </details>
  {#if pendingVoiceover}
    <section class="composition-voiceover-pending card" aria-label="Несохранённая голосовая запись">
      <p role="status">
        <strong>{pendingVoiceover.name}</strong> хранится локально до успешной загрузки.
      </p>
      <button class="btn primary sm" type="button" disabled={voiceoverUploading || Boolean(voiceoverFileValidationError(pendingVoiceover))} onclick={() => void uploadPendingVoiceover()}>
        Повторить загрузку
      </button>
      <button class="btn ghost sm" type="button" onclick={downloadPendingVoiceover}>Скачать запись</button>
    </section>
  {/if}
  {#if voiceoverMessage}<p class="composition-voiceover-message" role="status">{voiceoverMessage}</p>{/if}
  {#if voiceoverError && !voiceoverOpen}<p class="composition-workspace-error error" role="alert">{voiceoverError}</p>{/if}

  <div class="composition-workspace-grid">
    <CompositionPreview />
    <CompositionInspector />
  </div>
  <MultiTrackTimeline {reviewThreads} />
  <CompositionReviewPanel onthreadschange={(threads) => { reviewThreads = threads }} />
  <CompositionLocalTools />
  <YouTubePublishPanel />
</main>

<style>
  .composition-archive-picker { display: none; }
</style>
