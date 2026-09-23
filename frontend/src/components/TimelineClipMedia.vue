<script setup lang="ts">
import { onMounted, ref, watch } from 'vue'

import { getAudioPeaks, getThumbnailStrip, WAVEFORM_BUCKET_COUNT } from '../browser-media-cache'
import { peakBucketRange, thumbnailSliceTicks } from '../domain/timeline-media'

const props = defineProps<{
  url: string
  kind: 'video' | 'audio' | 'image'
  mediaDurationSeconds: number | null
  sourceInTick: number
  durationTicks: number
  timeBase: number
  opacity?: number
}>()

const CANVAS_WIDTH = 256
const CANVAS_HEIGHT = 48
const SLICE_COUNT = 4

const canvas = ref<HTMLCanvasElement | null>(null)
let generation = 0

async function draw(): Promise<void> {
  const element = canvas.value
  if (!element) return
  const context = element.getContext('2d')
  if (!context) return
  const run = ++generation
  context.clearRect(0, 0, CANVAS_WIDTH, CANVAS_HEIGHT)
  try {
    if (props.kind === 'audio') {
      const { peaks, durationSeconds } = await getAudioPeaks(props.url)
      if (run !== generation) return
      const range = peakBucketRange(
        props.sourceInTick,
        props.durationTicks,
        props.mediaDurationSeconds ?? durationSeconds,
        props.timeBase,
        WAVEFORM_BUCKET_COUNT,
      )
      if (range.end <= range.start) return
      const step = CANVAS_WIDTH / (range.end - range.start)
      context.fillStyle = 'rgba(255 255 255 / 0.75)'
      for (let bucket = range.start; bucket < range.end; bucket += 1) {
        const low = peaks.min[bucket] ?? 0
        const high = peaks.max[bucket] ?? 0
        const top = ((1 - high) / 2) * CANVAS_HEIGHT
        const bottom = ((1 - low) / 2) * CANVAS_HEIGHT
        context.fillRect(
          (bucket - range.start) * step,
          top,
          Math.max(1, step - 0.5),
          Math.max(1, bottom - top),
        )
      }
    } else {
      const times = thumbnailSliceTicks(props.durationTicks, SLICE_COUNT)
        .map((tick) => (props.sourceInTick + tick) / props.timeBase)
      const frames = await getThumbnailStrip(props.url, times, 96, 54)
      if (run !== generation) return
      const sliceWidth = CANVAS_WIDTH / SLICE_COUNT
      frames.slice(0, SLICE_COUNT).forEach((frame, index) => {
        context.drawImage(
          frame,
          0,
          0,
          frame.width,
          frame.height,
          index * sliceWidth,
          0,
          Math.min(sliceWidth, CANVAS_WIDTH - index * sliceWidth),
          CANVAS_HEIGHT,
        )
      })
    }
  } catch {
    // Media is not previewable in this browser session; the clip stays a solid block.
  }
}

onMounted(() => { void draw() })
watch(
  () => [props.url, props.kind, props.sourceInTick, props.durationTicks, props.mediaDurationSeconds],
  () => { void draw() },
)
</script>

<template>
  <canvas
    :style="{ opacity: props.opacity ?? 1 }"
    ref="canvas"
    class="timeline-clip-media"
    :width="CANVAS_WIDTH"
    :height="CANVAS_HEIGHT"
    aria-hidden="true"
  />
</template>
