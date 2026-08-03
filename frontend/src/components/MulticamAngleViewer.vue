<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from 'vue'

import { multicamAngleSourceSeconds, type MulticamGroup } from '../domain/multicam'
import { resolveMulticamAngleSource } from '../multicam'
import { state, timelineState } from '../store'

const props = defineProps<{ groupId: string; activeAngleId: string }>()
const urls = ref<Record<string, string>>({})
const statuses = ref<Record<string, string>>({})
const elements = new Map<string, HTMLVideoElement>()
let generation = 0

const inClipBounds = computed(() => {
  const current = group()
  const clip = timelineState.document?.sequences.flatMap(sequence => sequence.tracks).flatMap(track => track.clips)
    .find(item => item.multicamGroupId === current?.id)
  if (!current || !clip) return false
  const tick = state.playerTime * current.timeBase
  return tick >= clip.timelineStartTick && tick < clip.timelineStartTick + clip.durationTicks
})

function group(): MulticamGroup | null {
  return timelineState.document?.multicamGroups.find(item => item.id === props.groupId) as MulticamGroup | undefined ?? null
}

function setVideo(angleId: string, element: unknown): void {
  if (element instanceof HTMLVideoElement) elements.set(angleId, element)
  else elements.delete(angleId)
}

async function resolveSources(): Promise<void> {
  const currentGeneration = ++generation
  const document = timelineState.document
  const current = group()
  if (!document || !current) return
  const resolved: Record<string, string> = {}
  for (const angle of current.angles) {
    statuses.value[angle.id] = 'Загрузка…'
    try {
      const url = await resolveMulticamAngleSource(current.id, angle.id)
      if (currentGeneration !== generation) return
      resolved[angle.id] = url
      statuses.value[angle.id] = 'Готов'
    } catch (error) {
      statuses.value[angle.id] = error instanceof Error ? error.message : String(error)
    }
  }
  if (currentGeneration === generation) { urls.value = resolved; await nextTick(); syncPlayers() }
}

function syncPlayers(): void {
  const current = group()
  if (!current) return
  const clip = timelineState.document?.sequences.flatMap(sequence => sequence.tracks).flatMap(track => track.clips)
    .find(item => item.multicamGroupId === current.id)
  const localTick = Math.round(state.playerTime * current.timeBase) - (clip?.timelineStartTick ?? 0)
  const outputTick = Math.min(clip?.sourceOutTick ? clip.sourceOutTick - 1 : current.durationTicks - 1,
    Math.max(clip?.sourceInTick ?? 0, (clip?.sourceInTick ?? 0) + localTick))
  for (const angle of current.angles) {
    const video = elements.get(angle.id)
    if (!video || !urls.value[angle.id]) continue
    if (!inClipBounds.value) { video.pause(); continue }
    const sourceSeconds = multicamAngleSourceSeconds(current, angle.id, outputTick)
    if (Math.abs(video.currentTime - sourceSeconds) > 1 / 15) video.currentTime = sourceSeconds
    video.muted = true
    const angleRate = angle.rate.numerator / angle.rate.denominator
    video.playbackRate = Math.max(.0625, Math.min(16, angleRate * state.edit.speed))
    if (state.playerPlaying) void video.play().catch(() => undefined)
    else video.pause()
  }
}

watch(() => [props.groupId, timelineState.revision], () => void resolveSources(), { immediate: true })
watch(() => [state.playerTime, state.playerPlaying, state.playerSeeking], syncPlayers)
onBeforeUnmount(() => { generation++; for (const video of elements.values()) video.pause(); elements.clear() })
</script>

<template>
  <div class="multicam-video-grid" aria-label="Синхронизированные камеры">
    <figure
      v-for="angle in group()?.angles ?? []"
      :key="angle.id"
      :class="{ program: angle.id === activeAngleId }"
    >
      <video
        v-if="urls[angle.id] && inClipBounds"
        :ref="element => setVideo(angle.id, element)"
        :src="urls[angle.id]"
        muted
        playsinline
        preload="metadata"
        :aria-label="`${angle.label}; ${angle.id === activeAngleId ? 'программный ракурс' : 'камера'}`"
      />
      <div v-else class="multicam-video-placeholder" role="status">{{ !inClipBounds ? 'Вне multicam-клипа' : statuses[angle.id] || 'Источник недоступен' }}</div>
      <figcaption>{{ angle.label }} · {{ angle.id === activeAngleId ? 'PROGRAM' : 'камера' }}</figcaption>
    </figure>
  </div>
</template>

<style scoped>
.multicam-video-grid { display:grid; grid-template-columns:repeat(auto-fit,minmax(12rem,1fr)); gap:.65rem; }
figure { margin:0; border:2px solid var(--border); border-radius:.65rem; overflow:hidden; background:#05070a; }
figure.program { border-color:var(--accent); box-shadow:0 0 0 1px var(--accent); }
video,.multicam-video-placeholder { display:block; width:100%; aspect-ratio:16/9; object-fit:contain; background:#000; }
.multicam-video-placeholder { display:grid; place-items:center; padding:.5rem; color:var(--muted); }
figcaption { padding:.45rem .6rem; font-size:.82rem; }
</style>
