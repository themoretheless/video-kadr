<script setup lang="ts">
// Audio mixing section. The wrapper stays tiny: the track list, the waveform
// decoding and the envelope editor are a dynamic import, so a user who never
// opens the section never downloads them.
//
// PanelSection keeps its body mounted with `v-show`, and it is not this
// feature's file to change, so "is the section open" is observed instead: an
// IntersectionObserver on the root reports false for a `display: none` subtree.
import { computed, defineAsyncComponent, onBeforeUnmount, ref, watch } from 'vue'
import { audioMixState } from '../../store/audioMix'
import PanelSection from './PanelSection.vue'

const AudioMixBody = defineAsyncComponent(() => import('../audio/AudioMixBody.vue'))

const root = ref<HTMLElement | null>(null)
/** Once true it stays true: unmounting the body would drop an edit in progress. */
const mounted = ref(false)

let observer: IntersectionObserver | null = null

watch(root, (element) => {
  observer?.disconnect()
  observer = null
  if (!element) return
  if (typeof IntersectionObserver === 'undefined') {
    mounted.value = true
    return
  }
  observer = new IntersectionObserver((entries) => {
    if (entries.some((entry) => entry.isIntersecting)) mounted.value = true
  })
  observer.observe(element)
})

onBeforeUnmount(() => {
  observer?.disconnect()
  observer = null
})

const summary = computed(() => {
  const parts: string[] = []
  if (audioMixState.tracks.length) parts.push(`дорожек: ${audioMixState.tracks.length}`)
  if (audioMixState.dynamics.volumeEnvelope.length) parts.push('огибающая')
  return parts.join(', ')
})
</script>

<template>
  <PanelSection title="Микс звука" :summary="summary">
    <div ref="root">
      <AudioMixBody v-if="mounted" />
      <p v-else class="hint">Загружаю инструменты звука…</p>
    </div>
  </PanelSection>
</template>
