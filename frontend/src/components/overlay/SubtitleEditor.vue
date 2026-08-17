<script setup lang="ts">
// Subtitle track: attach an .srt/.vtt asset, edit the cue list against the
// playhead and style the burn-in.
//
// The render burns in the STORED FILE, so an edited list only reaches the
// output once it has been saved back as a new asset. That is what the
// "Сохранить правки" button does, and until it is pressed the panel says so.

import { computed, onMounted, ref, watch } from 'vue'
import { seekTo, state, uploadAsset } from '../../store'
import {
  attachSubtitles,
  clearSubtitles,
  commitSubtitleCues,
  cueIndexAt,
  loadSubtitleCues,
  markCuesDirty,
  overlaysState,
  overlaysUi,
  parseSubtitleCues,
  removeCue,
  setSubtitleCues,
} from '../../store/overlays'
import type { SubtitlePosition } from '../../types'
import ColorField from './ColorField.vue'

const POSITIONS: { value: SubtitlePosition; label: string }[] = [
  { value: 'bottom', label: 'Снизу' },
  { value: 'top', label: 'Сверху' },
]

const fileInput = ref<HTMLInputElement | null>(null)
const saving = ref(false)

const outputTime = computed(() => Math.max(0, state.playerTime - state.edit.trimStart))
const activeCue = computed(() => cueIndexAt(outputTime.value))

// A restored project carries only the asset id, so the cue list is fetched
// back the first time the panel sees an attached track.
onMounted(() => void loadSubtitleCues())
watch(() => overlaysState.subtitles?.assetId, () => void loadSubtitleCues())

async function onFilePicked(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  // Parse the local copy first: the user sees the cues without a round trip,
  // and a file we cannot read never becomes an attached asset.
  const cues = parseSubtitleCues(await file.text())
  const asset = await uploadAsset(file, 'subtitle')
  if (!asset) return
  attachSubtitles(asset.id)
  setSubtitleCues(asset.id, cues)
}

async function onSave(): Promise<void> {
  saving.value = true
  try {
    await commitSubtitleCues()
  } finally {
    saving.value = false
  }
}

/** Jump the player to a cue; times are on the output timeline. */
function goToCue(index: number): void {
  const cue = overlaysUi.cues[index]
  if (cue) seekTo(state.edit.trimStart + cue.start)
}

function setCueTime(index: number, field: 'start' | 'end', raw: string): void {
  const cue = overlaysUi.cues[index]
  const value = Number.parseFloat(raw)
  if (!cue || !Number.isFinite(value)) return
  cue[field] = Math.max(0, value)
  markCuesDirty()
}
</script>

<template>
  <div class="ovl-group">
    <div class="ovl-group-head">
      <h4>Субтитры</h4>
      <span class="ovl-item-actions">
        <button class="btn ghost sm" @click="fileInput?.click()">
          {{ overlaysState.subtitles ? 'Заменить файл' : '+ Загрузить .srt/.vtt' }}
        </button>
        <button v-if="overlaysState.subtitles" class="btn ghost sm" @click="clearSubtitles()">
          Убрать
        </button>
      </span>
    </div>
    <input
      ref="fileInput"
      class="hidden-file"
      type="file"
      accept=".srt,.vtt,text/vtt,application/x-subrip"
      @change="onFilePicked"
    />

    <p v-if="!overlaysState.subtitles" class="hint">
      Загрузи файл субтитров, чтобы вшить их в кадр или отредактировать реплики.
    </p>

    <template v-else>
      <div class="ovl-row">
        <label class="toggle">
          <input type="checkbox" v-model="overlaysState.subtitles.burnIn" /> Вшить в изображение
        </label>
        <span v-if="!overlaysState.subtitles.burnIn" class="hint">
          Без вшивания субтитры не попадут в готовый файл.
        </span>
      </div>

      <template v-if="overlaysState.subtitles.burnIn">
        <div class="grid2">
          <label>
            Размер: {{ overlaysState.subtitles.fontSize }}
            <input
              type="range"
              min="8"
              max="120"
              step="1"
              v-model.number="overlaysState.subtitles.fontSize"
            />
          </label>
          <label>
            Обводка: {{ overlaysState.subtitles.outlineWidth.toFixed(0) }}
            <input
              type="range"
              min="0"
              max="10"
              step="1"
              v-model.number="overlaysState.subtitles.outlineWidth"
            />
          </label>
          <label>
            Положение
            <select v-model="overlaysState.subtitles.position">
              <option v-for="item in POSITIONS" :key="item.value" :value="item.value">
                {{ item.label }}
              </option>
            </select>
          </label>
          <label>
            Отступ от края: {{ overlaysState.subtitles.marginV }}
            <input
              type="range"
              min="0"
              max="300"
              step="1"
              v-model.number="overlaysState.subtitles.marginV"
            />
          </label>
        </div>
        <div class="ovl-row">
          <ColorField v-model="overlaysState.subtitles.color" label="Цвет текста" />
        </div>
      </template>

      <p v-if="overlaysUi.cuesLoading" class="hint">Читаем файл субтитров…</p>
      <p v-else-if="overlaysUi.cuesError" class="lut-error" role="alert">
        {{ overlaysUi.cuesError }}
      </p>

      <div v-if="overlaysUi.cues.length" class="ovl-cue-head">
        <span class="hint">Реплик: {{ overlaysUi.cues.length }}</span>
        <span class="ovl-item-actions">
          <span v-if="overlaysUi.cuesDirty" class="hint">Правки ещё не сохранены</span>
          <button
            class="btn ghost sm"
            :disabled="!overlaysUi.cuesDirty || saving"
            @click="onSave"
          >
            {{ saving ? 'Сохраняем…' : 'Сохранить правки' }}
          </button>
        </span>
      </div>

      <ol class="ovl-cues">
        <li
          v-for="(cue, index) in overlaysUi.cues"
          :key="index"
          class="ovl-cue-row"
          :class="{ 'is-active': index === activeCue }"
        >
          <button class="btn ghost sm" title="Перейти к реплике" @click="goToCue(index)">▶</button>
          <input
            class="time-input ovl-cue-time"
            type="number"
            min="0"
            step="0.1"
            :value="cue.start"
            :aria-label="`Начало реплики ${index + 1}`"
            @change="setCueTime(index, 'start', ($event.target as HTMLInputElement).value)"
          />
          <input
            class="time-input ovl-cue-time"
            type="number"
            min="0"
            step="0.1"
            :value="cue.end"
            :aria-label="`Конец реплики ${index + 1}`"
            @change="setCueTime(index, 'end', ($event.target as HTMLInputElement).value)"
          />
          <textarea
            class="ovl-text"
            rows="1"
            maxlength="512"
            :value="cue.text"
            :aria-label="`Текст реплики ${index + 1}`"
            @input="
              cue.text = ($event.target as HTMLTextAreaElement).value;
              markCuesDirty()
            "
          ></textarea>
          <button class="btn ghost sm" title="Удалить реплику" @click="removeCue(index)">×</button>
        </li>
      </ol>
    </template>
  </div>
</template>

<style scoped>
.ovl-cue-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-top: 8px;
}

.ovl-cues {
  list-style: none;
  margin: 8px 0 0;
  padding: 0;
  max-height: 260px;
  overflow-y: auto;
}

.ovl-cue-row {
  display: grid;
  grid-template-columns: auto 82px 82px 1fr auto;
  align-items: center;
  gap: 6px;
  padding: 4px;
  border-radius: 6px;
}

.ovl-cue-row.is-active {
  background: color-mix(in srgb, var(--accent) 16%, transparent);
}

.ovl-cue-time {
  width: 100%;
}
</style>
