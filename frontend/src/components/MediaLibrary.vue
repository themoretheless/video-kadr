<script setup lang="ts">
import { state, openFromLibrary, deleteFromLibrary } from '../store'
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
</script>

<template>
  <div v-if="state.library.length" class="card library">
    <h2>Медиатека</h2>
    <ul class="lib-list">
      <li v-for="e in state.library" :key="e.id" class="lib-item">
        <span class="lib-badge" :class="e.kind">{{ e.kind === 'output' ? 'результат' : 'источник' }}</span>
        <div class="lib-info">
          <div class="lib-name" :title="label(e)">{{ label(e) }}</div>
          <div class="lib-meta">{{ ext(e) }}<template v-if="meta(e)"> · {{ meta(e) }}</template></div>
        </div>
        <div class="lib-actions">
          <button v-if="e.kind === 'source'" class="btn ghost sm" @click="openFromLibrary(e)">Открыть</button>
          <a v-else class="btn ghost sm" :href="e.url" :download="e.filename">Скачать</a>
          <button class="btn ghost sm danger" title="Удалить" @click="deleteFromLibrary(e.id)">✕</button>
        </div>
      </li>
    </ul>
  </div>
</template>
