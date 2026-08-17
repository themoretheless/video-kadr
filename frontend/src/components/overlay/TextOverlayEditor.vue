<script setup lang="ts">
// Body of the "Наложения и текст" panel. Loaded lazily by TextOverlayPanel.vue
// so the three editors and the preview layer stay out of the entry chunk.
//
// The style block is intentionally NOT scoped: the `ovl-` classes are shared by
// the three sibling editors, and duplicating them per component would cost more
// CSS than the shared block does.

import { onMounted } from 'vue'
import { assetsState, loadAssets } from '../../store'
import OverlayEditor from './OverlayEditor.vue'
import PreviewOverlayLayer from './PreviewOverlayLayer.vue'
import SubtitleEditor from './SubtitleEditor.vue'
import TitleEditor from './TitleEditor.vue'

// Asset pickers are empty until the library has been read once.
onMounted(() => {
  if (!assetsState.list.length && !assetsState.loading) void loadAssets()
})
</script>

<template>
  <div class="ovl-editor">
    <p class="hint">
      Перетаскивай заголовки и наложения прямо в плеере. Стрелки двигают выбранный
      элемент, Shift ускоряет шаг, Alt со стрелками меняет размер наложения.
    </p>
    <p v-if="assetsState.loadError" class="lut-error" role="alert">
      {{ assetsState.loadError }}
    </p>

    <TitleEditor />
    <OverlayEditor />
    <SubtitleEditor />

    <PreviewOverlayLayer />
  </div>
</template>

<style>
.ovl-group {
  margin-top: 12px;
}

.ovl-group-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}

.ovl-group-head h4 {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.ovl-item {
  margin-top: 8px;
  padding: 8px;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--panel);
}

.ovl-item.is-selected {
  border-color: var(--accent);
}

.ovl-item-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
}

.ovl-item-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
}

.ovl-item-actions {
  display: flex;
  align-items: center;
  gap: 4px;
}

.ovl-row {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
  margin: 6px 0;
}

.ovl-field {
  display: block;
  font-size: 13px;
  color: var(--muted);
}

.ovl-text {
  display: block;
  width: 100%;
  margin-top: 4px;
  padding: 6px 8px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--panel-2);
  color: var(--text);
  font: inherit;
  font-size: 13px;
  resize: vertical;
}

.ovl-text:focus {
  outline: none;
  box-shadow: var(--ring);
}

.ovl-group select {
  width: 100%;
  margin-top: 4px;
  padding: 5px 6px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--panel-2);
  color: var(--text);
  font: inherit;
  font-size: 13px;
}
</style>
