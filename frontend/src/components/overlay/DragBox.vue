<script setup lang="ts">
// One draggable, resizable, keyboard-nudgeable box on the live preview.
//
// The component is deliberately unit-free: it takes CSS pixels relative to the
// preview element and emits CSS pixel deltas. Converting those to and from the
// normalized wire coordinates is the parent's job, because only the parent
// knows the video content box (see domain/contentBox.ts).

import { computed, onUnmounted } from 'vue'
import type { BoxTransform } from './types'

const props = withDefaults(
  defineProps<{
    left: number
    top: number
    /** Null sizes the box to its own content, which is what a title needs. */
    width?: number | null
    height?: number | null
    /** Fraction of the box that sits left of / above the anchor point. */
    anchorX?: number
    anchorY?: number
    selected?: boolean
    resizable?: boolean
    label: string
  }>(),
  {
    width: null,
    height: null,
    anchorX: 0,
    anchorY: 0,
    selected: false,
    resizable: false,
  },
)

const emit = defineEmits<{
  select: []
  transform: [BoxTransform]
  'interaction-start': []
  'interaction-end': []
}>()

/** Keyboard nudge in CSS pixels; Shift makes it a coarse step. */
const NUDGE = 2
const NUDGE_COARSE = 20

type Corner = 'nw' | 'ne' | 'sw' | 'se'
type Mode = 'move' | Corner

let mode: Mode | null = null
let lastX = 0
let lastY = 0

const style = computed(() => ({
  left: `${props.left}px`,
  top: `${props.top}px`,
  width: props.width === null ? undefined : `${Math.max(0, props.width)}px`,
  height: props.height === null ? undefined : `${Math.max(0, props.height)}px`,
  transform: `translate(${-props.anchorX * 100}%, ${-props.anchorY * 100}%)`,
}))

/** Turn a pointer delta into the transform the given grab produces. */
function transformFor(grab: Mode, dx: number, dy: number): BoxTransform {
  switch (grab) {
    case 'move':
      return { dx, dy, dw: 0, dh: 0 }
    case 'se':
      return { dx: 0, dy: 0, dw: dx, dh: dy }
    case 'sw':
      return { dx, dy: 0, dw: -dx, dh: dy }
    case 'ne':
      return { dx: 0, dy, dw: dx, dh: -dy }
    case 'nw':
      return { dx, dy, dw: -dx, dh: -dy }
  }
}

function begin(grab: Mode, event: PointerEvent): void {
  emit('select')
  mode = grab
  lastX = event.clientX
  lastY = event.clientY
  emit('interaction-start')
  ;(event.target as HTMLElement).setPointerCapture?.(event.pointerId)
  window.addEventListener('pointermove', onMove)
  window.addEventListener('pointerup', stopDrag)
  window.addEventListener('pointercancel', stopDrag)
  event.preventDefault()
  event.stopPropagation()
}

// Deltas are incremental: the parent owns the value, clamps it and re-renders,
// so accumulating an absolute origin here would fight that clamping.
function onMove(event: PointerEvent): void {
  if (!mode) return
  const dx = event.clientX - lastX
  const dy = event.clientY - lastY
  lastX = event.clientX
  lastY = event.clientY
  if (dx === 0 && dy === 0) return
  emit('transform', transformFor(mode, dx, dy))
}

function stopDrag(): void {
  if (!mode) return
  mode = null
  window.removeEventListener('pointermove', onMove)
  window.removeEventListener('pointerup', stopDrag)
  window.removeEventListener('pointercancel', stopDrag)
  emit('interaction-end')
}

/** Arrows move the box; Alt plus arrows resize it when resizing is allowed. */
function onKeydown(event: KeyboardEvent): void {
  const step = event.shiftKey ? NUDGE_COARSE : NUDGE
  const axis: Record<string, [number, number]> = {
    ArrowLeft: [-step, 0],
    ArrowRight: [step, 0],
    ArrowUp: [0, -step],
    ArrowDown: [0, step],
  }
  const delta = axis[event.key]
  if (!delta) return
  event.preventDefault()
  emit('select')
  const resizing = event.altKey && props.resizable
  emit(
    'transform',
    resizing
      ? { dx: 0, dy: 0, dw: delta[0], dh: delta[1] }
      : { dx: delta[0], dy: delta[1], dw: 0, dh: 0 },
  )
}

onUnmounted(stopDrag)

const corners: Corner[] = ['nw', 'ne', 'sw', 'se']
</script>

<template>
  <div
    class="ovl-box"
    :class="{ 'is-selected': selected }"
    :style="style"
    role="button"
    tabindex="0"
    :aria-label="label"
    :title="label"
    @pointerdown="begin('move', $event)"
    @keydown="onKeydown"
    @focus="emit('select')"
  >
    <slot />
    <template v-if="resizable && selected">
      <span
        v-for="corner in corners"
        :key="corner"
        class="ovl-handle"
        :class="corner"
        @pointerdown="begin(corner, $event)"
      ></span>
    </template>
  </div>
</template>

<style scoped>
.ovl-box {
  position: absolute;
  box-sizing: border-box;
  /* Minimum comfortable pointer target even for a one-word title. */
  min-width: 24px;
  min-height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  outline: 1px dashed color-mix(in srgb, var(--accent) 55%, transparent);
  cursor: move;
  pointer-events: auto;
  touch-action: none;
}

.ovl-box.is-selected {
  outline: 1.5px solid var(--accent);
}

.ovl-box:focus-visible {
  outline: none;
  box-shadow: var(--ring);
}

.ovl-handle {
  position: absolute;
  width: 24px;
  height: 24px;
  border-radius: 50%;
  background: color-mix(in srgb, var(--accent) 22%, transparent);
  pointer-events: auto;
  touch-action: none;
}

.ovl-handle::after {
  content: '';
  position: absolute;
  inset: 7px;
  border-radius: 50%;
  background: #fff;
  border: 2px solid var(--accent);
}

.ovl-handle.nw {
  top: -12px;
  left: -12px;
  cursor: nwse-resize;
}

.ovl-handle.ne {
  top: -12px;
  right: -12px;
  cursor: nesw-resize;
}

.ovl-handle.sw {
  bottom: -12px;
  left: -12px;
  cursor: nesw-resize;
}

.ovl-handle.se {
  bottom: -12px;
  right: -12px;
  cursor: nwse-resize;
}
</style>
