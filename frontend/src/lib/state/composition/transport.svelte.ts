import {
  findClipLocation
} from '../../composition/commands.js'
import {
  COMPOSITION_TIME_BASE
} from '../../composition/types.js'
import type {
  CompositionClip
} from '../../composition/types.js'
import {
  clampTick,
  cloneComposition,
  cloneCompositionMedia,
  compositionDuration,
  compositionRenderOutput,
  compositionState,
  repairSelection,
  scheduleAutosave,
  stopAtDuration
} from './core.svelte.js'

export function selectedCompositionClip(): CompositionClip | null {
  const id = compositionState.ui.selectedClipId
  if (!id) return null
  try {
    return findClipLocation(compositionState.document, id).clip
  } catch {
    return null
  }
}


export function setCompositionPlayhead(ticks: number): void {
  const duration = compositionDuration()
  compositionState.transport.playheadTicks = clampTick(ticks, Math.max(duration, 0))
  if (duration > 0 && compositionState.transport.playheadTicks >= duration) {
    compositionState.transport.playing = false
  }
}


export function seekComposition(seconds: number): void {
  setCompositionPlayhead(compositionState.transport.playheadTicks + seconds * COMPOSITION_TIME_BASE)
}


export function toggleCompositionPlayback(): void {
  const duration = compositionDuration()
  if (!duration) return
  if (!compositionState.transport.playing && compositionState.transport.playheadTicks >= duration) {
    compositionState.transport.playheadTicks = 0
  }
  compositionState.transport.playing = !compositionState.transport.playing
}


export function setCompositionZoom(pxPerSecond: number): void {
  compositionState.ui.zoomPxPerSecond = Math.max(24, Math.min(320, Math.round(pxPerSecond)))
}


export function toggleCompositionSnapping(): void {
  compositionState.ui.snapEnabled = !compositionState.ui.snapEnabled
}


export function undoComposition(): void {
  const previous = compositionState.history.past.pop()
  if (!previous) return
  const previousMedia = compositionState.history.pastMedia.pop() ?? cloneCompositionMedia(compositionState.media)
  const previousOutput = compositionState.history.pastOutput.pop() ?? compositionRenderOutput()
  compositionState.history.future.push(cloneComposition(compositionState.document))
  compositionState.history.futureMedia.push(cloneCompositionMedia(compositionState.media))
  compositionState.history.futureOutput.push(compositionRenderOutput())
  compositionState.document = cloneComposition(previous)
  compositionState.media = previousMedia
  compositionState.export.profile = { ...previousOutput.profile }
  compositionState.export.qualityTier = previousOutput.qualityTier
  compositionState.export.videoBitrateKbps = previousOutput.videoBitrateKbps ?? null
  repairSelection()
  stopAtDuration()
  scheduleAutosave()
}


export function redoComposition(): void {
  const next = compositionState.history.future.pop()
  if (!next) return
  const nextMedia = compositionState.history.futureMedia.pop() ?? cloneCompositionMedia(compositionState.media)
  const nextOutput = compositionState.history.futureOutput.pop() ?? compositionRenderOutput()
  compositionState.history.past.push(cloneComposition(compositionState.document))
  compositionState.history.pastMedia.push(cloneCompositionMedia(compositionState.media))
  compositionState.history.pastOutput.push(compositionRenderOutput())
  compositionState.document = cloneComposition(next)
  compositionState.media = nextMedia
  compositionState.export.profile = { ...nextOutput.profile }
  compositionState.export.qualityTier = nextOutput.qualityTier
  compositionState.export.videoBitrateKbps = nextOutput.videoBitrateKbps ?? null
  repairSelection()
  stopAtDuration()
  scheduleAutosave()
}

