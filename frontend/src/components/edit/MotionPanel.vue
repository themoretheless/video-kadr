<script setup lang="ts">
// Motion section. The workbench is loaded on demand: it pulls in the keyframe
// lane and a second video element for the Ken Burns stage, and nothing else in
// the editor needs either until the user opens the section. The collapsed body
// is `display: none`, so neither hover nor focus can mount it unasked.
import { computed, defineAsyncComponent, ref } from 'vue'
import { state } from '../../store'
import { motionState } from '../../store/motion'
import PanelSection from './PanelSection.vue'

const MotionWorkbench = defineAsyncComponent(() => import('../keyframe/MotionWorkbench.vue'))

const opened = ref(false)

const TRACK_WORDS = ['дорожек', 'дорожка', 'дорожки', 'дорожки', 'дорожки', 'дорожек']

const summary = computed(() => {
  const count = [
    motionState.zoom,
    motionState.panX,
    motionState.panY,
    motionState.rotation,
    motionState.speedRamps,
  ].filter((track) => track.length > 0).length
  return count ? `${count} ${TRACK_WORDS[count]}` : ''
})
</script>

<template>
  <PanelSection title="Движение" :summary="summary">
    <div @pointerenter="opened = true" @focusin="opened = true">
      <p v-if="!state.video" class="hint">Импортируйте видео, чтобы настроить движение кадра.</p>
      <MotionWorkbench v-else-if="opened" />
      <button v-else type="button" class="btn sm" @click="opened = true">Открыть движение</button>
    </div>
  </PanelSection>
</template>
