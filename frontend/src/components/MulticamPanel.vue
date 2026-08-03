<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from 'vue'

import { defaultTimecodeRate, parseSmpteTimecode, parseSyncClock } from '../domain/multicam-sync-input'
import MulticamAngleViewer from './MulticamAngleViewer.vue'
// The multicam feature facade owns persistence, commands, sync jobs and
// playback state. This component deliberately contains no schema mutations.
import {
  createMulticam,
  multicamState,
  multicamTimeBase,
  projectMulticamSources,
  resyncMulticam,
  selectMulticamGroup,
  switchMulticamAngle,
} from '../multicam'

type SyncMode = 'audio' | 'timecode' | 'marker'

interface AngleView {
  id: string
  mediaId: string
  name: string
  availability: 'ready' | 'offline' | 'syncing' | 'error'
  offsetTicks?: number
}

interface GroupView {
  id: string
  name: string
  activeAngleId: string
  referenceMediaId: string
  syncMode: SyncMode
  angles: AngleView[]
}

const selectedMediaIds = ref<string[]>([])
const name = ref('Multicam 1')
const syncMode = ref<SyncMode>('marker')
const referenceMediaId = ref('')
const localError = ref('')
const alert = ref<HTMLElement | null>(null)
const markerInputs = ref<Record<string, string>>({})
const timecodeInputs = ref<Record<string, { value: string; rate: string; dropFrame: boolean }>>({})
const resyncGroupId = ref<string | null>(null)

const sources = computed(() => projectMulticamSources.value)
const activeGroup = computed<GroupView | null>(() =>
  (multicamState.groups as GroupView[]).find(group => group.id === multicamState.activeGroupId) ?? null,
)
const busy = computed(() => Boolean(multicamState.busy))
const error = computed(() => localError.value || String(multicamState.error || ''))
const canCreate = computed(() => selectedMediaIds.value.length >= 2
  && Boolean(name.value.trim())
  && Boolean(referenceMediaId.value)
  && !busy.value)

function label(source: { name: string }): string { return source.name }

function toggleSource(mediaId: string, checked: boolean): void {
  selectedMediaIds.value = checked
    ? [...new Set([...selectedMediaIds.value, mediaId])]
    : selectedMediaIds.value.filter(id => id !== mediaId)
  if (!selectedMediaIds.value.includes(referenceMediaId.value)) {
    referenceMediaId.value = selectedMediaIds.value[0] ?? ''
  }
  const source = sources.value.find(item => item.id === mediaId)
  if (checked && source) {
    markerInputs.value[mediaId] ??= '00:00.000'
    timecodeInputs.value[mediaId] ??= { value: '00:00:00:00', rate: defaultTimecodeRate(source.fps), dropFrame: false }
  }
}

async function submit(): Promise<void> {
  if (!canCreate.value) return
  localError.value = ''
  try {
    const markerAnchors = syncMode.value === 'marker' ? Object.fromEntries(selectedMediaIds.value.map(id => {
      const source = sources.value.find(item => item.id === id)!
      return [id, parseSyncClock(markerInputs.value[id] ?? '', multicamTimeBase.value, source.durationTicks)]
    })) : undefined
    const timecodeAnchors = syncMode.value === 'timecode' ? Object.fromEntries(selectedMediaIds.value.map(id => {
      const input = timecodeInputs.value[id]
      if (!input) throw new Error(`Таймкод не указан для ${sources.value.find(item => item.id === id)?.name ?? id}`)
      return [id, parseSmpteTimecode(input.value, input.rate, input.dropFrame)]
    })) : undefined
    const request = {
      name: name.value.trim(),
      mediaIds: [...selectedMediaIds.value],
      syncMode: syncMode.value,
      referenceMediaId: referenceMediaId.value,
      markerAnchors, timecodeAnchors,
    }
    if (resyncGroupId.value) await resyncMulticam(resyncGroupId.value, request)
    else await createMulticam(request)
  } catch (cause) {
    localError.value = cause instanceof Error ? cause.message : String(cause)
    await nextTick()
    alert.value?.focus()
  }
}

function beginResync(): void {
  const group = activeGroup.value
  if (!group) return
  resyncGroupId.value = group.id
  name.value = group.name
  selectedMediaIds.value = group.angles.map(angle => angle.mediaId)
  referenceMediaId.value = group.referenceMediaId || group.angles[0]?.mediaId || ''
  syncMode.value = group.syncMode ?? 'marker'
  for (const angle of group.angles) {
    markerInputs.value[angle.mediaId] = formatSyncTick(angle.offsetTicks ?? 0)
    const source = sources.value.find(item => item.id === angle.mediaId)
    timecodeInputs.value[angle.mediaId] = { value: '', rate: defaultTimecodeRate(source?.fps), dropFrame: false }
  }
}

function formatSyncTick(tick: number): string {
  const seconds = tick / multicamTimeBase.value
  const minutes = Math.floor(seconds / 60)
  return `${String(minutes).padStart(2, '0')}:${(seconds - minutes * 60).toFixed(3).padStart(6, '0')}`
}

async function chooseAngle(angle: AngleView): Promise<void> {
  const group = activeGroup.value
  if (!group || angle.availability !== 'ready' || busy.value || angle.id === group.activeAngleId) return
  localError.value = ''
  try {
    await switchMulticamAngle(group.id, angle.id, { live: Boolean(multicamState.playing) })
  } catch (cause) {
    localError.value = cause instanceof Error ? cause.message : String(cause)
    await nextTick()
    alert.value?.focus()
  }
}

function editableTarget(target: EventTarget | null): boolean {
  const element = target instanceof HTMLElement ? target : null
  return Boolean(element?.isContentEditable || element?.closest('input, textarea, select, [role="dialog"]'))
}

function onShortcut(event: KeyboardEvent): void {
  if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || editableTarget(event.target)) return
  const number = Number(event.key)
  if (!Number.isInteger(number) || number < 1 || number > 9) return
  const angle = activeGroup.value?.angles[number - 1]
  if (!angle) return
  event.preventDefault()
  void chooseAngle(angle)
}

onMounted(() => window.addEventListener('keydown', onShortcut))
onBeforeUnmount(() => window.removeEventListener('keydown', onShortcut))
</script>

<template>
  <section class="card multicam-panel" aria-labelledby="multicam-heading" :aria-busy="busy">
    <header>
      <div>
        <h2 id="multicam-heading">Multicam</h2>
        <p>Синхронизируйте камеры и переключайте ракурсы во время воспроизведения.</p>
      </div>
      <span v-if="multicamState.playing" class="multicam-live" role="status">● LIVE</span>
    </header>

    <label v-if="multicamState.groups.length" class="multicam-group-select">
      Сохранённая multicam-группа
      <select
        :value="multicamState.activeGroupId ?? ''"
        :disabled="busy"
        @change="selectMulticamGroup(($event.target as HTMLSelectElement).value || null)"
      >
        <option value="" disabled>Выберите группу</option>
        <option v-for="group in multicamState.groups" :key="group.id" :value="group.id">
          {{ group.name }} · {{ group.angles.length }} камер
        </option>
      </select>
    </label>

    <form class="multicam-create" aria-labelledby="multicam-create-heading" @submit.prevent="submit">
      <h3 id="multicam-create-heading">{{ resyncGroupId ? 'Пересинхронизировать multicam' : 'Создать multicam' }}</h3>
      <fieldset>
        <legend>Видеоисточники — выберите минимум два</legend>
        <p v-if="!sources.length" class="hint" role="status">Добавьте видео в библиотеку проекта.</p>
        <label v-for="source in sources" :key="source.id" class="multicam-source">
          <input
            type="checkbox"
            :checked="selectedMediaIds.includes(source.id)"
            :disabled="busy || source.availability !== 'ready'"
            @change="toggleSource(source.id, ($event.target as HTMLInputElement).checked)"
          >
          <span>{{ label(source) }}<template v-if="source.availability !== 'ready'"> · недоступен</template></span>
        </label>
      </fieldset>

      <label>
        Название
        <input v-model="name" type="text" maxlength="120" required :disabled="busy" autocomplete="off">
      </label>

      <fieldset>
        <legend>Способ синхронизации</legend>
        <label><input v-model="syncMode" type="radio" value="audio" :disabled="busy"> По звуку</label>
        <label><input v-model="syncMode" type="radio" value="timecode" :disabled="busy"> По таймкоду</label>
        <label><input v-model="syncMode" type="radio" value="marker" :disabled="busy"> По маркерам</label>
      </fieldset>
      <p v-if="syncMode === 'audio'" class="hint">Локальный FFmpeg извлекает первые 20 секунд в mono 50 Hz. WORKERFS не копирует исходник в память; ограниченный MEMFS fallback загружает файл целиком и доступен только до 64 МБ. Неоднозначный или тихий звук будет отклонён.</p>

      <fieldset v-if="syncMode === 'marker'" class="sync-inputs">
        <legend>Ручные sync-точки источников</legend>
        <label v-for="source in sources.filter(item => selectedMediaIds.includes(item.id))" :key="source.id">
          <span>{{ source.name }}</span>
          <input
            v-model="markerInputs[source.id]"
            type="text"
            inputmode="decimal"
            placeholder="00:00.000"
            :disabled="busy"
            :aria-label="`Sync-точка ${source.name}`"
          >
        </label>
      </fieldset>

      <fieldset v-if="syncMode === 'timecode'" class="sync-inputs timecode-inputs">
        <legend>Начальный SMPTE-таймкод каждого источника</legend>
        <div v-for="source in sources.filter(item => selectedMediaIds.includes(item.id))" :key="source.id" class="timecode-row">
          <label>
            <span>{{ source.name }}</span>
            <input v-model="timecodeInputs[source.id].value" type="text" placeholder="00:00:00:00" :disabled="busy" :aria-label="`Таймкод ${source.name}`">
          </label>
          <label>
            FPS
            <select v-model="timecodeInputs[source.id].rate" :disabled="busy" :aria-label="`Частота таймкода ${source.name}`">
              <option v-for="rate in ['23.976','24','25','29.97','30','50','59.94','60']" :key="rate" :value="rate">{{ rate }}</option>
            </select>
          </label>
          <label>
            <input
              v-model="timecodeInputs[source.id].dropFrame"
              type="checkbox"
              :disabled="busy || !['29.97', '59.94'].includes(timecodeInputs[source.id].rate)"
            > Drop-frame
          </label>
        </div>
      </fieldset>

      <label>
        Опорная камера
        <select v-model="referenceMediaId" required :disabled="busy || selectedMediaIds.length < 2">
          <option value="" disabled>Выберите камеру</option>
          <option v-for="source in sources.filter(item => selectedMediaIds.includes(item.id))" :key="source.id" :value="source.id">
            {{ label(source) }}
          </option>
        </select>
      </label>

      <button class="btn primary" type="submit" :disabled="!canCreate">
        {{ busy ? 'Синхронизация…' : resyncGroupId ? 'Применить ресинхронизацию' : 'Создать multicam' }}
      </button>
    </form>

    <p v-if="multicamState.status" class="multicam-status" role="status" aria-live="polite">
      {{ multicamState.status }}
    </p>
    <p v-if="error" ref="alert" class="error" role="alert" tabindex="-1">{{ error }}</p>

    <section v-if="activeGroup" class="angle-viewer" :aria-labelledby="`angle-viewer-${activeGroup.id}`">
      <div class="angle-viewer-heading">
        <h3 :id="`angle-viewer-${activeGroup.id}`">Ракурсы · {{ activeGroup.name }}</h3>
        <span class="hint">Клавиши 1–9 переключают доступные камеры</span>
      </div>
      <button type="button" class="btn ghost sm" :disabled="busy" @click="beginResync">Настроить синхронизацию</button>
      <MulticamAngleViewer :group-id="activeGroup.id" :active-angle-id="activeGroup.activeAngleId" />
      <div class="angle-grid" role="group" aria-label="Выбор ракурса multicam">
        <button
          v-for="(angle, index) in activeGroup.angles"
          :key="angle.id"
          type="button"
          class="angle-button"
          :class="{ active: angle.id === activeGroup.activeAngleId }"
          :disabled="busy || angle.availability !== 'ready'"
          :aria-pressed="angle.id === activeGroup.activeAngleId"
          :aria-keyshortcuts="index < 9 ? String(index + 1) : undefined"
          :aria-label="`${index + 1}. ${angle.name}; ${angle.id === activeGroup.activeAngleId ? 'эфирный ракурс' : angle.availability === 'ready' ? 'доступен' : 'недоступен'}`"
          @click="chooseAngle(angle)"
        >
          <span class="angle-number" aria-hidden="true">{{ index + 1 }}</span>
          <strong>{{ angle.name }}</strong>
          <span>{{ angle.id === activeGroup.activeAngleId ? 'В эфире' : angle.availability === 'ready' ? 'Переключить' : angle.availability === 'syncing' ? 'Синхронизация…' : 'Недоступен' }}</span>
        </button>
      </div>
    </section>
  </section>
</template>

<style scoped>
.multicam-panel, .multicam-create, .angle-viewer { display: grid; gap: .85rem; }
header, .angle-viewer-heading { display: flex; align-items: start; justify-content: space-between; gap: 1rem; }
header h2, header p, h3 { margin: 0; }
.multicam-live { color: #ff6969; font-weight: 800; }
fieldset { display: flex; flex-wrap: wrap; gap: .65rem 1rem; margin: 0; padding: .75rem; border: 1px solid var(--border); border-radius: .65rem; }
legend { padding: 0 .35rem; font-weight: 700; }
.multicam-source { min-width: 12rem; }
.multicam-create > label { display: grid; gap: .3rem; }
.angle-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr)); gap: .65rem; }
.angle-button { position: relative; display: grid; gap: .35rem; min-height: 6rem; padding: .8rem; border: 2px solid var(--border); border-radius: .7rem; background: var(--panel); color: inherit; text-align: left; cursor: pointer; }
.angle-button.active { border-color: var(--accent); box-shadow: inset 0 0 0 1px var(--accent); }
.angle-button:focus-visible { outline: 3px solid var(--accent); outline-offset: 2px; }
.angle-button:disabled { cursor: not-allowed; opacity: .62; }
.angle-number { position: absolute; top: .4rem; right: .5rem; font-weight: 800; }
.angle-button span:last-child { color: var(--muted); font-size: .8rem; }
@media (max-width: 560px) { header, .angle-viewer-heading { display: grid; } }
</style>
