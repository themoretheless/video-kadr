import {
  MAX_COMPOSITION_MARKERS,
  upsertCompositionMarker,
  replaceCompositionMarkersByOrigin,
  deleteCompositionMarker,
  type CompositionMarker
} from '../../composition/markers.js'
import {
  MAX_COMPOSITION_DURATION_TICKS
} from '../../composition/types.js'
import type {
  TimelineBeat
} from '../../audio/beatMarkers.js'
import {
  autoBeatMarkerId,
  commitDocument,
  compositionMarkerList,
  compositionState,
  makeId
} from './core.svelte.js'

export function addCompositionMarkerAtPlayhead(label?: string): string {
  const markers = compositionMarkerList()
  const id = makeId('marker')
  const palette = ['#f59e0b', '#22c55e', '#38bdf8', '#a78bfa', '#fb7185'] as const
  const marker: CompositionMarker = {
    id,
    tick: compositionState.transport.playheadTicks,
    label: label?.trim() || `Маркер ${markers.length + 1}`,
    color: palette[markers.length % palette.length],
  }
  commitDocument(upsertCompositionMarker(compositionState.document, marker))
  compositionState.ui.selectedMarkerId = id
  return id
}

/** Replace the generated Auto Beat marker family as one history/autosave edit. */

export function replaceCompositionAutoBeatMarkers(
  beats: readonly TimelineBeat[],
  estimatedBpm: number | null,
): number {
  if (estimatedBpm !== null && (!Number.isFinite(estimatedBpm) || estimatedBpm <= 0 || estimatedBpm > 1_000)) {
    throw new Error('Auto Beat BPM должен быть положительным числом')
  }
  const uniqueByTick: TimelineBeat[] = []
  for (const beat of beats) {
    if (!Number.isSafeInteger(beat.tick) || beat.tick < 0 || beat.tick > MAX_COMPOSITION_DURATION_TICKS) {
      throw new Error('Auto Beat marker tick выходит за пределы композиции')
    }
    if (!Number.isFinite(beat.strength) || beat.strength < 0) {
      throw new Error('Auto Beat marker strength должна быть неотрицательным числом')
    }
    const existingIndex = uniqueByTick.findIndex((candidate) => candidate.tick === beat.tick)
    if (existingIndex === -1) uniqueByTick.push({ ...beat })
    else if (beat.strength > uniqueByTick[existingIndex]!.strength) uniqueByTick[existingIndex] = { ...beat }
  }
  const normalized = uniqueByTick.sort((left, right) => left.tick - right.tick)
  const manualIds = compositionMarkerList()
    .filter((marker) => marker.origin !== 'auto_beat')
    .map((marker) => marker.id)
  if (manualIds.length + normalized.length > MAX_COMPOSITION_MARKERS) {
    throw new Error(`Marker limit is ${MAX_COMPOSITION_MARKERS}`)
  }
  const bpmLabel = estimatedBpm === null ? '' : ` · ≈ ${Number(estimatedBpm.toFixed(1))} BPM`
  const markers = normalized.map((beat, index): CompositionMarker => ({
    id: autoBeatMarkerId(beat.tick, manualIds),
    tick: beat.tick,
    label: `Auto Beat ${index + 1}${bpmLabel}`,
    color: '#f97316',
    origin: 'auto_beat',
  }))
  commitDocument(replaceCompositionMarkersByOrigin(compositionState.document, 'auto_beat', markers))
  return markers.length
}


export function updateCompositionMarker(
  markerId: string,
  patch: Partial<{ tick: number; label: string; color: string | null }>,
): void {
  const marker = compositionMarkerList().find((candidate) => candidate.id === markerId)
  if (!marker) throw new Error(`Marker ${markerId} не найден`)
  const color = patch.color === null ? undefined : patch.color ?? marker.color
  commitDocument(upsertCompositionMarker(compositionState.document, {
    ...marker,
    tick: patch.tick === undefined ? marker.tick : Math.round(patch.tick),
    label: patch.label ?? marker.label,
    ...(color === undefined ? { color: undefined } : { color }),
  }))
  compositionState.ui.selectedMarkerId = markerId
}


export function removeCompositionMarker(markerId: string): void {
  commitDocument(deleteCompositionMarker(compositionState.document, markerId))
  if (compositionState.ui.selectedMarkerId === markerId) compositionState.ui.selectedMarkerId = null
}


export function seekCompositionMarker(markerId: string): void {
  const marker = compositionMarkerList().find((candidate) => candidate.id === markerId)
  if (!marker) throw new Error(`Marker ${markerId} не найден`)
  compositionState.transport.playheadTicks = marker.tick
  compositionState.transport.playing = false
  compositionState.ui.selectedMarkerId = markerId
}

