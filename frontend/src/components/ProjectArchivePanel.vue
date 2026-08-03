<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import {
  clientOnlyMode,
  currentProjectId,
  flushProjectSave,
  loadLibrary,
  openSavedProject,
  state,
  timelineState,
} from '../store'
import {
  exportPortableProjectArchive,
  importPortableProjectArchive,
  type ProjectArchiveRuntimeProgress,
  type ProjectDto,
} from '../project-archive-service'
import { toast } from '../toasts'
import ProgressBar from './ProgressBar.vue'

type ArchiveSavePickerWindow = Window & {
  showSaveFilePicker?: (options?: {
    suggestedName?: string
    types?: Array<{ description?: string; accept: Record<string, string[]> }>
  }) => Promise<FileSystemFileHandle>
}

const MIME_TYPE = 'application/vnd.video-kadr.project'
const selectedProjectId = ref('')
const includeOriginalMedia = ref(true)
const exporting = ref(false)
const importing = ref(false)
const progress = ref<ProjectArchiveRuntimeProgress | null>(null)
const error = ref('')
const status = ref('')
const warnings = ref<string[]>([])
const importPicker = ref<HTMLInputElement | null>(null)
let controller: AbortController | null = null

const selectedProject = computed(() =>
  state.projects.find(project => project.id === selectedProjectId.value) ?? null,
)
const busy = computed(() => exporting.value || importing.value)
const canExport = computed(() => Boolean(selectedProject.value || state.video))
const progressPercent = computed(() => {
  const value = progress.value
  if (!value || value.totalBytes <= 0) return null
  return Math.max(0, Math.min(100, value.completedBytes / value.totalBytes * 100))
})

function projectUsesLut(project: ProjectDto): boolean {
  if (typeof project.edit.lutId === 'string' && project.edit.lutId.trim()) return true
  return Boolean(project.document?.sequences
    .flatMap(sequence => sequence.tracks)
    .flatMap(track => track.clips)
    .flatMap(clip => clip.effects)
    .some(effect => {
      const direct = effect.parameters.lutId
      const nested = effect.parameters.lut
      return typeof direct === 'string' && direct.trim()
        || Boolean(nested && typeof nested === 'object' && !Array.isArray(nested)
          && typeof (nested as Record<string, unknown>).id === 'string')
    }))
}

watch(
  () => [timelineState.revision, state.video?.id, ...state.projects.map(project => `${project.id}:${project.revision ?? 0}`)],
  () => {
    const active = currentProjectId()
    if (active && state.projects.some(project => project.id === active)) {
      selectedProjectId.value = active
      return
    }
    if (selectedProject.value) return
    const matchingVideo = state.video
      ? state.projects.find(project => project.videoId === (state.video?.assetId ?? state.video?.id))
      : null
    selectedProjectId.value = matchingVideo?.id ?? state.projects[0]?.id ?? ''
  },
  { immediate: true },
)

function archiveFilename(name: string): string {
  const base = name.trim().replace(/[^\p{L}\p{N}._-]+/gu, '-').replace(/^-+|-+$/g, '').slice(0, 80)
  return `${base || 'project'}.vkadr`
}

function isCancelled(value: unknown): boolean {
  return value instanceof DOMException && value.name === 'AbortError'
    || value instanceof Error && /отмен|cancelled/i.test(value.message)
}

async function chooseSaveHandle(name: string): Promise<FileSystemFileHandle | null | undefined> {
  const picker = (window as ArchiveSavePickerWindow).showSaveFilePicker
  if (!picker) return undefined
  try {
    return await picker({
      suggestedName: archiveFilename(name),
      types: [{ description: 'Архив проекта Video Kadr', accept: { [MIME_TYPE]: ['.vkadr'] } }],
    })
  } catch (cause) {
    if (isCancelled(cause)) return null
    throw cause
  }
}

async function saveArchive(blob: Blob, filename: string, handle?: FileSystemFileHandle): Promise<void> {
  if (handle) {
    const writable = await handle.createWritable()
    try {
      await writable.write(blob)
      await writable.close()
    } catch (cause) {
      await writable.abort(cause).catch(() => undefined)
      throw cause
    }
    return
  }
  const url = URL.createObjectURL(blob)
  const link = document.createElement('a')
  link.href = url
  link.download = filename
  link.hidden = true
  document.body.append(link)
  link.click()
  link.remove()
  window.setTimeout(() => URL.revokeObjectURL(url), 1_000)
}

async function flushBeforeArchiveOperation(): Promise<void> {
  await flushProjectSave()
  if (timelineState.error) {
    throw new Error(`Сначала устраните ошибку сохранения текущего проекта: ${timelineState.error}`)
  }
}

function resetFeedback(): void {
  error.value = ''
  status.value = ''
  warnings.value = []
  progress.value = null
}

async function exportArchive(): Promise<void> {
  if (busy.value || !canExport.value) return
  resetFeedback()
  const initialName = selectedProject.value?.name
    ?? timelineState.document?.name
    ?? state.video?.title
    ?? state.video?.filename
    ?? 'project'
  let handle: FileSystemFileHandle | null | undefined
  try {
    // The picker must be opened while the click still carries user activation.
    handle = await chooseSaveHandle(initialName)
    if (handle === null) return
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause)
    return
  }
  exporting.value = true
  controller = new AbortController()
  try {
    const requestedId = selectedProjectId.value
    await flushBeforeArchiveOperation()
    await loadLibrary()
    const active = currentProjectId()
    const targetId = requestedId || active || ''
    const project = state.projects.find(candidate => candidate.id === targetId)
      ?? (active ? state.projects.find(candidate => candidate.id === active) : undefined)
    if (!project) throw new Error('Сохранённый проект для архива не найден.')
    if (projectUsesLut(project)) {
      throw new Error('Проект использует LUT. Архив не создан, чтобы не потерять цветокоррекцию.')
    }
    const result = await exportPortableProjectArchive(project, {
      includeOriginalMedia: includeOriginalMedia.value,
      includeProxies: false,
    }, {
      signal: controller.signal,
      onProgress: value => { progress.value = value },
    })
    progress.value = {
      phase: 'done', completedBytes: result.blob.size, totalBytes: result.blob.size, message: 'Архив готов',
    }
    await saveArchive(result.blob, result.filename, handle ?? undefined)
    warnings.value = result.warnings
    status.value = `Архив «${result.filename}» сохранён.`
    selectedProjectId.value = project.id
    toast('success', 'Архив проекта сохранён')
  } catch (cause) {
    if (isCancelled(cause)) {
      status.value = 'Экспорт архива отменён.'
      toast('info', status.value)
    } else {
      error.value = cause instanceof Error ? cause.message : String(cause)
      toast('error', error.value)
    }
  } finally {
    exporting.value = false
    controller = null
  }
}

async function importArchive(file: File): Promise<void> {
  if (busy.value) return
  resetFeedback()
  importing.value = true
  controller = new AbortController()
  try {
    await flushBeforeArchiveOperation()
    progress.value = {
      phase: 'verifying', completedBytes: 0, totalBytes: file.size,
      message: clientOnlyMode ? 'Проверяю архив…' : 'Загружаю и проверяю архив на сервере…',
    }
    const result = await importPortableProjectArchive(file, {
      signal: controller.signal,
      onProgress: value => { progress.value = value },
    })
    await loadLibrary()
    const imported = state.projects.find(project => project.id === result.projectId) ?? result.project
    if (!imported) throw new Error('Импортированный проект не найден после обновления библиотеки.')
    selectedProjectId.value = imported.id
    openSavedProject(imported)
    warnings.value = [
      ...result.warnings,
      ...(result.missingMedia.length
        ? [`Нет оригиналов: ${result.missingMedia.join(', ')}. Выполните точный relink.`]
        : []),
    ]
    const mediaSummary = result.importedMedia || result.reusedMedia
      ? ` Оригиналы: ${result.importedMedia} добавлено, ${result.reusedMedia} уже было.`
      : ''
    progress.value = { phase: 'done', completedBytes: file.size, totalBytes: file.size, message: 'Проект импортирован' }
    status.value = `Проект «${imported.name}» импортирован и открыт.${mediaSummary}`
    toast('success', 'Архив проекта импортирован')
  } catch (cause) {
    if (isCancelled(cause)) {
      status.value = 'Импорт архива отменён.'
      toast('info', status.value)
    } else {
      error.value = cause instanceof Error ? cause.message : String(cause)
      toast('error', error.value)
    }
  } finally {
    importing.value = false
    controller = null
  }
}

function onImportPick(event: Event): void {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (file) void importArchive(file)
  input.value = ''
}

function cancel(): void {
  controller?.abort()
}

onUnmounted(cancel)
</script>

<template>
  <section class="card project-archive" aria-labelledby="project-archive-title">
    <div class="project-archive-heading">
      <div>
        <h2 id="project-archive-title">Архив проекта</h2>
        <p>Переносимый файл .vkadr для резервной копии или другого браузера.</p>
      </div>
      <span class="project-archive-badge">v1</span>
    </div>

    <div class="project-archive-controls">
      <label class="project-archive-project">
        <span>Проект</span>
        <select v-model="selectedProjectId" :disabled="busy || !state.projects.length">
          <option v-if="!state.projects.length" value="">Текущий проект сохранится перед экспортом</option>
          <option v-for="project in state.projects" :key="project.id" :value="project.id">
            {{ project.name }}
          </option>
        </select>
      </label>

      <div class="project-archive-options">
        <label class="toggle">
          <input v-model="includeOriginalMedia" type="checkbox" :disabled="busy">
          Включить оригиналы
        </label>
        <label class="toggle project-archive-disabled" title="Прокси безопасно пересоздаются после импорта">
          <input type="checkbox" disabled>
          Прокси — пересоздать после импорта
        </label>
      </div>
    </div>

    <p class="project-archive-note">
      Кривые сохраняются в проекте. Проект с LUT пока нельзя экспортировать: операция остановится до создания файла, чтобы не потерять цветокоррекцию.
    </p>

    <div class="project-archive-actions">
      <button type="button" class="btn primary" :disabled="busy || !canExport" @click="exportArchive">
        {{ exporting ? 'Создаю архив…' : 'Экспортировать .vkadr' }}
      </button>
      <button type="button" class="btn ghost" :disabled="busy" @click="importPicker?.click()">
        {{ importing ? 'Проверяю архив…' : 'Импортировать .vkadr' }}
      </button>
      <input
        ref="importPicker"
        class="hidden-file"
        type="file"
        accept=".vkadr,application/vnd.video-kadr.project"
        :disabled="busy"
        @change="onImportPick"
      >
    </div>

    <ProgressBar
      v-if="busy"
      class="project-archive-progress"
      :progress="progressPercent"
      :stage="progress?.message"
      cancellable
      @cancel="cancel"
    />
    <p v-if="status" class="project-archive-status" role="status">{{ status }}</p>
    <ul v-if="warnings.length" class="project-archive-warnings" role="status">
      <li v-for="warning in warnings" :key="warning">{{ warning }}</li>
    </ul>
    <p v-if="error" class="error" role="alert">Ошибка: {{ error }}</p>
  </section>
</template>
