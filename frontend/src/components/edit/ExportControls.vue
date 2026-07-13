<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  doExport,
  hasMeaningfulChanges,
  selectedExportUnavailableReason,
  state,
} from '../../store'

type Platform = 'telegram' | 'shorts' | 'reels' | 'youtube'

const emit = defineEmits<{ platform: [Platform] }>()
const showNoopWarning = ref(false)

const formats = [
  { value: 'mp4', label: 'MP4' },
  { value: 'webm', label: 'WebM' },
  { value: 'av1', label: 'AV1' },
  { value: 'prores', label: 'ProRes' },
  { value: 'gif', label: 'GIF' },
  { value: 'png', label: 'Кадр PNG' },
  { value: 'jpg', label: 'Кадр JPG' },
  { value: 'mp3', label: 'Аудио MP3' },
]

const qualityTiers = [
  { value: '', label: 'Авто' },
  { value: 'high', label: 'Высокое' },
  { value: 'medium', label: 'Среднее' },
  { value: 'compact', label: 'Компактное' },
]

const formatHint = computed(() => {
  switch (state.edit.format) {
    case 'gif':
      return 'GIF без звука, по умолчанию 12 fps. Лучше выбрать размер и короткий отрезок.'
    case 'png':
      return 'Один кадр на позиции начала обрезки, без звука.'
    case 'jpg':
      return 'Один кадр JPG на позиции начала обрезки, без звука.'
    case 'mp3':
      return 'Только звук, видеоэффекты игнорируются.'
    case 'webm':
      return 'VP9 + Opus: меньше размер, дольше кодируется.'
    case 'av1':
      return 'AV1 даёт компактный файл, но кодируется медленно.'
    case 'prores':
      return 'ProRes 422 HQ для монтажа: крупный MOV-файл со звуком PCM.'
    default:
      return ''
  }
})

const showQuality = computed(() => ['mp4', 'webm', 'av1'].includes(state.edit.format))
const exportUnavailable = computed(() => selectedExportUnavailableReason())

function formatCapability(id: string) {
  return state.capabilities?.formats.find((option) => option.id === id)
}

function codecCapability(id: string) {
  return state.capabilities?.codecs.find((option) => option.id === id)
}

function unavailableReason(kind: 'format' | 'codec', id: string): string | undefined {
  const option = kind === 'format' ? formatCapability(id) : codecCapability(id)
  return option && !option.available ? option.reason || 'Недоступно в текущей сборке' : undefined
}

function selectFormat(id: string): void {
  if (unavailableReason('format', id)) return
  state.edit.format = id
}

function selectCodec(id: string): void {
  if (unavailableReason('codec', id)) return
  state.edit.codec = id
}

function requestExport(): void {
  if (!hasMeaningfulChanges()) {
    showNoopWarning.value = true
    return
  }
  void doExport()
}

function exportUnchangedCopy(): void {
  showNoopWarning.value = false
  void doExport()
}

watch(
  () => state.edit,
  () => {
    showNoopWarning.value = false
  },
  { deep: true },
)
</script>

<template>
  <section class="group export-group">
    <div class="group-title">Экспорт</div>
    <div class="field">
      <label>Формат</label>
      <div class="chips" role="group" aria-label="Формат экспорта">
        <button
          v-for="format in formats"
          :key="format.value"
          type="button"
          class="chip"
          :class="{ active: state.edit.format === format.value }"
          :aria-pressed="state.edit.format === format.value"
          :aria-disabled="formatCapability(format.value)?.available === false"
          :aria-label="unavailableReason('format', format.value) ? `${format.label}. ${unavailableReason('format', format.value)}` : format.label"
          :title="unavailableReason('format', format.value)"
          @click="selectFormat(format.value)"
        >
          {{ format.label }}
        </button>
      </div>
    </div>

    <div v-if="state.edit.format === 'mp4'" class="field">
      <label>Кодек</label>
      <div class="chips" role="group" aria-label="Кодек экспорта">
        <button
          type="button"
          class="chip"
          :class="{ active: state.edit.codec === 'h264' }"
          :aria-pressed="state.edit.codec === 'h264'"
          :aria-disabled="codecCapability('h264')?.available === false"
          :aria-label="unavailableReason('codec', 'h264') ? `H.264. ${unavailableReason('codec', 'h264')}` : 'H.264'"
          :title="unavailableReason('codec', 'h264')"
          @click="selectCodec('h264')"
        >
          H.264
        </button>
        <button
          type="button"
          class="chip"
          :class="{ active: state.edit.codec === 'h265' }"
          :aria-pressed="state.edit.codec === 'h265'"
          :aria-disabled="codecCapability('h265')?.available === false"
          :aria-label="unavailableReason('codec', 'h265') ? `H.265. ${unavailableReason('codec', 'h265')}` : 'H.265'"
          :title="unavailableReason('codec', 'h265')"
          @click="selectCodec('h265')"
        >
          H.265
        </button>
      </div>
    </div>

    <div v-if="showQuality" class="field">
      <label>Качество</label>
      <div class="chips" role="group" aria-label="Качество экспорта">
        <button
          v-for="quality in qualityTiers"
          :key="quality.value"
          type="button"
          class="chip"
          :class="{ active: state.edit.qualityTier === quality.value }"
          :aria-pressed="state.edit.qualityTier === quality.value"
          @click="state.edit.qualityTier = quality.value"
        >
          {{ quality.label }}
        </button>
      </div>
    </div>

    <div class="field">
      <label>Под платформу</label>
      <div class="chips" role="group" aria-label="Платформа публикации">
        <button type="button" class="chip" @click="emit('platform', 'telegram')">Telegram</button>
        <button type="button" class="chip" @click="emit('platform', 'shorts')">Shorts</button>
        <button type="button" class="chip" @click="emit('platform', 'reels')">Reels</button>
        <button type="button" class="chip" @click="emit('platform', 'youtube')">YouTube</button>
      </div>
    </div>

    <p v-if="formatHint" class="hint">{{ formatHint }}</p>
  </section>

  <div v-if="showNoopWarning" class="export-warning" role="alert" aria-live="assertive">
    <div>
      <strong>Изменений пока нет</strong>
      <p>Экспорт создаст копию исходного файла с повторным кодированием.</p>
    </div>
    <div class="export-warning-actions">
      <button type="button" class="btn ghost sm" @click="showNoopWarning = false">
        Вернуться
      </button>
      <button type="button" class="btn primary sm" @click="exportUnchangedCopy">
        Создать копию
      </button>
    </div>
  </div>

  <button
    type="button"
    class="btn primary big export-submit"
    :disabled="state.exporting || Boolean(exportUnavailable)"
    :title="exportUnavailable || undefined"
    @click="requestExport"
  >
    {{ state.exporting ? 'Обработка…' : 'Экспортировать' }}
  </button>
</template>
