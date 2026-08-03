<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { cubeReferenceGradient, filterLutCatalog, type LutCatalogFilter } from '../../domain/lut-catalog'
import { colorWheelsActive } from '../../domain/edit'
import { hslSelectiveActive } from '../../domain/hsl-selective'
import { primaryCorrectionsActive } from '../../domain/primary-color'
import * as lutBrowserApi from '../../lut-browser-api'
import type { LutAsset, LutBakeRequest } from '../../types'
import {
  beginEditTransaction,
  clearLut,
  doUploadLut,
  endEditTransaction,
  state,
} from '../../store'

const picker = ref<HTMLInputElement | null>(null)
let intensityTransactionOpen = false
const catalog = ref<LutAsset[]>([])
const catalogLoading = ref(false)
const catalogError = ref('')
const query = ref('')
const catalogFilter = ref<LutCatalogFilter>('all')
const candidateId = ref<string | null>(null)
const referenceGradient = ref('')
const referenceStatus = ref<'idle' | 'loading' | 'ready' | 'error'>('idle')
const referenceError = ref('')
const favoriteBusy = ref<string | null>(null)
const baking = ref(false)
const bakeError = ref('')
let referenceController: AbortController | null = null

const selected = computed(() => Boolean(state.edit.lutId))
const intensityPercent = computed({
  get: () => Math.round(state.edit.lutIntensity * 100),
  set: (value: number) => {
    const percent = Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : 100
    state.edit.lutIntensity = percent / 100
  },
})
const lutDimension = computed(() => {
  const size = state.edit.lutSize
  return size ? `${size}×${size}×${size}` : ''
})
const visibleCatalog = computed(() => filterLutCatalog(catalog.value, query.value, catalogFilter.value))
const candidate = computed(() => catalog.value.find(asset => asset.id === candidateId.value) ?? null)

const capability = computed(() =>
  state.capabilities?.filters?.find((option) =>
    ['lut', 'lut3d', 'cube-lut'].includes(option.id.toLowerCase()),
  ),
)
const unavailableReason = computed(() => {
  if (!state.capabilities) return ''
  const option = capability.value
  if (!option) return 'Нужен обновлённый сервер с поддержкой 3D LUT'
  return option.available ? '' : option.reason || 'LUT недоступны в текущей сборке сервера'
})
const controlsDisabled = computed(() => state.lutUploading || Boolean(unavailableReason.value))
const bakerCapability = computed(() => state.capabilities?.filters?.find(option => option.id === 'lut-baker-33-v1'))
const bakeRecipeUnsupportedReason = computed(() => {
  if (primaryCorrectionsActive(state.edit)) return 'Primary-коррекция пока не поддерживается baker v1'
  if (colorWheelsActive(state.edit)) return 'Lift / Gamma / Gain пока не поддерживаются baker v1'
  if (hslSelectiveActive(state.edit.hslSelective)) return 'Selective HSL пока не поддерживается baker v1'
  if (state.edit.lutId && state.edit.lutIntensity > 0) return 'Вложенный LUT пока нельзя включить в экспортируемый LUT'
  if (state.edit.filter && !['grayscale', 'sepia'].includes(state.edit.filter)) {
    return `Пресет «${state.edit.filter}» пока нельзя представить в baker v1`
  }
  return ''
})
const bakerUnavailableReason = computed(() => {
  if (!state.capabilities) return 'Проверяем возможность экспорта LUT…'
  if (!bakerCapability.value) return 'Нужен обновлённый движок с LUT baker 33³'
  if (!bakerCapability.value.available) return bakerCapability.value.reason || 'LUT baker недоступен'
  return bakeRecipeUnsupportedReason.value
})

async function loadCatalog(): Promise<void> {
  catalogLoading.value = true
  catalogError.value = ''
  try {
    const value = await lutBrowserApi.listLuts()
    catalog.value = value.filter(asset => typeof asset.id === 'string' && typeof asset.name === 'string'
      && Number.isInteger(asset.cubeSize) && asset.cubeSize >= 2 && asset.cubeSize <= 65)
    if (!candidateId.value || !catalog.value.some(asset => asset.id === candidateId.value)) {
      candidateId.value = state.edit.lutId && catalog.value.some(asset => asset.id === state.edit.lutId)
        ? state.edit.lutId : null
    }
  } catch (error) {
    catalogError.value = error instanceof Error ? error.message : String(error)
  } finally {
    catalogLoading.value = false
  }
}

function chooseFile(): void {
  if (controlsDisabled.value) return
  picker.value?.click()
}

async function onPick(event: Event): Promise<void> {
  const input = event.currentTarget as HTMLInputElement
  const file = input.files?.[0]
  // Selecting the same file again must still fire a later change event.
  input.value = ''
  if (file) {
    const uploaded = await doUploadLut(file)
    if (uploaded) {
      await loadCatalog()
      candidateId.value = uploaded.id
    }
  }
}

function applyCandidate(asset: LutAsset): void {
  if (controlsDisabled.value || referenceStatus.value !== 'ready' || candidateId.value !== asset.id) return
  beginEditTransaction('lut-catalog-apply')
  state.edit.lutId = asset.id
  state.edit.lutName = asset.name.trim() || 'LUT'
  state.edit.lutSize = asset.cubeSize
  state.edit.lutIntensity = 1
  endEditTransaction()
}

async function toggleFavorite(asset: LutAsset): Promise<void> {
  if (favoriteBusy.value) return
  favoriteBusy.value = asset.id
  catalogError.value = ''
  try {
    const updated = await lutBrowserApi.setLutFavorite(asset.id, !asset.favorite)
    const index = catalog.value.findIndex(item => item.id === asset.id)
    if (index >= 0) catalog.value[index] = { ...asset, ...updated, favorite: Boolean(updated.favorite) }
  } catch (error) {
    catalogError.value = error instanceof Error ? error.message : String(error)
  } finally {
    favoriteBusy.value = null
  }
}

async function bakeCurrentGrade(): Promise<void> {
  if (baking.value || bakerUnavailableReason.value) return
  baking.value = true
  bakeError.value = ''
  try {
    const normalizeCurves = (curves: typeof state.edit.curves) => curves && ({
      master: curves.master.map(({ x, y }) => ({ x: x / 255, y: y / 255 })),
      red: curves.red.map(({ x, y }) => ({ x: x / 255, y: y / 255 })),
      green: curves.green.map(({ x, y }) => ({ x: x / 255, y: y / 255 })),
      blue: curves.blue.map(({ x, y }) => ({ x: x / 255, y: y / 255 })),
    })
    const edit: LutBakeRequest['edit'] = {
      brightness: state.edit.brightness,
      contrast: state.edit.contrast,
      saturation: state.edit.saturation,
      curves: normalizeCurves(state.edit.curves),
    }
    if (state.edit.filter === 'grayscale' || state.edit.filter === 'sepia') edit.filter = state.edit.filter
    const result = await lutBrowserApi.bakeLut({ edit, size: 33 })
    const url = URL.createObjectURL(result.blob)
    const link = document.createElement('a')
    link.href = url
    link.download = result.filename
    link.click()
    setTimeout(() => URL.revokeObjectURL(url), 0)
  } catch (error) {
    bakeError.value = error instanceof Error ? error.message : String(error)
  } finally {
    baking.value = false
  }
}

function beginIntensityTransaction(): void {
  if (controlsDisabled.value || intensityTransactionOpen) return
  intensityTransactionOpen = true
  beginEditTransaction('lut-intensity')
}

function endIntensityTransaction(): void {
  if (!intensityTransactionOpen) return
  intensityTransactionOpen = false
  endEditTransaction()
}

watch(candidateId, async (id) => {
  referenceController?.abort()
  referenceController = null
  referenceGradient.value = ''
  referenceError.value = ''
  referenceStatus.value = id ? 'loading' : 'idle'
  if (!id) return
  const controller = new AbortController()
  referenceController = controller
  try {
    const source = await (await lutBrowserApi.getLutContent(id, controller.signal)).text()
    if (controller.signal.aborted || candidateId.value !== id) return
    referenceGradient.value = cubeReferenceGradient(source)
    referenceStatus.value = 'ready'
  } catch (error) {
    if (!controller.signal.aborted && candidateId.value === id) {
      referenceStatus.value = 'error'
      referenceError.value = error instanceof Error ? error.message : String(error)
    }
  }
})

onMounted(() => void loadCatalog())
onBeforeUnmount(() => {
  endIntensityTransaction()
  referenceController?.abort()
})
</script>

<template>
  <div
    id="lut-library"
    class="color-tool lut-control"
    :class="{ 'is-unavailable': unavailableReason }"
    :aria-busy="state.lutUploading"
  >
    <div class="color-tool-head">
      <div>
        <h3>3D LUT</h3>
        <p>Цветовой профиль в формате .cube</p>
      </div>
      <span class="color-tool-badge">.cube</span>
    </div>

    <input
      ref="picker"
      class="hidden-file"
      type="file"
      accept=".cube,text/plain,application/octet-stream"
      :disabled="controlsDisabled"
      aria-label="Выбрать LUT в формате CUBE"
      @change="onPick"
    />

    <section class="lut-browser" aria-labelledby="lut-browser-heading">
      <div class="lut-browser-heading">
        <h4 id="lut-browser-heading">Библиотека LUT</h4>
        <button type="button" class="btn ghost sm" :disabled="catalogLoading" @click="loadCatalog">
          Обновить
        </button>
      </div>
      <label class="lut-search">
        <span>Поиск LUT</span>
        <input v-model="query" type="search" placeholder="Название или 33x33x33" autocomplete="off" />
      </label>
      <div class="lut-filter" aria-label="Фильтр библиотеки">
        <button type="button" class="btn ghost sm" :aria-pressed="catalogFilter === 'all'" @click="catalogFilter = 'all'">Все</button>
        <button type="button" class="btn ghost sm" :aria-pressed="catalogFilter === 'favorites'" @click="catalogFilter = 'favorites'">★ Избранные</button>
      </div>
      <p v-if="catalogError" class="error lut-error" role="alert">
        {{ catalogError }}
        <button type="button" class="btn ghost sm" @click="loadCatalog">Повторить</button>
      </p>
      <p v-if="catalogLoading" class="lut-status" role="status">Загружаю библиотеку LUT…</p>
      <p v-else-if="!catalogError && !visibleCatalog.length" class="hint" role="status">
        {{ catalog.length ? 'По этому запросу LUT не найдены.' : 'Библиотека пуста — загрузите первый LUT.' }}
      </p>
      <ul v-if="!catalogLoading && visibleCatalog.length" class="lut-grid" aria-label="Доступные LUT">
        <li v-for="asset in visibleCatalog" :key="asset.id" class="lut-card" :class="{ 'is-candidate': candidateId === asset.id }">
          <button
            type="button"
            class="lut-card-main"
            :aria-current="candidateId === asset.id ? 'true' : undefined"
            @click="candidateId = asset.id"
          >
            <strong>{{ asset.name }}</strong>
            <span>{{ asset.cubeSize }}×{{ asset.cubeSize }}×{{ asset.cubeSize }}</span>
          </button>
          <button
            type="button"
            class="lut-favorite"
            :aria-label="asset.favorite ? `Убрать ${asset.name} из избранного` : `Добавить ${asset.name} в избранное`"
            :aria-pressed="Boolean(asset.favorite)"
            :disabled="favoriteBusy === asset.id"
            @click="toggleFavorite(asset)"
          >{{ asset.favorite ? '★' : '☆' }}</button>
        </li>
      </ul>
      <div v-if="candidate" class="lut-reference">
        <div
          class="lut-reference-strip"
          :style="referenceGradient ? { backgroundImage: referenceGradient } : undefined"
          role="img"
          :aria-label="`Цветовая референс-полоса LUT ${candidate.name}`"
        />
        <p class="hint">
          {{ referenceStatus === 'loading' ? 'Готовлю референс…' : referenceStatus === 'ready' ? 'Референс по нейтральной шкале; исходный клип не изменён.' : referenceStatus === 'error' ? `Референс недоступен: ${referenceError}` : '' }}
        </p>
        <button type="button" class="btn primary sm" :disabled="controlsDisabled || referenceStatus !== 'ready'" @click="applyCandidate(candidate)">
          Применить {{ candidate.name }}
        </button>
      </div>
    </section>

    <div v-if="selected" class="lut-selected">
      <div class="lut-selected-copy">
        <strong :title="state.edit.lutName || 'Загруженный LUT'">
          {{ state.edit.lutName || 'Загруженный LUT' }}
        </strong>
        <span v-if="lutDimension">Таблица {{ lutDimension }}</span>
        <span v-else>Размер таблицы не указан</span>
      </div>
      <div class="lut-actions">
        <button
          type="button"
          class="btn ghost sm"
          :disabled="controlsDisabled"
          @click="chooseFile"
        >
          Заменить
        </button>
        <button
          type="button"
          class="btn ghost sm lut-remove"
          :disabled="state.lutUploading"
          @click="clearLut"
        >
          Удалить
        </button>
      </div>
    </div>

    <button
      v-else
      type="button"
      class="btn ghost lut-upload"
      :disabled="controlsDisabled"
      @click="chooseFile"
    >
      {{ state.lutUploading ? 'Загружаю LUT…' : 'Загрузить .cube' }}
    </button>

    <div v-if="selected" class="lut-intensity">
      <div class="lut-intensity-head">
        <label for="lut-intensity">Интенсивность</label>
        <output for="lut-intensity">{{ intensityPercent }}%</output>
      </div>
      <input
        id="lut-intensity"
        v-model.number="intensityPercent"
        type="range"
        min="0"
        max="100"
        step="1"
        :disabled="controlsDisabled"
        :aria-valuetext="`${intensityPercent}%`"
        :aria-describedby="unavailableReason ? undefined : 'lut-preview-note'"
        @pointerdown="beginIntensityTransaction"
        @pointerup="endIntensityTransaction"
        @pointercancel="endIntensityTransaction"
        @focus="beginIntensityTransaction"
        @blur="endIntensityTransaction"
        @keydown="beginIntensityTransaction"
        @keyup="endIntensityTransaction"
      />
    </div>

    <p v-if="state.lutUploading" class="lut-status" role="status">Проверяю и загружаю LUT…</p>
    <p v-if="state.lutUploadError" class="error lut-error" role="alert">
      {{ state.lutUploadError }}
    </p>
    <p v-if="unavailableReason" class="lut-capability" role="status">
      {{ unavailableReason }}
    </p>
    <p v-if="!unavailableReason" id="lut-preview-note" class="hint lut-note">
      На сервере точный результат появляется на кадре после паузы; в браузере — после экспорта.
    </p>

    <div class="lut-baker">
      <button type="button" class="btn ghost" :disabled="baking || Boolean(bakerUnavailableReason)" @click="bakeCurrentGrade">
        {{ baking ? 'Создаю LUT 33³…' : 'Экспортировать грейд в .cube 33³' }}
      </button>
      <p v-if="bakerUnavailableReason" class="hint" role="status">{{ bakerUnavailableReason }}</p>
      <p v-if="bakeError" class="error" role="alert">{{ bakeError }}</p>
    </div>
  </div>
</template>

<style scoped>
.lut-browser { display: grid; gap: .65rem; margin-top: .8rem; padding-top: .8rem; border-top: 1px solid var(--border); }
.lut-browser-heading, .lut-filter, .lut-reference, .lut-baker { display: flex; align-items: center; gap: .5rem; flex-wrap: wrap; }
.lut-browser-heading { justify-content: space-between; }
.lut-browser-heading h4 { margin: 0; }
.lut-search { display: grid; gap: .3rem; font-size: .82rem; }
.lut-search input { width: 100%; }
.lut-filter [aria-pressed="true"] { border-color: var(--accent); color: var(--accent); }
.lut-grid { display: grid; gap: .45rem; max-height: 15rem; margin: 0; padding: 0; overflow: auto; list-style: none; }
.lut-card { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: stretch; border: 1px solid var(--border); border-radius: .65rem; overflow: hidden; }
.lut-card.is-candidate { border-color: var(--accent); }
.lut-card-main, .lut-favorite { border: 0; background: transparent; color: inherit; cursor: pointer; }
.lut-card-main { display: grid; gap: .15rem; min-width: 0; padding: .55rem .7rem; text-align: left; }
.lut-card-main strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.lut-card-main span { color: var(--muted); font-size: .76rem; }
.lut-favorite { min-width: 2.7rem; font-size: 1.2rem; }
.lut-reference { align-items: stretch; }
.lut-reference-strip { flex: 1 1 10rem; min-height: 2.4rem; border: 1px solid var(--border); border-radius: .5rem; background: var(--panel); }
.lut-reference .hint { flex-basis: 100%; margin: 0; }
.lut-baker { margin-top: .75rem; }
</style>
