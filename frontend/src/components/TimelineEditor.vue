<script setup lang="ts">
import { computed, ref } from 'vue'

import {
  executeTimelineCommand,
  redoTimeline,
  timelineState,
  undoTimeline,
} from '../store'
import type { ProjectClip, ProjectTrack } from '../project-schema'
import { applyTimelineCommand, projectFrameDurationTicks } from '../domain/timeline'
import type { TimelineCommand } from '../domain/timeline'

const dragClipId = ref<string | null>(null)
const dropMode = ref<'insert' | 'overwrite'>('insert')
const dropPreview = ref<{ trackId: string; tick: number } | null>(null)

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

function clipStyle(clip: ProjectClip) {
  return {
    left: `${(clip.timelineStartTick / totalTicks.value) * 100}%`,
    width: `${Math.max(1.2, (clip.durationTicks / totalTicks.value) * 100)}%`,
  }
}

function selectClip(clip: ProjectClip): void {
  timelineState.selectedClipId = clip.id
}

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
  const rectangle = lane.getBoundingClientRect()
  const ratio = Math.max(0, Math.min(1, (event.clientX - rectangle.left) / rectangle.width))
  const timelineStartTick = Math.round(ratio * totalTicks.value)
  const dragged = active.tracks.flatMap((item) => item.clips).find((clip) => clip.id === clipId)
  if (!dragged) return
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
  const rectangle = (event.currentTarget as HTMLElement).getBoundingClientRect()
  const ratio = Math.max(0, Math.min(1, (event.clientX - rectangle.left) / rectangle.width))
  dropPreview.value = { trackId: track.id, tick: Math.round(ratio * totalTicks.value) }
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
        <button class="btn ghost sm" :disabled="!selectedClip" @click="moveSelected(-1)">← кадр</button>
        <button class="btn ghost sm" :disabled="!selectedClip" @click="moveSelected(1)">кадр →</button>
      </div>
    </div>

    <div class="timeline-body">
      <div
        v-for="track in sequence.tracks"
        :key="track.id"
        class="timeline-track"
        :class="{ locked: track.locked, hidden: track.hidden }"
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
            {{ clip.id }}
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
    </div>
    <p v-if="timelineState.error" class="timeline-error" role="alert">{{ timelineState.error }}</p>
  </section>
</template>
