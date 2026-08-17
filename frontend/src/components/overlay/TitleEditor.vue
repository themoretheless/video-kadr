<script setup lang="ts">
// Title list editor. Every control here has a matching option in
// backend/src/render/graph/titles.rs, so nothing on this panel is ignored at
// export time.

import { computed } from 'vue'
import { assetsState, state } from '../../store'
import {
  addTitle,
  isSelected,
  moveTitle,
  overlaysState,
  removeTitle,
  selectItem,
} from '../../store/overlays'
import { MAX_TITLES } from '../../store/validation'
import type { TitleAlign, TitleAnimation, TitleSpec } from '../../types'
import ColorField from './ColorField.vue'

const ALIGNS: { value: TitleAlign; label: string }[] = [
  { value: 'left', label: 'По левому краю' },
  { value: 'center', label: 'По центру' },
  { value: 'right', label: 'По правому краю' },
]

const ANIMATIONS: { value: TitleAnimation; label: string }[] = [
  { value: 'none', label: 'Без анимации' },
  { value: 'fade', label: 'Проявление' },
  { value: 'slide-up', label: 'Выезд снизу' },
  { value: 'typewriter', label: 'Печатная машинка' },
  { value: 'pop', label: 'Пружина' },
]

const duration = computed(() => Math.max(0, state.edit.trimEnd - state.edit.trimStart))

const fonts = computed(() => assetsState.list.filter((asset) => asset.kind === 'font'))

function toggleBox(title: TitleSpec): void {
  title.box = title.box ? null : { color: '#000000', opacity: 0.5, padding: 12 }
}

/** An empty input means "до конца ролика", which is a null wire end. */
function setEnd(title: TitleSpec, raw: string): void {
  const value = Number.parseFloat(raw)
  title.end = Number.isFinite(value) ? Math.max(0, value) : null
}
</script>

<template>
  <div class="ovl-group">
    <div class="ovl-group-head">
      <h4>Заголовки</h4>
      <button
        class="btn ghost sm"
        :disabled="overlaysState.titles.length >= MAX_TITLES"
        @click="addTitle()"
      >
        + Добавить
      </button>
    </div>

    <p v-if="!overlaysState.titles.length" class="hint">
      Текст поверх видео: заголовки, подписи, титры. Перетаскивай прямо в плеере.
    </p>

    <article
      v-for="(title, index) in overlaysState.titles"
      :key="index"
      class="ovl-item"
      :class="{ 'is-selected': isSelected('title', index) }"
      @pointerdown="selectItem('title', index)"
    >
      <header class="ovl-item-head">
        <span class="ovl-item-name">{{ index + 1 }}. {{ title.text || 'Без текста' }}</span>
        <span class="ovl-item-actions">
          <button class="btn ghost sm" :disabled="index === 0" title="Выше" @click="moveTitle(index, -1)">
            ↑
          </button>
          <button
            class="btn ghost sm"
            :disabled="index === overlaysState.titles.length - 1"
            title="Ниже"
            @click="moveTitle(index, 1)"
          >
            ↓
          </button>
          <button class="btn ghost sm" title="Удалить" @click="removeTitle(index)">×</button>
        </span>
      </header>

      <label class="ovl-field">
        <span>Текст</span>
        <textarea v-model="title.text" class="ovl-text" rows="2" maxlength="512"></textarea>
      </label>

      <div class="grid2">
        <label>
          Размер: {{ title.fontSize }}
          <input type="range" min="8" max="200" step="1" v-model.number="title.fontSize" />
        </label>
        <label>
          Выравнивание
          <select v-model="title.align">
            <option v-for="align in ALIGNS" :key="align.value" :value="align.value">
              {{ align.label }}
            </option>
          </select>
        </label>
        <label>
          Шрифт
          <select
            :value="title.fontAssetId ?? ''"
            @change="title.fontAssetId = ($event.target as HTMLSelectElement).value || null"
          >
            <option value="">Стандартный</option>
            <option v-for="font in fonts" :key="font.id" :value="font.id">
              {{ font.filename || font.id }}
            </option>
          </select>
        </label>
        <label>
          X: {{ (title.x * 100).toFixed(0) }}%
          <input type="range" min="0" max="1" step="0.005" v-model.number="title.x" />
        </label>
        <label>
          Y: {{ (title.y * 100).toFixed(0) }}%
          <input type="range" min="0" max="1" step="0.005" v-model.number="title.y" />
        </label>
      </div>

      <div class="ovl-row">
        <ColorField v-model="title.color" label="Цвет" />
        <label class="toggle">
          <input type="checkbox" :checked="title.box !== null" @change="toggleBox(title)" />
          Подложка
        </label>
      </div>

      <template v-if="title.box">
        <div class="ovl-row">
          <ColorField v-model="title.box.color" label="Цвет подложки" />
        </div>
        <div class="grid2">
          <label>
            Прозрачность подложки: {{ title.box.opacity.toFixed(2) }}
            <input type="range" min="0" max="1" step="0.05" v-model.number="title.box.opacity" />
          </label>
          <label>
            Отступ: {{ title.box.padding }}
            <input type="range" min="0" max="80" step="1" v-model.number="title.box.padding" />
          </label>
        </div>
      </template>

      <div class="grid2">
        <label>
          Обводка: {{ (title.borderWidth ?? 0).toFixed(0) }}
          <input type="range" min="0" max="20" step="1" v-model.number="title.borderWidth" />
        </label>
        <label>
          Тень X: {{ (title.shadowX ?? 0).toFixed(0) }}
          <input type="range" min="-20" max="20" step="1" v-model.number="title.shadowX" />
        </label>
        <label>
          Тень Y: {{ (title.shadowY ?? 0).toFixed(0) }}
          <input type="range" min="-20" max="20" step="1" v-model.number="title.shadowY" />
        </label>
        <label>
          Анимация
          <select v-model="title.animation">
            <option v-for="item in ANIMATIONS" :key="item.value" :value="item.value">
              {{ item.label }}
            </option>
          </select>
        </label>
      </div>

      <div class="ovl-row">
        <ColorField
          :model-value="title.borderColor ?? '#000000'"
          label="Цвет обводки"
          @update:model-value="title.borderColor = $event"
        />
        <ColorField
          :model-value="title.shadowColor ?? '#000000'"
          label="Цвет тени"
          @update:model-value="title.shadowColor = $event"
        />
      </div>

      <div class="grid2">
        <label>
          Начало, с
          <input type="number" min="0" step="0.1" v-model.number="title.start" />
        </label>
        <label>
          Конец, с (пусто – до конца)
          <input
            type="number"
            min="0"
            step="0.1"
            :value="title.end ?? ''"
            @change="setEnd(title, ($event.target as HTMLInputElement).value)"
          />
        </label>
        <label>
          Появление: {{ (title.fadeIn ?? 0).toFixed(1) }} с
          <input type="range" min="0" max="5" step="0.1" v-model.number="title.fadeIn" />
        </label>
        <label>
          Затухание: {{ (title.fadeOut ?? 0).toFixed(1) }} с
          <input type="range" min="0" max="5" step="0.1" v-model.number="title.fadeOut" />
        </label>
      </div>

      <p class="hint">Отсчёт от начала готового ролика, всего {{ duration.toFixed(1) }} с.</p>
    </article>
  </div>
</template>
