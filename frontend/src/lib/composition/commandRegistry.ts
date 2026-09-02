import { clipEndTicks, COMPOSITION_TIME_BASE } from './types.js'
import type { ShortcutCommandId } from '$lib/shortcuts.js'
import {
  compositionState,
  deleteSelectedCompositionClip,
  duplicateSelectedCompositionClip,
  moveCompositionClip,
  redoComposition,
  reorderCompositionTrack,
  selectedCompositionClip,
  selectedCompositionTrack,
  setCompositionPlayhead,
  splitSelectedCompositionClip,
  toggleCompositionPlayback,
  toggleCompositionSnapping,
  trimCompositionClip,
  undoComposition,
} from '$lib/state/composition.svelte.js'

export type CompositionCommandId = Extract<ShortcutCommandId, `composition.${string}`>

export interface CompositionCommandState {
  enabled: boolean
  disabledReason?: string
}

export const COMPOSITION_TOOLBAR_COMMANDS = [
  'composition.undo',
  'composition.redo',
  'composition.split',
  'composition.duplicate',
  'composition.delete',
] as const satisfies readonly CompositionCommandId[]

export function compositionCommandState(command: CompositionCommandId): CompositionCommandState {
  const selectedClip = selectedCompositionClip()
  const selectedTrack = selectedCompositionTrack()
  switch (command) {
    case 'composition.undo':
      return availability(compositionState.history.past.length > 0, 'Нет действий для отмены')
    case 'composition.redo':
      return availability(compositionState.history.future.length > 0, 'Нет действий для повтора')
    case 'composition.split':
    case 'composition.duplicate':
    case 'composition.delete':
    case 'composition.nudgePrevious':
    case 'composition.nudgeNext':
    case 'composition.trimStart':
    case 'composition.trimEnd':
      if (!selectedClip || !selectedTrack) return availability(false, 'Сначала выберите клип')
      return availability(!selectedTrack.locked, 'Дорожка заблокирована')
    case 'composition.trackUp':
    case 'composition.trackDown': {
      if (!selectedTrack) return availability(false, 'Сначала выберите дорожку')
      const index = compositionState.document.tracks.findIndex((track) => track.id === selectedTrack.id)
      const direction = command === 'composition.trackUp' ? -1 : 1
      return availability(index + direction >= 0 && index + direction < compositionState.document.tracks.length, 'Дорожка уже у границы')
    }
    default:
      return availability(true)
  }
}

/** One execution boundary for toolbar, menus, keyboard shortcuts and palette. */
export function executeCompositionCommand(command: CompositionCommandId): boolean {
  const status = compositionCommandState(command)
  if (!status.enabled) return false
  const frameTicks = compositionFrameTicks()
  switch (command) {
    case 'composition.playPause': return run(toggleCompositionPlayback)
    case 'composition.framePrevious': return run(() => setCompositionPlayhead(compositionState.transport.playheadTicks - frameTicks))
    case 'composition.frameNext': return run(() => setCompositionPlayhead(compositionState.transport.playheadTicks + frameTicks))
    case 'composition.seekPrevious': return run(() => setCompositionPlayhead(compositionState.transport.playheadTicks - COMPOSITION_TIME_BASE / 10))
    case 'composition.seekNext': return run(() => setCompositionPlayhead(compositionState.transport.playheadTicks + COMPOSITION_TIME_BASE / 10))
    case 'composition.seekPreviousLarge': return run(() => setCompositionPlayhead(compositionState.transport.playheadTicks - 5 * COMPOSITION_TIME_BASE))
    case 'composition.seekNextLarge': return run(() => setCompositionPlayhead(compositionState.transport.playheadTicks + 5 * COMPOSITION_TIME_BASE))
    case 'composition.undo': return run(undoComposition)
    case 'composition.redo': return run(redoComposition)
    case 'composition.split': return run(splitSelectedCompositionClip)
    case 'composition.duplicate': return run(duplicateSelectedCompositionClip)
    case 'composition.delete': return run(deleteSelectedCompositionClip)
    case 'composition.toggleSnapping': return run(toggleCompositionSnapping)
    case 'composition.nudgePrevious': return nudgeClip(-frameTicks)
    case 'composition.nudgeNext': return nudgeClip(frameTicks)
    case 'composition.trimStart': return trimAtPlayhead('start')
    case 'composition.trimEnd': return trimAtPlayhead('end')
    case 'composition.trackUp': return reorderSelectedTrack(-1)
    case 'composition.trackDown': return reorderSelectedTrack(1)
  }
}

function availability(enabled: boolean, disabledReason?: string): CompositionCommandState {
  return enabled ? { enabled: true } : { enabled: false, disabledReason }
}

function run(action: () => void): boolean {
  try {
    compositionState.ui.message = ''
    action()
  } catch (error) {
    compositionState.ui.message = error instanceof Error ? error.message : String(error)
  }
  return true
}

function compositionFrameTicks(): number {
  const fps = compositionState.document.canvas.fps
  return Math.max(1, Math.round(COMPOSITION_TIME_BASE / (Number.isFinite(fps) && fps > 0 ? fps : 30)))
}

function nudgeClip(deltaTicks: number): boolean {
  const clip = selectedCompositionClip()!
  const track = selectedCompositionTrack()!
  return run(() => moveCompositionClip(clip.id, track.id, Math.max(0, clip.timelineStartTicks + deltaTicks), false))
}

function trimAtPlayhead(edge: 'start' | 'end'): boolean {
  const clip = selectedCompositionClip()!
  return run(() => trimCompositionClip(
    clip.id,
    edge === 'start' ? compositionState.transport.playheadTicks : clip.timelineStartTicks,
    edge === 'end' ? compositionState.transport.playheadTicks : clipEndTicks(clip),
  ))
}

function reorderSelectedTrack(direction: -1 | 1): boolean {
  const track = selectedCompositionTrack()!
  const index = compositionState.document.tracks.findIndex((candidate) => candidate.id === track.id)
  return run(() => reorderCompositionTrack(track.id, index + direction))
}
