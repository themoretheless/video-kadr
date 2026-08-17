<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  beginEditTransaction,
  doExport,
  endEditTransaction,
  hasMeaningfulChanges,
  selectedExportUnavailableReason,
  state,
} from '../../store'
import ProgressBar from '../ProgressBar.vue'
import { applyFraming, batchState, clearBatch, runBatchExport } from '../spatial/batchExport'
import {
  ASPECT_PRESETS,
  framingForAspect,
  isAspectActive,
  PLATFORM_PRESETS,
  platformPatch,
  type AspectPreset,
  type FitMode,
  type PlatformId,
  type PlatformPreset,
} from '../spatial/exportPresets'

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

// --- aspect ratio and platform presets ---
// `crop` fills the target ratio and loses the edges, `pad` keeps the whole
// frame and adds bars. The two are mutually exclusive, so switching presets
// never leaves a stale crop behind a new letterbox.

const fitMode = ref<FitMode>('crop')
const fitModes: { value: FitMode; label: string }[] = [
  { value: 'crop', label: 'Заполнить' },
  { value: 'pad', label: 'Вписать в поля' },
]

const aspectPresets = ASPECT_PRESETS
const platformPresets = PLATFORM_PRESETS

function applyAspect(preset: AspectPreset): void {
  const video = state.video
  if (!video) return
  const framing = framingForAspect(video, preset, fitMode.value)
  beginEditTransaction('aspect-preset')
  state.edit.cropEnabled = framing.cropEnabled
  state.edit.crop = { ...framing.crop }
  state.edit.pad = framing.pad
  endEditTransaction()
}

function aspectActive(preset: AspectPreset): boolean {
  return isAspectActive(state.edit, preset, fitMode.value)
}

function setFitMode(mode: FitMode): void {
  fitMode.value = mode
}

function applyPlatform(preset: PlatformPreset): void {
  const video = state.video
  if (!video) return
  beginEditTransaction('platform-preset')
  applyFraming(platformPatch(video, preset))
  endEditTransaction()
}

// --- batch export ---
// One edit, several presets. Submission and polling stay in the store; this
// panel only picks the presets and shows the queue.

const batchSelection = ref<PlatformId[]>([])

function toggleBatch(id: PlatformId): void {
  const index = batchSelection.value.indexOf(id)
  if (index >= 0) batchSelection.value.splice(index, 1)
  else batchSelection.value.push(id)
}

function startBatch(): void {
  if (!batchSelection.value.length) return
  void runBatchExport([...batchSelection.value])
}

const batchDisabled = computed(
  () =>
    !state.video ||
    state.exporting ||
    batchState.running ||
    !batchSelection.value.length ||
    Boolean(exportUnavailable.value),
)

const batchStatusLabels: Record<string, string> = {
  pending: 'в очереди',
  running: 'идёт',
  done: 'готово',
  error: 'ошибка',
  skipped: 'пропущено',
}
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
      <label>Пропорции кадра</label>
      <div class="chips" role="group" aria-label="Режим подгонки">
        <button
          v-for="mode in fitModes"
          :key="mode.value"
          type="button"
          class="chip"
          :class="{ active: fitMode === mode.value }"
          :aria-pressed="fitMode === mode.value"
          @click="setFitMode(mode.value)"
        >
          {{ mode.label }}
        </button>
      </div>
      <div class="chips" role="group" aria-label="Пропорции кадра">
        <button
          v-for="preset in aspectPresets"
          :key="preset.id"
          type="button"
          class="chip"
          :class="{ active: aspectActive(preset) }"
          :aria-pressed="aspectActive(preset)"
          :disabled="!state.video"
          @click="applyAspect(preset)"
        >
          {{ preset.label }}
        </button>
      </div>
      <p class="hint">
        «Заполнить» кадрирует по центру, «Вписать в поля» добавляет чёрные поля и сохраняет весь
        кадр. 2.39:1 уходит на сервер как 98:41: там пропорции задаются целыми числами.
      </p>
    </div>

    <div class="field">
      <label>Под платформу</label>
      <div class="chips" role="group" aria-label="Платформа публикации">
        <button type="button" class="chip" @click="emit('platform', 'telegram')">Telegram</button>
        <button
          v-for="preset in platformPresets"
          :key="preset.id"
          type="button"
          class="chip"
          :disabled="!state.video"
          :title="preset.hint"
          @click="applyPlatform(preset)"
        >
          {{ preset.label }}
        </button>
      </div>
    </div>

    <div class="field">
      <label>Пакетный экспорт</label>
      <div class="chips" role="group" aria-label="Пресеты пакетного экспорта">
        <button
          v-for="preset in platformPresets"
          :key="preset.id"
          type="button"
          class="chip"
          :class="{ active: batchSelection.includes(preset.id) }"
          :aria-pressed="batchSelection.includes(preset.id)"
          :disabled="batchState.running"
          @click="toggleBatch(preset.id)"
        >
          {{ preset.label }}
        </button>
      </div>
      <div class="chips">
        <button type="button" class="btn ghost sm" :disabled="batchDisabled" @click="startBatch">
          {{ batchState.running ? 'Экспортирую…' : 'Экспортировать пакетом' }}
        </button>
        <button
          v-if="batchState.jobs.length && !batchState.running"
          type="button"
          class="btn ghost sm"
          @click="clearBatch"
        >
          Очистить список
        </button>
      </div>
      <p class="hint">
        Тот же монтаж уходит в каждый выбранный пресет по очереди. Пропорции и размер меняются
        только на время очереди и возвращаются обратно в конце.
      </p>
      <ul v-if="batchState.jobs.length" class="batch-queue">
        <li v-for="job in batchState.jobs" :key="job.id" class="batch-job">
          <div class="batch-job-head">
            <span class="batch-job-label">{{ job.label }}</span>
            <span class="batch-job-status" :class="`is-${job.status}`">
              {{ batchStatusLabels[job.status] }}
            </span>
          </div>
          <ProgressBar
            v-if="job.status === 'running'"
            :progress="job.progress"
            :stage="state.exportStage"
          />
          <a v-else-if="job.status === 'done'" class="batch-job-link" :href="job.url" download>
            {{ job.filename }}
          </a>
          <span v-else-if="job.error" class="batch-job-error">{{ job.error }}</span>
        </li>
      </ul>
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
    :disabled="state.exporting || batchState.running || Boolean(exportUnavailable)"
    :title="exportUnavailable || undefined"
    @click="requestExport"
  >
    {{ state.exporting ? 'Обработка…' : 'Экспортировать' }}
  </button>
</template>

<style scoped>
.batch-queue {
  list-style: none;
  margin: 8px 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.batch-job {
  padding: 8px 10px;
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
}

.batch-job-head {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  font-size: 13px;
}

.batch-job-label {
  font-weight: 600;
}

.batch-job-status {
  color: var(--muted);
}

.batch-job-status.is-done {
  color: var(--ok);
}

.batch-job-status.is-error {
  color: var(--danger);
}

.batch-job-status.is-skipped {
  color: var(--warn);
}

.batch-job-link {
  color: var(--accent);
  font-size: 13px;
  word-break: break-all;
}

.batch-job-error {
  color: var(--danger);
  font-size: 13px;
}
</style>
