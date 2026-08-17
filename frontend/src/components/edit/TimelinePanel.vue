<script setup lang="ts">
// Timeline section. The workspace is loaded on demand: it pulls in the
// filmstrip and waveform code, which nothing else in the editor needs until the
// user actually opens the section. The collapsed body is `display: none`, so
// neither hover nor focus can mount it behind the user's back.
import { computed, defineAsyncComponent, ref } from 'vue'
import { compositionState, expectedOutputSeconds } from '../../store/composition'
import { state } from '../../store'
import PanelSection from './PanelSection.vue'

const TimelineWorkspace = defineAsyncComponent(() => import('../timeline/TimelineWorkspace.vue'))

const opened = ref(false)

const summary = computed(() => {
  const count = compositionState.clips.length
  return count ? `${count} клипов, ${expectedOutputSeconds().toFixed(1)} с` : ''
})
</script>

<template>
  <PanelSection title="Таймлайн и переходы" :summary="summary">
    <div @pointerenter="opened = true" @focusin="opened = true">
      <p v-if="!state.video" class="hint">Импортируйте клип, чтобы собрать таймлайн.</p>
      <TimelineWorkspace v-else-if="opened" />
      <button v-else type="button" class="btn sm" @click="opened = true">Открыть таймлайн</button>
    </div>
  </PanelSection>
</template>
