<script setup lang="ts">
// Overlay list editor: watermarks, logos, picture-in-picture and green screen.
// Mirrors backend/src/render/graph/overlays.rs one to one.

import { computed, ref } from 'vue'
import { assetsState, assetUrl, findAsset, uploadAsset } from '../../store'
import {
  addOverlay,
  isSelected,
  moveOverlay,
  overlaysState,
  overlaysUi,
  removeOverlay,
  selectItem,
  startEyedropper,
} from '../../store/overlays'
import { MAX_OVERLAYS } from '../../store/validation'
import type { AssetEntry, OverlaySpec } from '../../types'
import ColorField from './ColorField.vue'

const fileInput = ref<HTMLInputElement | null>(null)

const media = computed(() =>
  assetsState.list.filter((asset) => asset.kind === 'image' || asset.kind === 'video'),
)

/** The uploaded file decides the kind; the server sniffs the content anyway. */
async function onFilePicked(event: Event): Promise<void> {
  const input = event.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (!file) return
  const kind = file.type.startsWith('video/') ? 'video' : 'image'
  const asset = await uploadAsset(file, kind)
  if (asset) addOverlay(asset)
}

function assetOf(overlay: OverlaySpec): AssetEntry | null {
  return findAsset(overlay.assetId)
}

function nameOf(overlay: OverlaySpec): string {
  return assetOf(overlay)?.filename || overlay.assetId
}

function toggleChromaKey(overlay: OverlaySpec): void {
  overlay.chromaKey = overlay.chromaKey
    ? null
    : { color: '#00FF00', similarity: 0.12, blend: 0.05 }
}

function toggleAudio(overlay: OverlaySpec): void {
  overlay.audio = overlay.audio ? null : { enabled: true, volume: 1 }
}

/** An empty height input means "keep the source aspect ratio". */
function setHeight(overlay: OverlaySpec, raw: string): void {
  const value = Number.parseFloat(raw)
  overlay.height = Number.isFinite(value) && value > 0 ? Math.min(4, value) : null
}

function setEnd(overlay: OverlaySpec, raw: string): void {
  const value = Number.parseFloat(raw)
  overlay.end = Number.isFinite(value) ? Math.max(0, value) : null
}
</script>

<template>
  <div class="ovl-group">
    <div class="ovl-group-head">
      <h4>Наложения</h4>
      <span class="ovl-item-actions">
        <button
          class="btn ghost sm"
          :disabled="assetsState.uploading || overlaysState.overlays.length >= MAX_OVERLAYS"
          @click="fileInput?.click()"
        >
          {{ assetsState.uploading ? 'Загрузка…' : '+ Загрузить файл' }}
        </button>
      </span>
    </div>
    <input
      ref="fileInput"
      class="hidden-file"
      type="file"
      accept="image/*,video/*"
      @change="onFilePicked"
    />

    <div v-if="media.length" class="chips">
      <button
        v-for="asset in media"
        :key="asset.id"
        class="chip"
        :disabled="overlaysState.overlays.length >= MAX_OVERLAYS"
        :title="`Добавить ${asset.filename}`"
        @click="addOverlay(asset)"
      >
        {{ asset.kind === 'video' ? '▶' : '🖼' }} {{ asset.filename || asset.id }}
      </button>
    </div>

    <p v-if="assetsState.uploadError" class="lut-error" role="alert">
      {{ assetsState.uploadError }}
    </p>
    <p v-if="!overlaysState.overlays.length" class="hint">
      Водяной знак, логотип, картинка в картинке или зелёный экран поверх кадра.
    </p>

    <article
      v-for="(overlay, index) in overlaysState.overlays"
      :key="index"
      class="ovl-item"
      :class="{ 'is-selected': isSelected('overlay', index) }"
      @pointerdown="selectItem('overlay', index)"
    >
      <header class="ovl-item-head">
        <img
          v-if="overlay.kind === 'image' && assetOf(overlay)"
          class="ovl-thumb"
          :src="assetUrl(assetOf(overlay)!)"
          alt=""
        />
        <span class="ovl-item-name">{{ index + 1 }}. {{ nameOf(overlay) }}</span>
        <span class="ovl-item-actions">
          <button class="btn ghost sm" :disabled="index === 0" title="Ниже в стопке" @click="moveOverlay(index, -1)">
            ↑
          </button>
          <button
            class="btn ghost sm"
            :disabled="index === overlaysState.overlays.length - 1"
            title="Выше в стопке"
            @click="moveOverlay(index, 1)"
          >
            ↓
          </button>
          <button class="btn ghost sm" title="Удалить" @click="removeOverlay(index)">×</button>
        </span>
      </header>

      <p v-if="!assetOf(overlay)" class="lut-error" role="alert">
        Файл не найден в библиотеке ассетов, экспорт завершится ошибкой.
      </p>

      <div class="grid2">
        <label>
          X: {{ (overlay.x * 100).toFixed(0) }}%
          <input type="range" min="-0.5" max="1.5" step="0.005" v-model.number="overlay.x" />
        </label>
        <label>
          Y: {{ (overlay.y * 100).toFixed(0) }}%
          <input type="range" min="-0.5" max="1.5" step="0.005" v-model.number="overlay.y" />
        </label>
        <label>
          Ширина: {{ (overlay.width * 100).toFixed(0) }}%
          <input type="range" min="0.01" max="2" step="0.005" v-model.number="overlay.width" />
        </label>
        <label>
          Высота, доля (пусто – по пропорциям)
          <input
            type="number"
            min="0"
            max="4"
            step="0.01"
            :value="overlay.height ?? ''"
            @change="setHeight(overlay, ($event.target as HTMLInputElement).value)"
          />
        </label>
        <label>
          Непрозрачность: {{ Math.round((overlay.opacity ?? 1) * 100) }}%
          <input type="range" min="0" max="1" step="0.05" v-model.number="overlay.opacity" />
        </label>
        <label>
          Поворот: {{ (overlay.rotation ?? 0).toFixed(0) }}°
          <input type="range" min="-180" max="180" step="1" v-model.number="overlay.rotation" />
        </label>
        <label>
          Начало, с
          <input type="number" min="0" step="0.1" v-model.number="overlay.start" />
        </label>
        <label>
          Конец, с (пусто – до конца)
          <input
            type="number"
            min="0"
            step="0.1"
            :value="overlay.end ?? ''"
            @change="setEnd(overlay, ($event.target as HTMLInputElement).value)"
          />
        </label>
        <label>
          Появление: {{ (overlay.fadeIn ?? 0).toFixed(1) }} с
          <input type="range" min="0" max="5" step="0.1" v-model.number="overlay.fadeIn" />
        </label>
        <label>
          Затухание: {{ (overlay.fadeOut ?? 0).toFixed(1) }} с
          <input type="range" min="0" max="5" step="0.1" v-model.number="overlay.fadeOut" />
        </label>
      </div>

      <div class="ovl-row">
        <label class="toggle">
          <input
            type="checkbox"
            :checked="overlay.chromaKey !== null"
            @change="toggleChromaKey(overlay)"
          />
          Хромакей
        </label>
        <label v-if="overlay.kind === 'video'" class="toggle">
          <input type="checkbox" :checked="overlay.audio !== null" @change="toggleAudio(overlay)" />
          Звук наложения
        </label>
      </div>

      <template v-if="overlay.chromaKey">
        <div class="ovl-row">
          <ColorField v-model="overlay.chromaKey.color" label="Цвет ключа">
            <button
              class="btn ghost sm"
              type="button"
              :aria-pressed="overlaysUi.eyedropper === index"
              title="Взять цвет прямо из кадра"
              @click.prevent="startEyedropper(index)"
            >
              Пипетка
            </button>
          </ColorField>
        </div>
        <div class="grid2">
          <label>
            Допуск: {{ overlay.chromaKey.similarity.toFixed(2) }}
            <input
              type="range"
              min="0"
              max="1"
              step="0.01"
              v-model.number="overlay.chromaKey.similarity"
            />
          </label>
          <label>
            Размытие края: {{ overlay.chromaKey.blend.toFixed(2) }}
            <input
              type="range"
              min="0"
              max="1"
              step="0.01"
              v-model.number="overlay.chromaKey.blend"
            />
          </label>
        </div>
      </template>

      <template v-if="overlay.kind === 'video' && overlay.audio">
        <div class="ovl-row">
          <label class="toggle">
            <input type="checkbox" v-model="overlay.audio.enabled" /> Подмешивать в звук
          </label>
        </div>
        <p class="hint">
          Звук наложения сводится в общий микс со сдвигом на время его появления.
        </p>
        <div class="grid2">
          <label>
            Громкость: {{ Math.round(overlay.audio.volume * 100) }}%
            <input type="range" min="0" max="4" step="0.05" v-model.number="overlay.audio.volume" />
          </label>
        </div>
      </template>
    </article>
  </div>
</template>

<style scoped>
.ovl-thumb {
  width: 28px;
  height: 28px;
  object-fit: contain;
  border-radius: 4px;
  background: var(--panel);
}
</style>
