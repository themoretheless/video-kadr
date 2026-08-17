<script setup lang="ts">
// One draggable clip on the video track. The block itself is focusable and
// carries every gesture as a keyboard command, so the pointer handles are pure
// mouse affordances and stay `aria-hidden`.
import { computed } from 'vue'
import { clipDuration, type TimelineSlot } from '../../store/composition'
import { bucketOf, cachedThumbnail } from './filmstrip'

/** Matches the tile width the filmstrip cache renders at. */
const TILE_WIDTH = 96
/** Below this the trim handles would overlap, so they are hidden. */
const HANDLES_MIN_WIDTH = 56

const props = defineProps<{
  entry: TimelineSlot
  total: number
  pxPerSecond: number
  bucketSeconds: number
  sourceKey: string
  selected: boolean
  /** Bumped by the parent when a new thumbnail lands in the cache. */
  version: number
}>()

const emit = defineEmits<{
  (event: 'grab', mode: 'move' | 'trim-start' | 'trim-end', pointer: PointerEvent): void
  (event: 'select'): void
  (event: 'command', command: string, fine: boolean): void
}>()

const widthPx = computed(() => Math.max(2, clipDuration(props.entry.clip) * props.pxPerSecond))
const leftPx = computed(() => props.entry.start * props.pxPerSecond)
const showHandles = computed(() => widthPx.value >= HANDLES_MIN_WIDTH)

function clock(seconds: number): string {
  const total = Math.max(0, seconds)
  const minutes = Math.floor(total / 60)
  return `${minutes}:${Math.floor(total % 60).toString().padStart(2, '0')}`
}

const tiles = computed(() => {
  // `version` is read so the list recomputes when the cache gains a tile.
  void props.version
  const speed = props.entry.clip.speed ?? 1
  const count = Math.min(64, Math.ceil(widthPx.value / TILE_WIDTH))
  const out: { x: number; url: string }[] = []
  for (let index = 0; index < count; index += 1) {
    const x = index * TILE_WIDTH
    const source = props.entry.clip.start + (x / props.pxPerSecond) * speed
    const url = cachedThumbnail(props.sourceKey, bucketOf(source, props.bucketSeconds))
    if (url) out.push({ x, url })
  }
  return out
})

const label = computed(() => {
  const clip = props.entry.clip
  const parts = [
    `Клип ${props.entry.index + 1} из ${props.total}`,
    `исходник с ${clock(clip.start)} по ${clock(clip.end)}`,
    `длительность ${clipDuration(clip).toFixed(1)} секунды`,
  ]
  if (clip.muted) parts.push('без звука')
  if (clip.transitionIn) parts.push(`переход ${clip.transitionIn.kind}`)
  return parts.join(', ')
})

/** Keyboard equivalents of every pointer gesture on the block. */
function onKey(event: KeyboardEvent): void {
  const fine = event.shiftKey
  const commands: Record<string, string> = {
    ArrowLeft: fine ? 'trim-start-back' : 'move-left',
    ArrowRight: fine ? 'trim-start-forward' : 'move-right',
    BracketLeft: 'trim-end-back',
    BracketRight: 'trim-end-forward',
    Delete: 'remove',
    Backspace: 'remove',
    Enter: 'seek',
  }
  const command = commands[event.code]
  if (!command) return
  event.preventDefault()
  event.stopPropagation()
  emit('command', command, fine)
}
</script>

<template>
  <div
    class="tl-clip"
    :class="{ 'is-selected': selected, 'is-muted': entry.clip.muted }"
    :style="{ left: `${leftPx}px`, width: `${widthPx}px` }"
    role="option"
    tabindex="0"
    :aria-selected="selected"
    :aria-label="label"
    @pointerdown="emit('grab', 'move', $event)"
    @focus="emit('select')"
    @keydown="onKey"
  >
    <img
      v-for="tile in tiles"
      :key="tile.x"
      class="tl-tile"
      :style="{ left: `${tile.x}px` }"
      :src="tile.url"
      alt=""
      draggable="false"
    />
    <span v-if="entry.clip.transitionIn" class="tl-xfade" aria-hidden="true">
      {{ entry.clip.transitionIn.kind }}
    </span>
    <span class="tl-clip-name" aria-hidden="true">{{ entry.index + 1 }}</span>
    <template v-if="showHandles">
      <span
        class="tl-handle tl-handle-l"
        aria-hidden="true"
        @pointerdown.stop="emit('grab', 'trim-start', $event)"
      ></span>
      <span
        class="tl-handle tl-handle-r"
        aria-hidden="true"
        @pointerdown.stop="emit('grab', 'trim-end', $event)"
      ></span>
    </template>
  </div>
</template>

<style scoped>
.tl-clip {
  position: absolute;
  top: 0;
  bottom: 0;
  min-width: 24px;
  overflow: hidden;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--panel-3);
  cursor: grab;
  touch-action: none;
  user-select: none;
}

.tl-clip.is-selected {
  border-color: var(--accent);
  box-shadow: var(--ring);
}

.tl-clip.is-muted {
  opacity: 0.72;
}

.tl-clip:focus-visible {
  outline: none;
  border-color: var(--accent);
  box-shadow: var(--ring);
}

.tl-tile {
  position: absolute;
  top: 0;
  height: 100%;
  width: 96px;
  object-fit: cover;
  pointer-events: none;
}

.tl-clip-name,
.tl-xfade {
  position: absolute;
  bottom: 2px;
  padding: 0 4px;
  border-radius: var(--radius-sm);
  background: rgba(0, 0, 0, 0.55);
  color: #fff;
  font-size: 11px;
  line-height: 16px;
}

.tl-clip-name {
  left: 2px;
}

.tl-xfade {
  right: 2px;
  background: var(--accent);
}

.tl-handle {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 24px;
  cursor: ew-resize;
  touch-action: none;
}

.tl-handle::before {
  content: '';
  position: absolute;
  top: 6px;
  bottom: 6px;
  width: 4px;
  border-radius: 2px;
  background: var(--accent);
}

.tl-handle-l {
  left: 0;
}

.tl-handle-l::before {
  left: 3px;
}

.tl-handle-r {
  right: 0;
}

.tl-handle-r::before {
  right: 3px;
}
</style>
