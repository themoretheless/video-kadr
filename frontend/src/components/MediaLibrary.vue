<script setup lang="ts">
import { computed } from 'vue'
import {
  state,
  openFromLibrary,
  openSavedProject,
  deleteFromLibrary,
  addMediaToTimeline,
  relinkLibraryMedia,
  restoreExternalLibraryMedia,
  timelineState,
  relinkState,
  batchRelinkLibraryMedia,
} from '../store'
import type { MediaEntry } from '../types'

function label(e: MediaEntry): string {
  return e.title?.trim() || e.filename
}

function fmtDuration(t?: number | null): string {
  if (!t || !isFinite(t)) return ''
  const m = Math.floor(t / 60)
  const s = Math.floor(t % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

function fmtSize(bytes?: number | null): string {
  if (!bytes) return ''
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} ГБ`
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} МБ`
  return `${Math.max(1, Math.round(bytes / 1024))} КБ`
}

function meta(e: MediaEntry): string {
  const parts: string[] = []
  if (e.width && e.height) parts.push(`${e.width}×${e.height}`)
  const d = fmtDuration(e.duration)
  if (d) parts.push(d)
  const s = fmtSize(e.sizeBytes)
  if (s) parts.push(s)
  return parts.join(' · ')
}

function ext(e: MediaEntry): string {
  return e.filename.split('.').pop()?.toUpperCase() || ''
}

function onRelink(event: Event, entry: MediaEntry): void {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  if (file) void relinkLibraryMedia(entry, file)
  input.value = ''
}

const missingProjectEntries = computed(() => {
  const expected = new Map(timelineState.document?.media.map((media) => [media.assetRef ?? media.id, media.contentFingerprint]) ?? [])
  return state.library.filter((entry) => {
    const fingerprint = expected.get(entry.assetId ?? entry.id)
    return entry.kind === 'source' && expected.has(entry.assetId ?? entry.id) && (
      entry.availability === 'offline' || entry.availability === 'permission-required'
      || Boolean(fingerprint && entry.fingerprint !== fingerprint)
    )
  })
})

function onBatchRelink(event: Event): void {
  const input = event.target as HTMLInputElement
  const files = [...(input.files ?? [])]
  if (files.length) void batchRelinkLibraryMedia(missingProjectEntries.value, files)
  input.value = ''
}

type FilePickerWindow = Window & {
  showOpenFilePicker?: (options?: { multiple?: boolean }) => Promise<FileSystemFileHandle[]>
}
const hasExternalPicker = typeof window !== 'undefined' && Boolean((window as FilePickerWindow).showOpenFilePicker)

async function pickExternalFile(entry: MediaEntry): Promise<void> {
  const picker = (window as FilePickerWindow).showOpenFilePicker
  if (!picker) return
  try {
    const [handle] = await picker({ multiple: false })
    if (handle) await relinkLibraryMedia(entry, await handle.getFile(), handle)
  } catch (error) {
    if (!(error instanceof DOMException && error.name === 'AbortError')) throw error
  }
}
</script>

<template>
  <div v-if="state.library.length" class="card library">
    <h2>Медиатека</h2>
    <div v-if="state.projects?.length" class="project-choices" aria-label="Сохранённые проекты">
      <strong>Проекты</strong>
      <button
        v-for="project in state.projects"
        :key="project.id"
        class="btn ghost sm"
        :aria-label="`Открыть проект ${project.name}`"
        @click="openSavedProject(project)"
      >{{ project.name }}</button>
    </div>
    <div v-if="timelineState.document && missingProjectEntries.length" class="relink-summary" role="status" aria-live="polite">
      <strong>Недоступно исходников: {{ missingProjectEntries.length }}</strong>
      <span>{{ missingProjectEntries.map(label).join(', ') }}</span>
      <label class="btn primary sm" :class="{ disabled: relinkState.batchBusy }">
        {{ relinkState.batchBusy ? 'Проверяю файлы…' : 'Найти несколько файлов' }}
        <input class="hidden-file" type="file" multiple :disabled="relinkState.batchBusy" @change="onBatchRelink">
      </label>
      <span v-if="relinkState.batchSummary">{{ relinkState.batchSummary }}</span>
    </div>
    <ul class="lib-list">
      <li v-for="e in state.library" :key="e.id" class="lib-item">
        <span class="lib-badge" :class="e.kind">{{ e.kind === 'output' ? 'результат' : 'источник' }}</span>
        <div class="lib-info">
          <div class="lib-name" :title="label(e)">{{ label(e) }}</div>
          <div class="lib-meta">{{ ext(e) }}<template v-if="meta(e)"> · {{ meta(e) }}</template></div>
          <div v-if="e.availability === 'offline' || e.availability === 'permission-required'" class="lib-offline" role="status">
            {{ relinkState.busy[e.id] ? 'Проверяю файл…' : e.identityConflict
              ? 'Проекты ожидают разные версии этого исходника — откройте нужный проект перед relink'
              : e.availability === 'permission-required'
              ? 'Нужно снова разрешить доступ к исходному файлу'
              : 'Файл недоступен — выберите исходник повторно' }}
          </div>
        </div>
        <div class="lib-actions">
          <button v-if="e.kind === 'source' && !e.identityConflict" class="btn ghost sm" @click="openFromLibrary(e)">Открыть как проект</button>
          <button
            v-if="e.kind === 'source' && e.availability === 'permission-required'"
            class="btn ghost sm"
            :aria-label="`Разрешить доступ к ${label(e)}`"
            :disabled="relinkState.busy[e.id]"
            @click="restoreExternalLibraryMedia(e)"
          >Разрешить доступ</button>
          <button
            v-if="e.kind === 'source' && timelineState.document"
            class="btn ghost sm"
            :aria-label="`Добавить ${label(e)} в текущий проект`"
            :disabled="e.availability !== undefined && e.availability !== 'ready'"
            :title="e.availability !== undefined && e.availability !== 'ready' ? 'Сначала найдите исходный файл заново' : undefined"
            @click="addMediaToTimeline(e)"
          >Добавить</button>
          <label v-if="e.kind === 'source' && (e.availability === 'offline' || e.availability === 'permission-required')" class="btn ghost sm" :class="{ disabled: relinkState.busy[e.id] }">
            {{ e.availability === 'permission-required' ? 'Выбрать замену' : 'Найти файл' }}
            <input class="hidden-file" type="file" :aria-label="`Найти файл для ${label(e)}`" :disabled="relinkState.busy[e.id]" @change="onRelink($event, e)">
          </label>
          <button
            v-if="e.kind === 'source' && (e.availability === 'offline' || e.availability === 'permission-required') && hasExternalPicker"
            class="btn ghost sm"
            :aria-label="`Связать внешний файл для ${label(e)}`"
            :disabled="relinkState.busy[e.id]"
            @click="pickExternalFile(e)"
          >Выбрать постоянную замену</button>
          <a v-if="e.kind === 'output'" class="btn ghost sm" :href="e.url" :download="e.filename">Скачать</a>
          <button class="btn ghost sm danger" title="Удалить" @click="deleteFromLibrary(e.id)">✕</button>
        </div>
      </li>
    </ul>
  </div>
</template>
