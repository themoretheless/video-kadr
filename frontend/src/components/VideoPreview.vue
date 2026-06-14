<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { state } from '../store'
import CropOverlay from './CropOverlay.vue'

const videoEl = ref<HTMLVideoElement | null>(null)

// While playing, loop within the trim region so the preview reflects the cut.
function onTimeUpdate() {
  const el = videoEl.value
  if (!el) return
  state.playerTime = el.currentTime
  applyPlayback(el)
  if (el.paused) return
  const { trimStart, trimEnd } = state.edit
  if (el.currentTime > trimEnd) {
    el.currentTime = trimStart
  }
}

// Live preview of speed/volume (color + flip are pure CSS via videoStyle).
function applyPlayback(el: HTMLVideoElement) {
  const s = state.edit.speed
  if (s > 0 && el.playbackRate !== s) el.playbackRate = s
  const v = Math.min(1, Math.max(0, state.edit.volume))
  if (el.volume !== v) el.volume = v
}

watch(
  () => [state.edit.speed, state.edit.volume],
  () => {
    if (videoEl.value) applyPlayback(videoEl.value)
  },
)

// CSS approximation of the colour/flip effects so the user sees them live.
const videoStyle = computed(() => {
  const e = state.edit
  const f: string[] = []
  if (e.brightness) f.push(`brightness(${(1 + e.brightness).toFixed(3)})`)
  if (e.contrast !== 1) f.push(`contrast(${e.contrast})`)
  if (e.saturation !== 1) f.push(`saturate(${e.saturation})`)
  const presets: Record<string, string> = {
    grayscale: 'grayscale(1)',
    sepia: 'sepia(0.6)',
    warm: 'sepia(0.4) saturate(1.3)',
    cold: 'hue-rotate(-12deg) saturate(1.15)',
  }
  if (e.filter && presets[e.filter]) f.push(presets[e.filter])
  const sx = e.flipH ? -1 : 1
  const sy = e.flipV ? -1 : 1
  return {
    filter: f.length ? f.join(' ') : undefined,
    transform: sx !== 1 || sy !== 1 ? `scaleX(${sx}) scaleY(${sy})` : undefined,
  }
})

// Reload the player when a new source is imported.
watch(
  () => state.video?.url,
  () => {
    videoEl.value?.load()
  },
)

// Player bridge: react to seek requests and play/pause toggles from hotkeys.
watch(
  () => state.seekTo,
  (t) => {
    if (t != null && videoEl.value) {
      videoEl.value.currentTime = t
      state.seekTo = null
    }
  },
)

watch(
  () => state.playToggle,
  () => {
    const el = videoEl.value
    if (!el) return
    if (el.paused) void el.play()
    else el.pause()
  },
)

function fmtDuration(t: number): string {
  if (!isFinite(t)) return '0:00'
  const m = Math.floor(t / 60)
  const s = Math.floor(t % 60)
  return `${m}:${s.toString().padStart(2, '0')}`
}

function fmtSize(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} ГБ`
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} МБ`
  return `${Math.max(1, Math.round(bytes / 1024))} КБ`
}

const meta = computed(() => {
  const v = state.video
  if (!v) return [] as string[]
  const parts: string[] = []
  if (v.width && v.height) parts.push(`${v.width}×${v.height}`)
  parts.push(`${fmtDuration(v.duration)}`)
  if (v.fps) parts.push(`${v.fps.toFixed(1)} fps`)
  const codecs = [v.vcodec, v.acodec].filter(Boolean).join(' / ')
  if (codecs) parts.push(codecs)
  if (typeof v.sizeBytes === 'number') parts.push(fmtSize(v.sizeBytes))
  return parts
})
</script>

<template>
  <div class="card preview">
    <h2 v-if="state.video?.title" class="preview-title" :title="state.video.title">
      {{ state.video.title }}
    </h2>
    <div class="player-wrap">
      <video
        ref="videoEl"
        class="player"
        :src="state.video?.url"
        :style="videoStyle"
        :muted="state.edit.mute"
        controls
        playsinline
        @timeupdate="onTimeUpdate"
      ></video>
      <CropOverlay v-if="state.video && state.edit.cropEnabled" />
    </div>
    <div v-if="state.video" class="meta">
      <span v-for="(m, i) in meta" :key="i" class="meta-chip">{{ m }}</span>
    </div>
    <p v-if="state.video" class="hint">Превью показывает оригинал, обрезка зациклена внутри выбранного отрезка.</p>
  </div>
</template>
