<script setup lang="ts">
// Live preview of the overlays, titles and the current subtitle cue, drawn on
// top of the player so that what the user drags is what the render produces.
//
// The layer teleports itself into `.player-wrap`, which VideoPreview.vue owns.
// Teleporting keeps this feature entirely inside the files this agent owns and
// still puts the boxes over the video; `defer` makes the target lookup happen
// after the whole tree has rendered, so mount order does not matter.
//
// Every placement goes through domain/contentBox.ts, so the boxes track the
// real video content box rather than the letterboxed element box.

import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import {
  boxToNormalized,
  contentBox,
  overlayPlacement,
  titlePlacement,
  type ContentBox,
  type Size,
} from '../../domain/contentBox'
import {
  applyEyedropper,
  cancelEyedropper,
  dragOverlay,
  dragTitle,
  isSelected,
  overlayAspect,
  overlaysState,
  overlaysUi,
  overlayVisibleAt,
  selectItem,
  titleVisibleAt,
} from '../../store/overlays'
import { assetUrl, beginEditTransaction, endEditTransaction, findAsset, state } from '../../store'
import { toast } from '../../toasts'
import DragBox from './DragBox.vue'
import type { BoxTransform } from './types'

const root = ref<HTMLElement | null>(null)
const elementSize = ref({ width: 0, height: 0 })
const teleportReady = ref(false)
let observer: ResizeObserver | null = null

onMounted(() => {
  teleportReady.value = document.querySelector('.player-wrap') !== null
})

// The layer is `inset: 0` inside `.player-wrap`, which hugs the video element,
// so observing the layer is the same as observing the player.
watch(root, (element) => {
  observer?.disconnect()
  observer = null
  if (!element || typeof ResizeObserver === 'undefined') return
  observer = new ResizeObserver(([entry]) => {
    elementSize.value = {
      width: entry.contentRect.width,
      height: entry.contentRect.height,
    }
  })
  observer.observe(element)
  const rect = element.getBoundingClientRect()
  elementSize.value = { width: rect.width, height: rect.height }
})

onBeforeUnmount(() => {
  observer?.disconnect()
  observer = null
  cancelEyedropper()
})

const source = computed(() => {
  const video = state.video
  return video && video.width && video.height ? { width: video.width, height: video.height } : null
})

const box = computed<ContentBox>(() => contentBox(elementSize.value, source.value))

/**
 * Overlay and title times are on the OUTPUT timeline, while the player shows
 * the untrimmed source, so the playhead has to be rebased before it can gate
 * anything.
 */
const outputTime = computed(() => Math.max(0, state.playerTime - state.edit.trimStart))

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, Number.isFinite(value) ? value : min))
}

// --- overlays --------------------------------------------------------------

interface OverlayView {
  index: number
  left: number
  top: number
  width: number
  height: number
  rotation: number
  opacity: number
  url: string
  isVideo: boolean
  /** Outside its own time window: shown only because it is being edited. */
  dimmed: boolean
  label: string
}

// A selected layer stays on screen outside its time window, dimmed, so it can
// still be dragged without first scrubbing the playhead onto it.
const overlayViews = computed<OverlayView[]>(() =>
  overlaysState.overlays
    .map((overlay, index) => ({ overlay, index }))
    .filter(
      ({ overlay, index }) =>
        overlayVisibleAt(overlay, outputTime.value) || isSelected('overlay', index),
    )
    .map(({ overlay, index }) => {
      const placement = overlayPlacement(box.value, overlay, overlayAspect(overlay))
      const asset = findAsset(overlay.assetId)
      return {
        index,
        left: placement.left,
        top: placement.top,
        width: placement.width,
        height: placement.height,
        rotation: placement.rotation,
        opacity: overlay.opacity ?? 1,
        url: asset ? assetUrl(asset) : '',
        isVideo: overlay.kind === 'video',
        dimmed: !overlayVisibleAt(overlay, outputTime.value),
        label: `Наложение ${index + 1}: ${asset?.filename ?? overlay.assetId}`,
      }
    }),
)

function onOverlayTransform(index: number, change: BoxTransform): void {
  dragOverlay(index, box.value, change)
}

/** Live preview media per overlay index, used for scrubbing and the eyedropper. */
const overlayMedia = new Map<number, HTMLImageElement | HTMLVideoElement>()

function registerOverlayMedia(index: number, element: unknown): void {
  if (element instanceof HTMLImageElement || element instanceof HTMLVideoElement) {
    overlayMedia.set(index, element)
  } else {
    overlayMedia.delete(index)
  }
}

// A video overlay has its own timeline, so its preview element is scrubbed to
// the frame the render would composite instead of sitting on its poster frame.
watch([outputTime, overlayViews], () => {
  for (const [index, element] of overlayMedia) {
    if (!(element instanceof HTMLVideoElement)) continue
    const overlay = overlaysState.overlays[index]
    if (!overlay || !element.duration || !Number.isFinite(element.duration)) continue
    const wanted = clamp(outputTime.value - (overlay.start ?? 0), 0, element.duration)
    // Only reseek on a real jump: assigning currentTime every tick stalls the
    // decoder and makes the preview flicker.
    if (Math.abs(element.currentTime - wanted) > 0.2) element.currentTime = wanted
  }
})

// --- titles ----------------------------------------------------------------

interface TitleView {
  index: number
  left: number
  top: number
  anchorX: number
  style: Record<string, string>
  boxStyle: Record<string, string>
  text: string
  dimmed: boolean
  label: string
}

function rgba(hex: string, alpha: number): string {
  const value = /^#([0-9a-fA-F]{6})$/.exec(hex)
  if (!value) return `rgba(0, 0, 0, ${alpha})`
  const numeric = Number.parseInt(value[1], 16)
  const [r, g, b] = [(numeric >> 16) & 255, (numeric >> 8) & 255, numeric & 255]
  return `rgba(${r}, ${g}, ${b}, ${alpha})`
}

const titleViews = computed<TitleView[]>(() =>
  overlaysState.titles
    .map((title, index) => ({ title, index }))
    .filter(
      ({ title, index }) => titleVisibleAt(title, outputTime.value) || isSelected('title', index),
    )
    .map(({ title, index }) => {
      const placement = titlePlacement(box.value, source.value, title)
      const boxStyle: Record<string, string> = {}
      if (title.box) {
        boxStyle.background = rgba(title.box.color, title.box.opacity)
        boxStyle.padding = `${placement.paddingPx}px`
      }
      const shadow =
        placement.shadowXPx !== 0 || placement.shadowYPx !== 0
          ? `${placement.shadowXPx}px ${placement.shadowYPx}px 0 ${title.shadowColor ?? '#000000'}`
          : 'none'
      return {
        index,
        left: placement.left,
        top: placement.top,
        anchorX: placement.anchorX,
        style: {
          fontSize: `${placement.fontSizePx}px`,
          color: title.color,
          textShadow: shadow,
          // drawtext's `borderw` paints an outline around the glyphs.
          WebkitTextStrokeWidth: `${placement.borderPx}px`,
          WebkitTextStrokeColor: title.borderColor ?? '#000000',
        },
        boxStyle,
        text: title.text,
        dimmed: !titleVisibleAt(title, outputTime.value),
        label: `Заголовок ${index + 1}: ${title.text}`,
      }
    }),
)

function onTitleTransform(index: number, change: BoxTransform): void {
  dragTitle(index, box.value, change)
}

// --- subtitle cue ----------------------------------------------------------

const currentCue = computed(() => {
  const spec = overlaysState.subtitles
  if (!spec?.burnIn) return null
  const time = outputTime.value
  const cue = overlaysUi.cues.find((entry) => time >= entry.start && time <= entry.end)
  if (!cue) return null
  const scale = box.value.height > 0 && source.value ? box.value.height / source.value.height : 1
  return {
    text: cue.text,
    style: {
      color: spec.color,
      fontSize: `${(spec.fontSize * box.value.height) / 1080}px`,
      WebkitTextStrokeWidth: `${spec.outlineWidth * scale}px`,
      WebkitTextStrokeColor: '#000000',
      left: `${box.value.left + box.value.width / 2}px`,
      width: `${box.value.width}px`,
      [spec.position === 'top' ? 'top' : 'bottom']: `${spec.marginV * scale}px`,
    },
  }
})

// --- eyedropper ------------------------------------------------------------

/** Natural pixel size of a preview media element, or null when not decoded. */
function naturalSize(media: HTMLImageElement | HTMLVideoElement): Size | null {
  const width = media instanceof HTMLVideoElement ? media.videoWidth : media.naturalWidth
  const height = media instanceof HTMLVideoElement ? media.videoHeight : media.naturalHeight
  return width > 0 && height > 0 ? { width, height } : null
}

/**
 * Turn a screen point into the media's own pixel coordinates.
 *
 * The preview element is stretched to the layer box and may carry a CSS
 * rotation about its centre, so the point is un-rotated first and then scaled
 * by the natural size. Returns null when the click landed outside the layer.
 */
function mediaPixel(
  media: HTMLImageElement | HTMLVideoElement,
  size: Size,
  rotation: number,
  clientX: number,
  clientY: number,
): { x: number; y: number } | null {
  const rect = media.getBoundingClientRect()
  if (rect.width <= 0 || rect.height <= 0) return null
  const centreX = rect.left + rect.width / 2
  const centreY = rect.top + rect.height / 2
  const radians = (-rotation * Math.PI) / 180
  const dx = clientX - centreX
  const dy = clientY - centreY
  // The bounding rect of a rotated element grew, so the un-rotated half sizes
  // come from the layer placement rather than from the rect itself.
  const local = {
    x: dx * Math.cos(radians) - dy * Math.sin(radians),
    y: dx * Math.sin(radians) + dy * Math.cos(radians),
  }
  const half = { x: media.offsetWidth / 2, y: media.offsetHeight / 2 }
  if (half.x <= 0 || half.y <= 0) return null
  if (Math.abs(local.x) > half.x || Math.abs(local.y) > half.y) return null
  return {
    x: Math.floor(clamp((local.x + half.x) / (half.x * 2), 0, 0.999) * size.width),
    y: Math.floor(clamp((local.y + half.y) / (half.y * 2), 0, 0.999) * size.height),
  }
}

/**
 * Sample one pixel for the chroma key.
 *
 * The key is applied to the OVERLAY layer, not to the base video, so the sample
 * is taken from the keyed layer's own media when the click lands on it, and
 * only falls back to the player frame otherwise. Both are served from this
 * origin, so the canvas stays untainted; a browser that still refuses is
 * reported instead of throwing.
 */
function sampleFrame(event: PointerEvent): void {
  const index = overlaysUi.eyedropper
  if (index === null) return

  const view = overlayViews.value.find((entry) => entry.index === index)
  const layer = overlayMedia.get(index)
  const layerSize = layer ? naturalSize(layer) : null
  let media: HTMLImageElement | HTMLVideoElement | null = null
  let size: Size | null = null
  let pixel: { x: number; y: number } | null = null

  if (layer && layerSize && view) {
    pixel = mediaPixel(layer, layerSize, view.rotation, event.clientX, event.clientY)
    if (pixel) {
      media = layer
      size = layerSize
    }
  }

  if (!pixel) {
    const player = root.value?.parentElement?.querySelector('video')
    const rect = root.value?.getBoundingClientRect()
    const playerSize = player instanceof HTMLVideoElement ? naturalSize(player) : null
    if (!player || !rect || !playerSize) {
      cancelEyedropper()
      toast('error', 'Кадр ещё не готов, попробуй ещё раз')
      return
    }
    const normalized = boxToNormalized(box.value, event.clientX - rect.left, event.clientY - rect.top)
    media = player
    size = playerSize
    pixel = {
      x: Math.floor(clamp(normalized.x, 0, 0.999) * playerSize.width),
      y: Math.floor(clamp(normalized.y, 0, 0.999) * playerSize.height),
    }
  }

  if (!media || !size) {
    cancelEyedropper()
    return
  }
  try {
    const canvas = document.createElement('canvas')
    canvas.width = size.width
    canvas.height = size.height
    const context = canvas.getContext('2d')
    if (!context) throw new Error('canvas 2d unavailable')
    context.drawImage(media, 0, 0)
    const [r, g, b] = context.getImageData(pixel.x, pixel.y, 1, 1).data
    const channels = [r, g, b].map((part) => part.toString(16).padStart(2, '0')).join('')
    const hex = `#${channels}`.toUpperCase()
    // `applyEyedropper` reads the armed index and disarms the eyedropper itself.
    applyEyedropper(hex)
    toast('success', `Цвет ключа: ${hex}`)
  } catch {
    cancelEyedropper()
    toast('error', 'Не удалось взять цвет из кадра')
  }
}

function onEyedropperKey(event: KeyboardEvent): void {
  if (event.key === 'Escape') cancelEyedropper()
}
</script>

<template>
  <Teleport v-if="teleportReady" to=".player-wrap" defer>
    <div ref="root" class="ovl-layer">
      <DragBox
        v-for="view in overlayViews"
        :key="`o-${view.index}`"
        :class="{ 'is-dimmed': view.dimmed }"
        :left="view.left"
        :top="view.top"
        :width="view.width"
        :height="view.height"
        :selected="isSelected('overlay', view.index)"
        resizable
        :label="view.label"
        @select="selectItem('overlay', view.index)"
        @transform="onOverlayTransform(view.index, $event)"
        @interaction-start="beginEditTransaction('overlay-drag')"
        @interaction-end="endEditTransaction"
      >
        <video
          v-if="view.isVideo && view.url"
          :ref="(element) => registerOverlayMedia(view.index, element)"
          class="ovl-media"
          :src="view.url"
          :style="{ opacity: view.opacity, transform: `rotate(${view.rotation}deg)` }"
          muted
          playsinline
        ></video>
        <img
          v-else-if="view.url"
          class="ovl-media"
          :src="view.url"
          alt=""
          :style="{ opacity: view.opacity, transform: `rotate(${view.rotation}deg)` }"
        />
      </DragBox>

      <DragBox
        v-for="view in titleViews"
        :key="`t-${view.index}`"
        :class="{ 'is-dimmed': view.dimmed }"
        :left="view.left"
        :top="view.top"
        :anchor-x="view.anchorX"
        :anchor-y="0.5"
        :selected="isSelected('title', view.index)"
        :label="view.label"
        @select="selectItem('title', view.index)"
        @transform="onTitleTransform(view.index, $event)"
        @interaction-start="beginEditTransaction('title-drag')"
        @interaction-end="endEditTransaction"
      >
        <span class="ovl-title" :style="{ ...view.style, ...view.boxStyle }">{{ view.text }}</span>
      </DragBox>

      <p v-if="currentCue" class="ovl-cue" :style="currentCue.style">{{ currentCue.text }}</p>

      <div
        v-if="overlaysUi.eyedropper !== null"
        class="ovl-eyedropper"
        role="button"
        tabindex="0"
        aria-label="Взять цвет ключа из кадра, Escape чтобы отменить"
        @pointerdown.prevent="sampleFrame"
        @keydown="onEyedropperKey"
      >
        <span>Кликни по кадру, чтобы взять цвет ключа. Esc – отмена.</span>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.ovl-layer {
  position: absolute;
  inset: 0;
  pointer-events: none;
  overflow: hidden;
  border-radius: var(--radius);
}

/* Selected but outside its own time window: editable, visibly not on air. */
.ovl-layer :deep(.is-dimmed) {
  opacity: 0.35;
}

.ovl-media {
  width: 100%;
  height: 100%;
  object-fit: fill;
  pointer-events: none;
}

.ovl-title {
  white-space: pre-wrap;
  line-height: 1.2;
  font-weight: 700;
  pointer-events: none;
}

.ovl-cue {
  position: absolute;
  margin: 0;
  transform: translateX(-50%);
  text-align: center;
  font-weight: 600;
  line-height: 1.25;
  white-space: pre-wrap;
  pointer-events: none;
}

.ovl-eyedropper {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 12px;
  background: rgba(0, 0, 0, 0.25);
  cursor: crosshair;
  pointer-events: auto;
}

.ovl-eyedropper span {
  padding: 6px 10px;
  border-radius: var(--radius);
  background: var(--panel);
  color: var(--text);
  font-size: 12px;
}
</style>
