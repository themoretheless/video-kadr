<script setup lang="ts">
import { ref, watch } from 'vue'
import { state } from '../store'

const videoEl = ref<HTMLVideoElement | null>(null)

// While playing, loop within the trim region so the preview reflects the cut.
function onTimeUpdate() {
  const el = videoEl.value
  if (!el || el.paused) return
  const { trimStart, trimEnd } = state.edit
  if (el.currentTime > trimEnd) {
    el.currentTime = trimStart
  }
}

// Reload the player when a new source is imported.
watch(
  () => state.video?.url,
  () => {
    videoEl.value?.load()
  },
)
</script>

<template>
  <div class="card preview">
    <video
      ref="videoEl"
      class="player"
      :src="state.video?.url"
      controls
      playsinline
      @timeupdate="onTimeUpdate"
    ></video>
    <div v-if="state.video" class="meta">
      {{ state.video.width }}×{{ state.video.height }} ·
      {{ state.video.duration.toFixed(1) }} c · превью показывает оригинал (обрезка зациклена)
    </div>
  </div>
</template>
