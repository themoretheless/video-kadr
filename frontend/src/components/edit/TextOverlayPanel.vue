<script setup lang="ts">
// Titles, overlays and subtitles. The panel itself stays tiny: the editors and
// the live preview layer are a lazy chunk, so a session that never opens this
// section never pays for them.

import { computed, defineAsyncComponent } from 'vue'
import { overlaysState } from '../../store/overlays'
import PanelSection from './PanelSection.vue'

const TextOverlayEditor = defineAsyncComponent(
  () => import('../overlay/TextOverlayEditor.vue'),
)

const summary = computed(() => {
  const parts: string[] = []
  if (overlaysState.titles.length) parts.push(`заголовков: ${overlaysState.titles.length}`)
  if (overlaysState.overlays.length) parts.push(`наложений: ${overlaysState.overlays.length}`)
  if (overlaysState.subtitles) parts.push('субтитры')
  return parts.join(', ')
})
</script>

<template>
  <PanelSection title="Наложения и текст" :summary="summary">
    <TextOverlayEditor />
  </PanelSection>
</template>
