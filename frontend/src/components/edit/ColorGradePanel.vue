<script setup lang="ts">
// Resolve-lite grading section. This wrapper stays tiny on purpose: the wheels,
// the scope math and the canvas work are a dynamic import, so a user who never
// opens the section never downloads them.
//
// PanelSection keeps its body mounted with `v-show`, and it is not this
// feature's file to change, so "is the section open" is observed instead: an
// IntersectionObserver on the root reports false for a `display: none` subtree.
// That same flag is what pauses the scope sampling loop.
import { computed, defineAsyncComponent, onBeforeUnmount, ref, watch } from 'vue'
import { colorAdvancedPayload, colorAdvancedState } from '../../store/colorAdvanced'
import PanelSection from './PanelSection.vue'

const ColorGradeBody = defineAsyncComponent(() => import('../color/ColorGradeBody.vue'))

const root = ref<HTMLElement | null>(null)
const visible = ref(false)
/** Once true it stays true: unmounting the body would drop the user's scroll and tabs. */
const mounted = ref(false)

let observer: IntersectionObserver | null = null

watch(root, (element) => {
  observer?.disconnect()
  observer = null
  if (!element || typeof IntersectionObserver === 'undefined') return
  observer = new IntersectionObserver((entries) => {
    visible.value = entries.some((entry) => entry.isIntersecting)
    if (visible.value) mounted.value = true
  })
  observer.observe(element)
})

onBeforeUnmount(() => {
  observer?.disconnect()
  observer = null
})

const summary = computed(() => {
  if (!Object.keys(colorAdvancedPayload()).length) return ''
  const bands = colorAdvancedState.hsl.length
  return bands ? `коррекция, HSL: ${bands}` : 'коррекция'
})
</script>

<template>
  <PanelSection title="Цветокоррекция" :summary="summary">
    <div ref="root">
      <ColorGradeBody v-if="mounted" :active="visible" />
      <p v-else class="hint">Загружаю инструменты коррекции…</p>
    </div>
  </PanelSection>
</template>
