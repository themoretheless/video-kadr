<script setup lang="ts">
import { computed, onBeforeUnmount, ref } from 'vue'
import {
  beginEditTransaction,
  clearLut,
  doUploadLut,
  endEditTransaction,
  state,
} from '../../store'

const picker = ref<HTMLInputElement | null>(null)
let intensityTransactionOpen = false

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

function chooseFile(): void {
  if (controlsDisabled.value) return
  picker.value?.click()
}

async function onPick(event: Event): Promise<void> {
  const input = event.currentTarget as HTMLInputElement
  const file = input.files?.[0]
  // Selecting the same file again must still fire a later change event.
  input.value = ''
  if (file) await doUploadLut(file)
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

onBeforeUnmount(endIntensityTransaction)
</script>

<template>
  <div
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
      LUT не отображается в предпросмотре; точный результат виден после экспорта.
    </p>
  </div>
</template>
