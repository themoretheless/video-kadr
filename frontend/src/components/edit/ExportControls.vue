<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  currentBrowserExportPlan,
  doStreamingExport,
  doExport,
  enqueueExportVariants,
  exportQueueState,
  hasMeaningfulChanges,
  selectedExportUnavailableReason,
  state,
  streamingOutputSupported,
} from '../../store'

type Platform = 'telegram' | 'shorts' | 'reels' | 'youtube'

const emit = defineEmits<{ platform: [Platform] }>()
const showNoopWarning = ref(false)
const batchMode = ref(false)
const batchError = ref('')
const maxVariants = computed(() => exportQueueState.maxVariants || 8)
interface ExportVariantDraft { id: string; name: string; format: string; codec: string; qualityTier: string }
const variants = ref<ExportVariantDraft[]>([])
const resourcePlan = computed(() => currentBrowserExportPlan())
const mib = (bytes: number) => Math.ceil(bytes / (1024 * 1024))

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

function variantDraft(index = variants.value.length): ExportVariantDraft {
  return {
    id: crypto.randomUUID?.() ?? `${Date.now()}-${index}`,
    name: `Вариант ${index + 1}`,
    format: state.edit.format,
    codec: state.edit.codec,
    qualityTier: state.edit.qualityTier,
  }
}

function toggleBatch(): void {
  batchMode.value = !batchMode.value
  batchError.value = ''
  if (batchMode.value && variants.value.length === 0) variants.value = [variantDraft(0), variantDraft(1)]
}

function addVariant(): void {
  if (variants.value.length >= maxVariants.value) return
  variants.value.push(variantDraft())
}

function removeVariant(id: string): void {
  variants.value = variants.value.filter(item => item.id !== id)
}

async function enqueueBatch(): Promise<void> {
  batchError.value = ''
  const names = variants.value.map(item => item.name.trim())
  if (variants.value.length < 2) batchError.value = 'Добавьте минимум два варианта.'
  else if (names.some(name => !name)) batchError.value = 'У каждого варианта должно быть название.'
  else if (new Set(names.map(name => name.toLocaleLowerCase())).size !== names.length) batchError.value = 'Названия вариантов должны отличаться.'
  if (batchError.value) return
  try {
    await enqueueExportVariants(variants.value.map(variant => ({
      name: variant.name.trim(), format: variant.format, codec: variant.codec, qualityTier: variant.qualityTier,
    })))
  } catch (error) {
    batchError.value = error instanceof Error ? error.message : String(error)
  }
}

async function enqueueCurrent(): Promise<void> {
  batchError.value = ''
  try {
    await enqueueExportVariants([{
      name: `Экспорт ${state.edit.format.toUpperCase()}`,
      format: state.edit.format, codec: state.edit.codec, qualityTier: state.edit.qualityTier,
    }])
  } catch (error) {
    batchError.value = error instanceof Error ? error.message : String(error)
  }
}

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
    <button type="button" class="btn ghost sm" :aria-expanded="batchMode" aria-controls="batch-export-builder" @click="toggleBatch">
      {{ batchMode ? 'Скрыть пакетный экспорт' : 'Пакетный экспорт' }}
    </button>
    <section v-if="batchMode" id="batch-export-builder" class="batch-export-builder" aria-labelledby="batch-export-title">
      <h3 id="batch-export-title">Варианты экспорта</h3>
      <p class="hint">В очередь сохраняется неизменяемый снимок текущего монтажа и настроек каждого варианта.</p>
      <fieldset v-for="(variant, index) in variants" :key="variant.id" class="batch-variant">
        <legend>Вариант {{ index + 1 }}</legend>
        <label>Название <input v-model="variant.name" type="text" maxlength="80" :aria-label="`Название варианта ${index + 1}`"></label>
        <label>Формат
          <select v-model="variant.format" :aria-label="`Формат варианта ${index + 1}`">
            <option v-for="format in formats" :key="format.value" :value="format.value" :disabled="Boolean(unavailableReason('format', format.value))">{{ format.label }}</option>
          </select>
        </label>
        <label v-if="variant.format === 'mp4'">Кодек
          <select v-model="variant.codec" :aria-label="`Кодек варианта ${index + 1}`">
            <option value="h264" :disabled="Boolean(unavailableReason('codec', 'h264'))">H.264</option>
            <option value="h265" :disabled="Boolean(unavailableReason('codec', 'h265'))">H.265</option>
          </select>
        </label>
        <label v-if="['mp4','webm','av1'].includes(variant.format)">Качество
          <select v-model="variant.qualityTier" :aria-label="`Качество варианта ${index + 1}`">
            <option v-for="quality in qualityTiers" :key="quality.value" :value="quality.value">{{ quality.label }}</option>
          </select>
        </label>
        <button type="button" class="btn ghost sm" :aria-label="`Удалить вариант ${index + 1}`" :disabled="variants.length <= 2" @click="removeVariant(variant.id)">Удалить</button>
      </fieldset>
      <div class="batch-actions">
        <button type="button" class="btn ghost sm" :disabled="variants.length >= maxVariants" @click="addVariant">Добавить вариант</button>
        <span class="hint">{{ variants.length }} из {{ maxVariants }}</span>
        <button type="button" class="btn ghost sm" @click="enqueueCurrent">Поставить текущий вариант</button>
        <button type="button" class="btn primary sm" @click="enqueueBatch">Поставить пакет в очередь</button>
      </div>
      <p v-if="batchError" class="error" role="alert" tabindex="-1">{{ batchError }}</p>
    </section>
    <div
      v-if="resourcePlan"
      class="resource-plan"
      :class="`risk-${resourcePlan.risk}`"
      :role="resourcePlan.risk === 'blocked' ? 'alert' : 'status'"
    >
      <strong>Ресурсы локального экспорта</strong>
      <span>Пик ≈ {{ mib(resourcePlan.estimatedPeakMemoryBytes) }} МБ из безопасных {{ mib(resourcePlan.memoryBudgetBytes) }} МБ · {{ resourcePlan.inputMode.toUpperCase() }}</span>
      <span v-if="resourcePlan.reason">{{ resourcePlan.reason }}</span>
      <span v-if="resourcePlan.risk === 'blocked'">{{ resourcePlan.suggestions.join(' · ') }}</span>
    </div>
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
  <button
    v-if="streamingOutputSupported()"
    type="button"
    class="btn ghost big export-submit"
    :disabled="state.exporting"
    title="Сохраняет оригинальный выбранный диапазон в WebM напрямую в файл; фильтры не применяются"
    @click="doStreamingExport"
  >
    Потоково сохранить оригинал
  </button>
</template>

<style scoped>
.batch-export-builder,.batch-variant { display:grid; gap:.65rem; }
.batch-export-builder { padding:.8rem; border:1px solid var(--border); border-radius:.7rem; }
.batch-export-builder h3 { margin:0; }
.batch-variant { grid-template-columns:repeat(auto-fit,minmax(9rem,1fr)); align-items:end; margin:0; padding:.7rem; border:1px solid var(--border); border-radius:.6rem; }
.batch-variant label { display:grid; gap:.3rem; }
.batch-actions { display:flex; flex-wrap:wrap; align-items:center; gap:.6rem; }
</style>
