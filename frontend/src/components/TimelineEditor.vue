<script setup lang="ts">
import { computed, nextTick, ref } from 'vue'

import {
  clearTimelineGapSelection,
  closeAllTimelineGaps,
  closeSelectedTimelineGap,
  createTextLayerAtPlayhead,
  executeTimelineCommand,
  redoTimeline,
  removeSelectedClip,
  rippleDeleteSelectedClip,
  selectedClipTransitionContext,
  selectedClipTransition,
  selectedTimelineGap,
  setOpacityOnSelectedClip,
  selectTimelineGap,
  setTimelinePlayheadTick,
  setTransitionOnSelectedClip,
  splitSelectedClipAtPlayhead,
  state,
  timelineState,
  timelineTrackGaps,
  undoTimeline,
  updateTextLayerStyle,
  addTimelineMarkerAtPlayhead,
  clearTimelineMarkerSelection,
  jumpTimelineMarker,
  removeSelectedTimelineMarker,
  selectTimelineMarker,
  selectedTimelineMarker,
  timelineMarkers,
  updateSelectedTimelineMarker,
} from '../store'
import type { TextLayerStylePatch } from '../store'
import type { ProjectClip, ProjectTrack } from '../project-schema'
import type { ClipTransitionType } from '../domain/timeline'
import { applyTimelineCommand, CLIP_TRANSITION_TYPES, projectFrameDurationTicks } from '../domain/timeline'
import type { TimelineCommand, TimelineGap } from '../domain/timeline'
import TimelineClipMedia from './TimelineClipMedia.vue'

const dragClipId = ref<string | null>(null)
const dropMode = ref<'insert' | 'overwrite'>('insert')
const dropPreview = ref<{ trackId: string; tick: number } | null>(null)
const pxPerSecond = ref(0)
const snapEnabled = ref(true)
const timelineBody = ref<HTMLElement | null>(null)

const sequence = computed(() => {
  const document = timelineState.document
  return document?.sequences.find((item) => item.id === document.activeSequenceId) ?? null
})
const totalTicks = computed(() =>
  Math.max(
    1,
    ...(sequence.value?.tracks.flatMap((track) =>
      track.clips.map((clip) => clip.timelineStartTick + clip.durationTicks),
    ) ?? [1]),
  ),
)
const selectedClip = computed(() =>
  sequence.value?.tracks
    .flatMap((track) => track.clips)
    .find((clip) => clip.id === timelineState.selectedClipId),
)
const timeBase = computed(() => sequence.value?.settings.timeBase ?? 1_000_000)
const selectedGap = computed(() => selectedTimelineGap())

function gapStyle(gap: TimelineGap) {
  const widthTicks = gap.endTick - gap.startTick
  if (laneGridStyle.value) {
    return {
      left: `${(gap.startTick / timeBase.value) * pxPerSecond.value}px`,
      width: `${Math.max(8, (widthTicks / timeBase.value) * pxPerSecond.value)}px`,
    }
  }
  return {
    left: `${(gap.startTick / totalTicks.value) * 100}%`,
    width: `${Math.max(1.2, (widthTicks / totalTicks.value) * 100)}%`,
  }
}
const selectedTextMedia = computed(() => {
  const document = timelineState.document
  const clip = selectedClip.value
  if (!document || !clip) return null
  const media = document.media.find((item) => item.id === clip.mediaId)
  return media && media.kind === 'text' ? media : null
})

function clipMediaKind(clip: ProjectClip): string | null {
  const media = timelineState.document?.media.find((item) => item.id === clip.mediaId)
  return media?.kind ?? null
}

function clipLabel(clip: ProjectClip): string {
  const media = timelineState.document?.media.find((item) => item.id === clip.mediaId)
  if (media?.kind === 'text' && typeof media.metadata.text === 'string') {
    return media.metadata.text.length > 24 ? `${media.metadata.text.slice(0, 23)}…` : media.metadata.text
  }
  return clip.id
}

function updateTextStyle(patch: TextLayerStylePatch): void {
  if (selectedTextMedia.value) updateTextLayerStyle(selectedTextMedia.value.id, patch)
}
const transitionContext = computed(() => selectedClipTransitionContext())
const currentTransition = computed(() => selectedClipTransition())
const transitionSeconds = computed(() => {
  const transition = currentTransition.value
  const context = transitionContext.value
  if (!transition || !context) return 0.5
  return Math.round((transition.durationTicks / context.timeBase) * 10) / 10
})
const transitionMaxSeconds = computed(() => {
  const context = transitionContext.value
  if (!context) return 2
  return Math.max(0.1, Math.floor((context.maxDurationTicks / context.timeBase) * 10) / 10)
})
function applyTransition(type: string, seconds: number): void {
  const context = transitionContext.value
  if (!context) return
  if (type === 'none') {
    setTransitionOnSelectedClip(null)
    return
  }
  const candidate = type as ClipTransitionType
  if (!CLIP_TRANSITION_TYPES.includes(candidate)) return
  const bounded = Math.max(0.1, Math.min(seconds, transitionMaxSeconds.value))
  const durationTicks = Math.max(1, Math.min(context.maxDurationTicks, Math.round(bounded * context.timeBase)))
  setTransitionOnSelectedClip({ type: candidate, durationTicks })
}
const totalSeconds = computed(() => totalTicks.value / timeBase.value)
const laneWidthPx = computed(() =>
  pxPerSecond.value > 0 ? pxPerSecond.value * totalSeconds.value : 0,
)
const laneGridStyle = computed(() =>
  laneWidthPx.value >= 700
    ? { gridTemplateColumns: `170px ${Math.round(laneWidthPx.value)}px` }
    : undefined,
)

function clipStyle(clip: ProjectClip) {
  if (laneGridStyle.value) {
    return {
      left: `${(clip.timelineStartTick / timeBase.value) * pxPerSecond.value}px`,
      width: `${Math.max(8, (clip.durationTicks / timeBase.value) * pxPerSecond.value)}px`,
    }
  }
  return {
    left: `${(clip.timelineStartTick / totalTicks.value) * 100}%`,
    width: `${Math.max(1.2, (clip.durationTicks / totalTicks.value) * 100)}%`,
  }
}

function tickAtClientX(clientX: number, lane: HTMLElement): number {
  const rectangle = lane.getBoundingClientRect()
  if (rectangle.width <= 0) return 0
  const ratio = Math.max(0, Math.min(1, (clientX - rectangle.left) / rectangle.width))
  return Math.round(ratio * totalTicks.value)
}

function draggedClipForDrop(): ProjectClip | null {
  const clipId = dragClipId.value
  if (!clipId) return null
  return sequence.value?.tracks.flatMap((track) => track.clips).find((clip) => clip.id === clipId) ?? null
}

// Magnet snap: align the dragged clip's start and end edges with the
// playhead, zero, and every other clip edge within an 8-pixel radius.
function snapTick(rawTick: number, clip: ProjectClip | null, altKey: boolean): number {
  if (!snapEnabled.value || altKey) return rawTick
  const lane = rulerLane.value
  if (!lane || clip && clip.durationTicks <= 0) return rawTick
  const rectangle = lane.getBoundingClientRect()
  if (rectangle.width <= 0) return rawTick
  const threshold = (8 / rectangle.width) * totalTicks.value
  const targets = new Set<number>([0, timelineState.playheadTick])
  for (const track of sequence.value?.tracks ?? []) {
    for (const item of track.clips) {
      if (clip && item.id === clip.id) continue
      targets.add(item.timelineStartTick)
      targets.add(item.timelineStartTick + item.durationTicks)
    }
  }
  const edges = clip ? [0, clip.durationTicks] : [0]
  let best = rawTick
  let bestDistance = threshold
  for (const target of targets) {
    for (const edge of edges) {
      const candidate = target - edge
      if (candidate < 0) continue
      const distance = Math.abs(candidate - rawTick)
      if (distance < bestDistance) {
        bestDistance = distance
        best = candidate
      }
    }
  }
  return best
}

function applyClipOpacity(percent: number): void {
  const bounded = Math.max(0, Math.min(100, Math.round(percent)))
  setOpacityOnSelectedClip(bounded >= 100 ? null : bounded / 100)
}

function selectClip(clip: ProjectClip): void {
  timelineState.selectedClipId = clip.id
  clearTimelineGapSelection()
  clearTimelineMarkerSelection()
}

const markers = computed(() => timelineMarkers())
const selectedMarker = computed(() => selectedTimelineMarker())

function pickMarker(markerId: string, markerTick: number): void {
  selectTimelineMarker(markerId)
  setTimelinePlayheadTick(markerTick)
}

function applyMarkerColor(event: Event): void {
  updateSelectedTimelineMarker({ color: (event.target as HTMLInputElement).value })
}

function applyMarkerLabel(event: Event): void {
  const value = (event.target as HTMLInputElement).value.trim()
  updateSelectedTimelineMarker({ label: value === '' ? null : value })
}

function removeMarker(): void {
  removeSelectedTimelineMarker()
}

interface ClipMediaSource {
  url: string
  kind: 'video' | 'audio' | 'image'
  durationSeconds: number | null
}

const mediaSources = computed(() => {
  const map = new Map<string, ClipMediaSource>()
  const document = timelineState.document
  if (!document) return map
  for (const media of document.media) {
    const assetKey = media.assetRef ?? media.id
    const video = state.video
    const matchesVideo = video !== null
      && (video.id === media.id || video.id === assetKey || (video.assetId ?? '') === assetKey)
    if (matchesVideo && video) {
      map.set(media.id, {
        url: video.url,
        kind: media.kind === 'audio' ? 'audio' : media.kind === 'image' ? 'image' : 'video',
        durationSeconds: typeof media.metadata.duration === 'number' ? media.metadata.duration : video.duration,
      })
      continue
    }
    const entry = state.library.find((candidate) => candidate.kind === 'source'
      && (candidate.id === media.id || (candidate.assetId ?? candidate.id) === assetKey))
    if (entry?.url) {
      map.set(media.id, {
        url: entry.url,
        kind: media.kind === 'audio' ? 'audio' : media.kind === 'image' ? 'image' : 'video',
        durationSeconds: typeof media.metadata.duration === 'number' ? media.metadata.duration : entry.duration ?? null,
      })
    }
  }
  return map
})

function beginDrag(event: DragEvent, clip: ProjectClip): void {
  dragClipId.value = clip.id
  event.dataTransfer?.setData('text/plain', clip.id)
  if (event.dataTransfer) event.dataTransfer.effectAllowed = 'move'
}

function dropOnTrack(event: DragEvent, track: ProjectTrack): void {
  event.preventDefault()
  const clipId = dragClipId.value || event.dataTransfer?.getData('text/plain')
  const document = timelineState.document
  const active = sequence.value
  const lane = event.currentTarget as HTMLElement
  if (!clipId || !document || !active || !lane) return
  const dragged = active.tracks.flatMap((item) => item.clips).find((clip) => clip.id === clipId)
  if (!dragged) return
  const timelineStartTick = snapTick(
    tickAtClientX(event.clientX, lane),
    dragged,
    event.altKey,
  )
  const sourceTrack = active.tracks.find((item) => item.clips.some((clip) => clip.id === clipId))
  if (!sourceTrack) return
  const commands: TimelineCommand[] = [{
    kind: 'remove_clip',
    sequenceId: active.id,
    trackId: sourceTrack.id,
    clipId,
  }]
  let preview = applyTimelineCommand(document, commands[0])
  let previewSequence = preview.sequences.find((item) => item.id === active.id)!
  let previewTrack = previewSequence.tracks.find((item) => item.id === track.id)!
  const collisions = previewTrack.clips.filter(
    (clip) =>
      clip.timelineStartTick < timelineStartTick + dragged.durationTicks &&
      timelineStartTick < clip.timelineStartTick + clip.durationTicks,
  )
  if (dropMode.value === 'overwrite') {
    for (const collision of collisions) {
      const command: TimelineCommand = {
        kind: 'remove_clip', sequenceId: active.id, trackId: track.id, clipId: collision.id,
      }
      commands.push(command)
      preview = applyTimelineCommand(preview, command)
    }
  } else {
    const downstream = previewTrack.clips
      .filter((clip) => clip.timelineStartTick >= timelineStartTick)
      .sort((left, right) => right.timelineStartTick - left.timelineStartTick)
    for (const clip of downstream) {
      previewSequence = preview.sequences.find((item) => item.id === active.id)!
      previewTrack = previewSequence.tracks.find((item) => item.id === track.id)!
      const command: TimelineCommand = {
        kind: 'move_clip',
        sequenceId: active.id,
        clipId: clip.id,
        targetTrackId: track.id,
        targetIndex: previewTrack.clips.findIndex((item) => item.id === clip.id),
        timelineStartTick: clip.timelineStartTick + dragged.durationTicks,
      }
      commands.push(command)
      preview = applyTimelineCommand(preview, command)
    }
  }
  previewSequence = preview.sequences.find((item) => item.id === active.id)!
  previewTrack = previewSequence.tracks.find((item) => item.id === track.id)!
  const targetIndex =
    previewTrack.clips.filter((clip) => clip.timelineStartTick <= timelineStartTick).length
  const inserted = { ...structuredClone(dragged), timelineStartTick }
  commands.push({
    kind: 'insert_clip',
    sequenceId: active.id,
    trackId: track.id,
    index: targetIndex,
    clip: inserted,
  })
  executeTimelineCommand({
    kind: 'batch',
    commands,
  })
  dragClipId.value = null
  dropPreview.value = null
}

function previewDrop(event: DragEvent, track: ProjectTrack): void {
  event.preventDefault()
  const lane = event.currentTarget as HTMLElement
  if (!lane) return
  dropPreview.value = {
    trackId: track.id,
    tick: snapTick(tickAtClientX(event.clientX, lane), draggedClipForDrop(), event.altKey),
  }
}

function setTrackState(track: ProjectTrack, key: 'muted' | 'solo' | 'locked' | 'hidden'): void {
  const active = sequence.value
  if (!active) return
  executeTimelineCommand({
    kind: 'set_track_state',
    sequenceId: active.id,
    trackId: track.id,
    patch: { [key]: track[key] !== true },
  })
}

function trimSelected(edge: 'in' | 'out', seconds: number): void {
  const active = sequence.value
  const clip = selectedClip.value
  if (!active || !clip || !Number.isFinite(seconds)) return
  const tick = Math.round(seconds * timeBase.value)
  const sourceInTick = edge === 'in' ? tick : clip.sourceInTick
  const sourceOutTick = edge === 'out' ? tick : clip.sourceOutTick
  const timelineStartTick =
    edge === 'in'
      ? clip.timelineStartTick + (sourceInTick - clip.sourceInTick)
      : clip.timelineStartTick
  executeTimelineCommand(
    {
      kind: 'trim_clip',
      sequenceId: active.id,
      clipId: clip.id,
      sourceInTick,
      sourceOutTick,
      timelineStartTick,
    },
    `trim:${clip.id}:${edge}`,
  )
}

function moveSelected(frameDelta: number): void {
  const active = sequence.value
  const clip = selectedClip.value
  if (!active || !clip) return
  const track = active.tracks.find((item) => item.clips.some((candidate) => candidate.id === clip.id))
  if (!track) return
  const index = track.clips.findIndex((candidate) => candidate.id === clip.id)
  executeTimelineCommand({
    kind: 'move_clip',
    sequenceId: active.id,
    clipId: clip.id,
    targetTrackId: track.id,
    targetIndex: index,
    timelineStartTick: Math.max(
      0,
      clip.timelineStartTick + frameDelta * projectFrameDurationTicks(active.settings),
    ),
  })
}

const rulerLane = ref<HTMLElement | null>(null)
const scrubbing = ref(false)

const frameTicks = computed(() =>
  sequence.value ? projectFrameDurationTicks(sequence.value.settings) : timeBase.value / 30,
)
const playheadRatio = computed(() =>
  Math.min(1, Math.max(0, timelineState.playheadTick / totalTicks.value)),
)
const playheadTimecode = computed(() => {
  const fps = Math.round(sequence.value?.settings.frameRate ?? 30)
  const totalFrames = Math.round(timelineState.playheadTick / frameTicks.value)
  const frames = totalFrames % fps
  const totalSeconds = Math.floor(totalFrames / fps)
  const pad = (value: number) => String(value).padStart(2, '0')
  return `${pad(Math.floor(totalSeconds / 3600))}:${pad(Math.floor(totalSeconds / 60) % 60)}:${pad(totalSeconds % 60)}:${pad(frames)}`
})
const rulerMarks = computed(() => {
  const seconds = totalTicks.value / timeBase.value
  const steps = [0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800, 3600]
  const step = steps.find((candidate) => seconds / candidate <= 10) ?? 3600
  const marks: { tick: number; label: string }[] = []
  for (let index = 0; index * step * timeBase.value <= totalTicks.value; index += 1) {
    const value = index * step
    const rounded = Math.round(value)
    const label = step < 1
      ? value.toFixed(1)
      : `${Math.floor(rounded / 60)}:${String(rounded % 60).padStart(2, '0')}`
    marks.push({ tick: Math.round(value * timeBase.value), label })
    if (marks.length > 40) break
  }
  return marks
})

function tickFromClientX(clientX: number): number {
  const lane = rulerLane.value
  if (!lane) return timelineState.playheadTick
  const rectangle = lane.getBoundingClientRect()
  const ratio = Math.max(0, Math.min(1, (clientX - rectangle.left) / rectangle.width))
  return Math.round(ratio * totalTicks.value)
}

function beginScrub(event: PointerEvent): void {
  scrubbing.value = true
  ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  setTimelinePlayheadTick(tickFromClientX(event.clientX))
}

function moveScrub(event: PointerEvent): void {
  if (!scrubbing.value) return
  setTimelinePlayheadTick(tickFromClientX(event.clientX))
}

function endScrub(): void {
  scrubbing.value = false
}

function rulerKey(event: KeyboardEvent): void {
  const frames = event.shiftKey ? 5 : 1
  if (event.key === 'ArrowLeft') {
    event.stopPropagation()
    event.preventDefault()
    setTimelinePlayheadTick(timelineState.playheadTick - frames * frameTicks.value)
  } else if (event.key === 'ArrowRight') {
    event.stopPropagation()
    event.preventDefault()
    setTimelinePlayheadTick(timelineState.playheadTick + frames * frameTicks.value)
  } else if (event.key === 'Home') {
    event.stopPropagation()
    event.preventDefault()
    setTimelinePlayheadTick(0)
  }
}

async function zoomBy(factor: number, clientX: number | null): Promise<void> {
  const lane = rulerLane.value
  const body = timelineBody.value
  if (!lane || !body) return
  const rectangle = lane.getBoundingClientRect()
  if (rectangle.width <= 0) return
  const currentPxPerSecond = pxPerSecond.value > 0
    ? pxPerSecond.value
    : rectangle.width / totalSeconds.value
  const next = Math.min(20_000, Math.max(700 / totalSeconds.value, currentPxPerSecond * factor))
  const anchorTick = clientX === null ? null : tickAtClientX(clientX, lane)
  pxPerSecond.value = Math.round(next * 100) / 100
  await nextTick()
  if (anchorTick !== null && clientX !== null) {
    const driftedTick = tickAtClientX(clientX, lane)
    body.scrollLeft += ((driftedTick - anchorTick) / totalTicks.value) * lane.getBoundingClientRect().width
  }
}

function zoomFit(): void {
  pxPerSecond.value = 0
}

function onWheel(event: WheelEvent): void {
  if (!event.ctrlKey && !event.metaKey) return
  event.preventDefault()
  void zoomBy(Math.exp(-event.deltaY * 0.0012), event.clientX)
}
</script>

<template>
  <section v-if="sequence" class="card timeline-editor" aria-label="Временная шкала проекта">
    <div class="timeline-toolbar">
      <div>
        <h2>Timeline</h2>
        <small>{{ sequence.tracks.length }} дорожек · {{ sequence.tracks.flatMap(track => track.clips).length }} клипов</small>
      </div>
      <div class="timeline-actions">
        <label class="timeline-mode">
          Drop
          <select v-model="dropMode" aria-label="Режим вставки клипа">
            <option value="insert">Insert</option>
            <option value="overwrite">Overwrite</option>
          </select>
        </label>
        <button class="btn ghost sm" :disabled="!timelineState.canUndo" @click="undoTimeline">↶ Undo</button>
        <button class="btn ghost sm" :disabled="!timelineState.canRedo" @click="redoTimeline">↷ Redo</button>
        <button class="btn ghost sm" title="Добавить текстовый слой на playhead" @click="createTextLayerAtPlayhead">T Текст</button>
        <button class="btn ghost sm" :disabled="!selectedClip" title="Разделить клип на playhead (S)" @click="splitSelectedClipAtPlayhead">✂ Разделить</button>
        <button class="btn ghost sm" :disabled="!selectedClip" title="Удалить с подтяжкой (Delete)" @click="rippleDeleteSelectedClip">⌫ Ripple</button>
        <button class="btn ghost sm" :disabled="!selectedClip" title="Удалить клип (Shift+Delete)" @click="removeSelectedClip">✕ Удалить</button>
        <button class="btn ghost sm" :disabled="!selectedGap" title="Закрыть выбранный пропуск (Delete)" @click="closeSelectedTimelineGap">⇤ Закрыть пропуск</button>
        <button class="btn ghost sm" :disabled="!selectedGap" title="Убрать все пропуски на дорожке" @click="closeAllTimelineGaps">⇤⇤ Все пропуски</button>
        <button class="btn ghost sm" title="Добавить маркер на playhead (M)" @click="addTimelineMarkerAtPlayhead">⚑ Маркер</button>
        <button class="btn ghost sm" :disabled="markers.length === 0" title="Предыдущий маркер" @click="jumpTimelineMarker(-1)">⏮ маркер</button>
        <button class="btn ghost sm" :disabled="markers.length === 0" title="Следующий маркер" @click="jumpTimelineMarker(1)">маркер ⏭</button>
        <button class="btn ghost sm" :disabled="!selectedClip" @click="moveSelected(-1)">← кадр</button>
        <button class="btn ghost sm" :disabled="!selectedClip" @click="moveSelected(1)">кадр →</button>
        <button
          class="btn ghost sm"
          :aria-pressed="snapEnabled"
          :class="{ active: snapEnabled }"
          title="Примагничивание к краям клипов и playhead (Alt отключает)"
          @click="snapEnabled = !snapEnabled"
        >🧲</button>
        <label class="timeline-zoom">
          Zoom
          <span class="timeline-zoom-controls">
            <button class="btn ghost sm" title="Отдалить" aria-label="Отдалить шкалу" @click="zoomBy(1 / 1.5, null)">−</button>
            <button class="btn ghost sm" title="Приблизить" aria-label="Приблизить шкалу" @click="zoomBy(1.5, null)">＋</button>
            <button class="btn ghost sm" title="Вписать проект" @click="zoomFit">⤢</button>
          </span>
        </label>
      </div>
    </div>

    <div v-if="selectedMarker" class="timeline-marker-controls">
      <label>
        Цвет
        <input
          type="color"
          :value="selectedMarker.color ?? '#ffd32a'"
          aria-label="Цвет маркера"
          @change="applyMarkerColor"
        >
      </label>
      <label>
        Метка
        <input
          type="text"
          maxlength="200"
          :value="selectedMarker.label ?? ''"
          aria-label="Текст маркера"
          @change="applyMarkerLabel"
        >
      </label>
      <span>{{ (selectedMarker.timelineTick / timeBase).toFixed(3) }} с</span>
      <button class="btn ghost sm" title="Удалить маркер (Delete)" @click="removeMarker">✕ Удалить маркер</button>
    </div>

    <div ref="timelineBody" class="timeline-body" @wheel="onWheel">
      <div class="timeline-ruler" role="group" aria-label="Линейка времени" :style="laneGridStyle">
        <div class="timeline-ruler-label">
          <span class="timeline-timecode">{{ playheadTimecode }}</span>
        </div>
        <div
          ref="rulerLane"
          class="timeline-ruler-lane"
          @pointerdown="beginScrub"
          @pointermove="moveScrub"
          @pointerup="endScrub"
          @pointercancel="endScrub"
          @keydown="rulerKey"
          tabindex="0"
          aria-label="Playhead, перетаскивание или стрелки для перемотки"
        >
          <span
            v-for="mark in rulerMarks"
            :key="mark.tick"
            class="timeline-ruler-mark"
            :style="{ left: `${(mark.tick / totalTicks) * 100}%` }"
          >{{ mark.label }}</span>
          <button
            v-for="marker in markers"
            :key="marker.id"
            class="timeline-marker"
            :class="{ selected: timelineState.selectedMarkerId === marker.id }"
            :style="{ left: `${Math.min(100, (marker.timelineTick / totalTicks) * 100)}%`, '--marker-color': marker.color ?? '#ffd32a' }"
            :title="`${(marker.timelineTick / timeBase).toFixed(2)} с${marker.label ? ` · ${marker.label}` : ''}`"
            @click.stop="pickMarker(marker.id, marker.timelineTick)"
          >⚑</button>
          <span
            class="timeline-ruler-head"
            :class="{ scrubbing }"
            :style="{ left: `${playheadRatio * 100}%` }"
            aria-hidden="true"
          />
        </div>
      </div>
      <div
        v-for="track in sequence.tracks"
        :key="track.id"
        class="timeline-track"
        :class="{ locked: track.locked, hidden: track.hidden }"
        :style="laneGridStyle"
      >
        <div class="track-header">
          <span :title="track.id">{{ track.name }}</span>
          <div class="track-controls" :aria-label="`Управление ${track.name}`">
            <button :aria-pressed="track.muted === true" title="Mute" @click="setTrackState(track, 'muted')">M</button>
            <button :aria-pressed="track.solo === true" title="Solo" @click="setTrackState(track, 'solo')">S</button>
            <button :aria-pressed="track.locked === true" title="Lock" @click="setTrackState(track, 'locked')">🔒</button>
            <button :aria-pressed="track.hidden === true" title="Hide" @click="setTrackState(track, 'hidden')">◉</button>
          </div>
        </div>
        <div
          class="track-lane"
          @dragover="previewDrop($event, track)"
          @dragleave="dropPreview = null"
          @drop="dropOnTrack($event, track)"
        >
          <span
            v-if="dropPreview?.trackId === track.id"
            class="timeline-drop-indicator"
            :style="{ left: `${(dropPreview.tick / totalTicks) * 100}%` }"
            aria-hidden="true"
          />
          <span
            class="timeline-playhead-line"
            :style="{ left: `${playheadRatio * 100}%` }"
            aria-hidden="true"
          />
          <button
            v-for="gap in timelineTrackGaps(track.id)"
            :key="`gap-${String(gap.startTick)}`"
            class="timeline-gap"
            :class="{ selected: selectedGap && selectedGap.trackId === track.id && selectedGap.gap.startTick === gap.startTick }"
            :style="gapStyle(gap)"
            :disabled="track.locked === true"
            :title="`Пропуск ${((gap.endTick - gap.startTick) / timeBase).toFixed(2)} с`"
            :aria-label="`Пропуск на ${track.name}`"
            @click="selectTimelineGap(track.id, gap.startTick)"
          >
            <span class="timeline-gap-label">␣</span>
          </button>
          <button
            v-for="clip in track.clips"
            :key="clip.id"
            class="timeline-clip"
            :class="{ selected: clip.id === timelineState.selectedClipId }"
            :style="clipStyle(clip)"
            :draggable="track.locked !== true"
            :aria-label="`Клип ${clip.id}`"
            @click="selectClip(clip)"
            @dragstart="beginDrag($event, clip)"
          >
            <TimelineClipMedia
              v-if="mediaSources.get(clip.mediaId) && clipMediaKind(clip) !== 'text'"
              :url="mediaSources.get(clip.mediaId)!.url"
              :kind="mediaSources.get(clip.mediaId)!.kind"
              :media-duration-seconds="mediaSources.get(clip.mediaId)!.durationSeconds"
              :source-in-tick="clip.sourceInTick"
              :duration-ticks="clip.durationTicks"
              :time-base="timeBase"
              :opacity="clip.opacity ?? 1"
            />
            <span class="timeline-clip-label">{{ clipLabel(clip) }}</span>
          </button>
        </div>
      </div>
    </div>

    <div v-if="selectedClip" class="timeline-trim-controls">
      <label>
        Source in
        <input
          type="number"
          min="0"
          step="0.001"
          :value="selectedClip.sourceInTick / timeBase"
          @change="trimSelected('in', Number(($event.target as HTMLInputElement).value))"
        >
      </label>
      <label>
        Source out
        <input
          type="number"
          min="0.001"
          step="0.001"
          :value="selectedClip.sourceOutTick / timeBase"
          @change="trimSelected('out', Number(($event.target as HTMLInputElement).value))"
        >
      </label>
      <span>Длительность: {{ (selectedClip.durationTicks / timeBase).toFixed(3) }} с</span>
      <label>
        Прозрачность
        <input
          type="range"
          min="0"
          max="100"
          step="1"
          :value="String(Math.round((selectedClip.opacity ?? 1) * 100))"
          @input="applyClipOpacity(Number(($event.target as HTMLInputElement).value))"
        >
      </label>
      <span>{{ Math.round((selectedClip.opacity ?? 1) * 100) }}%</span>
    </div>
    <div v-if="selectedTextMedia" class="timeline-text-controls">
      <label>
        Текст
        <input
          type="text"
          :value="String(selectedTextMedia.metadata.text ?? '')"
          @change="updateTextStyle({ text: ($event.target as HTMLInputElement).value })"
        >
      </label>
      <label>
        Размер
        <input
          type="range"
          min="0.02"
          max="0.3"
          step="0.01"
          :value="Number(selectedTextMedia.metadata.fontSizeRatio ?? 0.08)"
          @input="updateTextStyle({ fontSizeRatio: Number(($event.target as HTMLInputElement).value) })"
        >
      </label>
      <label>
        Цвет
        <input
          type="color"
          :value="String(selectedTextMedia.metadata.color ?? '#ffffff')"
          @input="updateTextStyle({ color: ($event.target as HTMLInputElement).value })"
        >
      </label>
      <label>
        X
        <input
          type="range"
          min="0"
          max="1"
          step="0.01"
          :value="Number(selectedTextMedia.metadata.xRatio ?? 0.5)"
          @input="updateTextStyle({ xRatio: Number(($event.target as HTMLInputElement).value) })"
        >
      </label>
      <label>
        Y
        <input
          type="range"
          min="0"
          max="1"
          step="0.01"
          :value="Number(selectedTextMedia.metadata.yRatio ?? 0.85)"
          @input="updateTextStyle({ yRatio: Number(($event.target as HTMLInputElement).value) })"
        >
      </label>
      <label>
        Прозрачность
        <input
          type="range"
          min="0"
          max="1"
          step="0.05"
          :value="Number(selectedTextMedia.metadata.opacity ?? 1)"
          @input="updateTextStyle({ opacity: Number(($event.target as HTMLInputElement).value) })"
        >
      </label>
    </div>
    <div v-if="transitionContext" class="timeline-text-controls">
      <label>
        Переход
        <select
          :value="currentTransition?.type ?? 'none'"
          @change="applyTransition(($event.target as HTMLSelectElement).value, transitionSeconds)"
        >
          <option value="none">Без перехода</option>
          <option value="crossfade">Кроссфейд</option>
          <option value="fade-black">Через чёрный</option>
          <optgroup label="Стирание">
            <option value="wipe-left">Стирание влево</option>
            <option value="wipe-right">Стирание вправо</option>
            <option value="wipe-up">Стирание вверх</option>
            <option value="wipe-down">Стирание вниз</option>
          </optgroup>
          <optgroup label="Сдвиг">
            <option value="slide-left">Сдвиг влево</option>
            <option value="slide-right">Сдвиг вправо</option>
            <option value="slide-up">Сдвиг вверх</option>
            <option value="slide-down">Сдвиг вниз</option>
          </optgroup>
          <optgroup label="Круг">
            <option value="circle-open">Раскрытие круга</option>
            <option value="circle-close">Закрытие круга</option>
          </optgroup>
        </select>
      </label>
      <label v-if="currentTransition">
        Длительность
        <input
          type="range"
          min="0.1"
          :max="String(transitionMaxSeconds)"
          step="0.1"
          :value="transitionSeconds"
          @input="applyTransition(currentTransition?.type ?? 'crossfade', Number(($event.target as HTMLInputElement).value))"
        >
      </label>
      <span v-if="currentTransition">{{ transitionSeconds.toFixed(1) }} с</span>
    </div>
    <p v-if="timelineState.error" class="timeline-error" role="alert">{{ timelineState.error }}</p>
  </section>
</template>
