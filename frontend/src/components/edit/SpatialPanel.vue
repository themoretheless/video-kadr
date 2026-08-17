<script setup lang="ts">
// 360 reframing, stabilization and lens correction. The wrapper stays tiny on
// purpose: the viewport, the WebGL sampler and the keyframe lane are a dynamic
// import, so a project that never touches action-cam footage never downloads
// them.
//
// PanelSection keeps its body mounted with `v-show` and is not this feature's
// file to change, so "is the section open" is observed instead: an
// IntersectionObserver reports false for a `display: none` subtree.
import { computed, defineAsyncComponent, onBeforeUnmount, ref, watch } from 'vue'
import { spatialState } from '../../store/spatial'
import PanelSection from './PanelSection.vue'

const SpatialBody = defineAsyncComponent(() => import('../spatial/SpatialBody.vue'))

const root = ref<HTMLElement | null>(null)
/** Once true it stays true: unmounting would drop the camera the user dialled in. */
const mounted = ref(false)

let observer: IntersectionObserver | null = null

watch(root, (element) => {
  observer?.disconnect()
  observer = null
  if (!element || typeof IntersectionObserver === 'undefined') return
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
  if (spatialState.reframe360.enabled) parts.push('360')
  if (spatialState.stabilize.mode !== 'off') {
    parts.push(
      spatialState.stabilize.mode === 'fast' ? 'стабилизация' : 'стабилизация 2 прохода',
    )
  }
  const lens = spatialState.lensCorrection
  if (lens.k1 !== 0 || lens.k2 !== 0) parts.push('объектив')
  return parts.join(', ')
})
</script>

<template>
  <PanelSection title="360 и стабилизация" :summary="summary">
    <div ref="root">
      <SpatialBody v-if="mounted" />
      <p v-else class="hint">Загружаю инструменты перекадрирования…</p>
    </div>
  </PanelSection>
</template>
