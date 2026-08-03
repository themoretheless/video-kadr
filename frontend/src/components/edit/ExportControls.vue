<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
  currentBrowserExportPlan,
  clientOnlyMode,
  buildExportSizingSnapshot,
  doStreamingExport,
  doExport,
  enqueueExportVariants,
  exportQueueState,
  hasMeaningfulChanges,
  selectedExportUnavailableReason,
  state,
  streamingOutputSupported,
} from '../../store'
import { DECIMAL_MB, estimateExportSize, type ExportRateControl, type ExportSizeEstimate } from '../../domain/export-size'
import { tierToCrf } from '../../domain/edit'

type Platform = 'telegram' | 'shorts' | 'reels' | 'youtube'

const emit = defineEmits<{ platform: [Platform] }>()
const showNoopWarning = ref(false)
const batchMode = ref(false)
const batchError = ref('')
const maxVariants = computed(() => exportQueueState.maxVariants || 8)
type RateMode = 'quality' | 'target_size'
interface ExportVariantDraft { id: string; name: string; format: string; codec: string; qualityTier: string; rateMode: RateMode; targetMb: number | null }
const variants = ref<ExportVariantDraft[]>([])
const rateMode = ref<RateMode>('quality')
const targetMb = ref<number | null>(25)
const resourcePlan = computed(() => currentBrowserExportPlan())
const mib = (bytes: number) => Math.ceil(bytes / (1024 * 1024))
const decimalMb = (bytes: number) => (bytes / DECIMAL_MB).toLocaleString('ru-RU', { maximumFractionDigits: 1 })
const targetFormats = new Set(['mp4', 'webm', 'av1'])
const BROWSER_TARGET_MAX_MB = 100
const targetUnsupportedReason = (format: string) => targetFormats.has(format) ? '' : 'Целевой размер недоступен для этого формата.'

function estimateFor(format: string, codec: string, qualityTier: string, mode: RateMode, mb: number | null): { estimate: ExportSizeEstimate | null; error: string } {
  if (!state.video) return { estimate: null, error: '' }
  if (mode === 'target_size' && targetUnsupportedReason(format)) return { estimate: null, error: targetUnsupportedReason(format) }
  if (mode === 'target_size' && (!(typeof mb === 'number') || !Number.isFinite(mb) || mb < 1)) return { estimate: null, error: 'Укажите целевой размер не меньше 1 МБ.' }
  const crf = tierToCrf(qualityTier, format) ?? undefined
  const snapshot = buildExportSizingSnapshot({ format, codec, ...(crf === undefined ? {} : { quality: crf }) })
  if (!snapshot) return { estimate: null, error: '' }
  if (snapshot.browser && mode === 'target_size' && mb! > BROWSER_TARGET_MAX_MB) return { estimate: null, error: `В браузере целевой размер ограничен ${BROWSER_TARGET_MAX_MB} МБ.` }
  try {
    return { estimate: estimateExportSize({
      durationSeconds: snapshot.durationSeconds, width: snapshot.width, height: snapshot.height, fps: snapshot.fps,
      format, codec, crf, muted: !snapshot.hasAudio || state.edit.mute,
      targetBytes: mode === 'target_size' ? Math.round(mb! * DECIMAL_MB) : null, browser: snapshot.browser,
    }), error: '' }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    return { estimate: null, error: /bitrate bounds|invalid target/i.test(message) ? 'Этот размер недостижим для длительности и разрешения. Измените размер, диапазон или разрешение.' : message }
  }
}

const sizeState = computed(() => estimateFor(state.edit.format, state.edit.codec, state.edit.qualityTier, rateMode.value, targetMb.value))
function rateControlFor(format: string, qualityTier: string, mode: RateMode, mb: number | null, estimate: ExportSizeEstimate | null): ExportRateControl | undefined {
  if (mode === 'target_size' && estimate?.targetVideoBitrateKbps) return {
    mode, targetBytes: Math.round(mb! * DECIMAL_MB), videoBitrateBps: estimate.targetVideoBitrateKbps * 1000,
    audioBitrateBps: estimate.audioBitrateBps, estimatorVersion: estimate.contract,
  }
  const crf = tierToCrf(qualityTier, format)
  return crf === null ? undefined : { mode: 'quality', crf }
}
const variantErrorId = (id: string) => `variant-target-error-${id}`

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
    rateMode: rateMode.value,
    targetMb: targetMb.value,
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
  else {
    const unavailable = variants.value.map(variant => unavailableReason('format', variant.format)
      || (variant.format === 'mp4' ? unavailableReason('codec', variant.codec) : '')).find(Boolean)
    if (unavailable) batchError.value = unavailable
    const invalid = variants.value.map(variant => estimateFor(variant.format, variant.codec, variant.qualityTier, variant.rateMode, variant.targetMb).error).find(Boolean)
    if (!batchError.value && invalid) batchError.value = invalid
  }
  if (batchError.value) return
  try {
    await enqueueExportVariants(variants.value.map(variant => ({
      name: variant.name.trim(), format: variant.format, codec: variant.codec, qualityTier: variant.qualityTier,
      rateControl: rateControlFor(variant.format, variant.qualityTier, variant.rateMode, variant.targetMb, estimateFor(variant.format, variant.codec, variant.qualityTier, variant.rateMode, variant.targetMb).estimate),
    })))
  } catch (error) {
    batchError.value = error instanceof Error ? error.message : String(error)
  }
}

async function enqueueCurrent(): Promise<void> {
  batchError.value = ''
  if (sizeState.value.error) { batchError.value = sizeState.value.error; return }
  try {
    await enqueueExportVariants([{
      name: `Экспорт ${state.edit.format.toUpperCase()}`,
      format: state.edit.format, codec: state.edit.codec, qualityTier: state.edit.qualityTier,
      rateControl: rateControlFor(state.edit.format, state.edit.qualityTier, rateMode.value, targetMb.value, sizeState.value.estimate),
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
  if (sizeState.value.error) { batchError.value = sizeState.value.error; return }
  if (rateMode.value === 'quality' && !hasMeaningfulChanges()) {
    showNoopWarning.value = true
    return
  }
  void doExport(rateControlFor(state.edit.format, state.edit.qualityTier, rateMode.value, targetMb.value, sizeState.value.estimate))
}

function exportUnchangedCopy(): void {
  showNoopWarning.value = false
  void doExport(rateControlFor(state.edit.format, state.edit.qualityTier, rateMode.value, targetMb.value, sizeState.value.estimate))
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
          :disabled="formatCapability(format.value)?.available === false"
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
          :disabled="codecCapability('h264')?.available === false"
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
          :disabled="codecCapability('h265')?.available === false"
          :aria-label="unavailableReason('codec', 'h265') ? `H.265. ${unavailableReason('codec', 'h265')}` : 'H.265'"
          :title="unavailableReason('codec', 'h265')"
          @click="selectCodec('h265')"
        >
          H.265
        </button>
      </div>
    </div>

    <fieldset v-if="showQuality" class="field rate-control">
      <legend>Управление размером</legend>
      <label><input v-model="rateMode" type="radio" value="quality"> По качеству</label>
      <label><input v-model="rateMode" type="radio" value="target_size"> В размер</label>
      <label v-if="rateMode === 'target_size'" for="target-size-mb">Целевой размер, МБ
        <input id="target-size-mb" v-model.number="targetMb" type="number" inputmode="decimal" min="1" :max="clientOnlyMode ? BROWSER_TARGET_MAX_MB : undefined" step="1"
          :aria-invalid="Boolean(sizeState.error)" aria-describedby="target-size-help target-size-error">
      </label>
      <small id="target-size-help">Размер — оценка: сложность сцены, звук и контейнер могут изменить результат.</small>
      <small v-if="rateMode === 'target_size' && sizeState.error" id="target-size-error" class="error" role="alert">{{ sizeState.error }}</small>
    </fieldset>
    <p v-else class="hint">{{ targetUnsupportedReason(state.edit.format) }}</p>

    <div v-if="showQuality && rateMode === 'quality'" class="field">
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
    <div v-if="sizeState.estimate" class="size-estimate" role="status" aria-live="polite" aria-atomic="true">
      <strong>Оценка размера: {{ decimalMb(sizeState.estimate.lowBytes) }}–{{ decimalMb(sizeState.estimate.highBytes) }} МБ</strong>
      <span>Ориентир {{ decimalMb(sizeState.estimate.centerBytes) }} МБ · точность {{ sizeState.estimate.confidence === 'medium' ? 'средняя' : 'низкая' }}</span>
      <span v-if="sizeState.estimate.targetVideoBitrateKbps">Видеобитрейт ≈ {{ sizeState.estimate.targetVideoBitrateKbps }} кбит/с</span>
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
          <select v-model="variant.qualityTier" :disabled="variant.rateMode === 'target_size'" :aria-label="`Качество варианта ${index + 1}`">
            <option v-for="quality in qualityTiers" :key="quality.value" :value="quality.value">{{ quality.label }}</option>
          </select>
        </label>
        <fieldset v-if="targetFormats.has(variant.format)">
          <legend>Управление размером варианта {{ index + 1 }}</legend>
          <label><input v-model="variant.rateMode" type="radio" :name="`rate-mode-${variant.id}`" value="quality"> По качеству</label>
          <label><input v-model="variant.rateMode" type="radio" :name="`rate-mode-${variant.id}`" value="target_size"> В размер</label>
          <label v-if="variant.rateMode === 'target_size'">Целевой размер, МБ
            <input v-model.number="variant.targetMb" type="number" min="1" :max="clientOnlyMode ? BROWSER_TARGET_MAX_MB : undefined" step="1" inputmode="decimal"
              :aria-label="`Целевой размер варианта ${index + 1}, МБ`"
              :aria-invalid="Boolean(estimateFor(variant.format, variant.codec, variant.qualityTier, variant.rateMode, variant.targetMb).error)"
              :aria-describedby="variantErrorId(variant.id)">
          </label>
          <small :id="variantErrorId(variant.id)" :class="{ error: estimateFor(variant.format, variant.codec, variant.qualityTier, variant.rateMode, variant.targetMb).error }" :role="estimateFor(variant.format, variant.codec, variant.qualityTier, variant.rateMode, variant.targetMb).error ? 'alert' : undefined">{{ estimateFor(variant.format, variant.codec, variant.qualityTier, variant.rateMode, variant.targetMb).error || 'Размер оценивается отдельно для этого варианта.' }}</small>
        </fieldset>
        <p v-else class="hint">{{ targetUnsupportedReason(variant.format) }}</p>
        <button type="button" class="btn ghost sm" :aria-label="`Удалить вариант ${index + 1}`" :disabled="variants.length <= 2" @click="removeVariant(variant.id)">Удалить</button>
      </fieldset>
      <div class="batch-actions">
        <button type="button" class="btn ghost sm" :disabled="variants.length >= maxVariants" @click="addVariant">Добавить вариант</button>
        <span class="hint">{{ variants.length }} из {{ maxVariants }}</span>
        <button type="button" class="btn ghost sm" :disabled="Boolean(sizeState.error || exportUnavailable)" @click="enqueueCurrent">Поставить текущий вариант</button>
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
    :disabled="state.exporting || Boolean(exportUnavailable) || Boolean(sizeState.error)"
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
