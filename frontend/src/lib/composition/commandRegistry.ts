import { insertFreezeReason } from './freezeInsert.js'
import { mergeClipsReason } from './mergeClips.js'
import { splitAllReason } from './splitAll.js'
import { canvasTimeGrid, gridBoundaryTicks, stepTimeGridTicks } from './timeGrid'
import { agentSession } from '../agent/session.svelte.js'
import { compositionLinkToggleState } from './links.js'
import { clipEndTicks, COMPOSITION_TIME_BASE } from './types.js'
import type { ShortcutCommandId } from '$lib/shortcuts.js'
import {
  compositionState,
  copyCompositionSelection,
  pasteCompositionClipboard,
  pasteInsertCompositionClipboard,
  deleteSelectedCompositionClip,
  duplicateSelectedCompositionClip,
  moveCompositionClip,
  moveCompositionKeyframeSelection,
  redoComposition,
  reorderCompositionTrack,
  selectedCompositionClip,
  selectedCompositionTrack,
  setCompositionPlayhead,
  splitSelectedCompositionClip,
  splitAllCompositionClips,
  mergeSelectedCompositionClips,
  insertCompositionFreezeFrame,
  toggleCompositionPlayback,
  toggleCompositionSnapping,
  toggleCompositionLinkOverride,
  toggleSelectedCompositionLink,
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
  'composition.splitAll',
  'composition.merge',
  'composition.freezeInsert',
  'composition.pasteInsert',
  'composition.duplicate',
  'composition.delete',
  'composition.linkSelection',
] as const satisfies readonly CompositionCommandId[]

export function compositionCommandState(command: CompositionCommandId): CompositionCommandState {
  const selectedClip = selectedCompositionClip()
  const selectedTrack = selectedCompositionTrack()
  if (['composition.delete', 'composition.nudgePrevious', 'composition.nudgeNext'].includes(command) && compositionState.ui.selectedKeyframes.length) {
    const editable = compositionState.ui.selectedKeyframes.every((subject) => compositionState.document.tracks.some((track) => !track.locked && track.clips.some((clip) => clip.id === subject.target.clipId && !clip.locked)))
    return availability(editable, 'Ключ или дорожка заблокированы')
  }
  if (['composition.delete', 'composition.duplicate', 'composition.nudgePrevious', 'composition.nudgeNext'].includes(command) && compositionState.document.tracks.some((track) => track.clips.some((clip) => (track.locked || clip.locked) && compositionState.ui.selectedClipIds.includes(clip.id)))) return availability(false, 'Дорожка выбранного участника заблокирована')
  if (command === 'composition.copy') return availability(compositionState.ui.selectedColorKeys.length > 0 || compositionState.ui.selectedKeyframes.length > 0 || !!selectedClip, 'Сначала выберите клип или ключевые кадры')
  if (command === 'composition.pasteInsert') {
    const slot = compositionState.ui.clipboard
    if (slot?.kind !== 'layer') return availability(false, 'Скопируйте клипы для вставки со сдвигом')
    const snapshot = slot.insertSnapshot ?? { document: compositionState.document, clipIds: [slot.clipId] }
    const targets = snapshot.clipIds.flatMap(id => {
      const source = snapshot.document.tracks.find(track => track.clips.some(clip => clip.id === id))
      if (!source) return []
      const ordinal = snapshot.document.tracks.filter(track => track.kind === source.kind).findIndex(track => track.id === source.id)
      const tracks = compositionState.document.tracks.filter(track => track.kind === source.kind)
      const target = tracks[ordinal] ?? tracks.at(-1)
      return target ? [target] : []
    })
    return availability(targets.length > 0 && targets.every(track => !track.locked), 'Нет доступной целевой дорожки')
  }
  if (command === 'composition.paste') {
    const slot = compositionState.ui.clipboard
    if (!slot) return availability(false, 'Буфер обмена пуст')
    if (slot.kind === 'layer') {
      const track = compositionState.document.tracks.find((candidate) => candidate.clips.some((clip) => clip.id === slot.clipId))
      const clip = track?.clips.find((candidate) => candidate.id === slot.clipId)
      return availability(!!clip, 'Исходный клип отсутствует')
    }
    const ids = slot.kind === 'color-keyframes' && compositionState.ui.selectedColorKeys.length ? [...new Set(compositionState.ui.selectedColorKeys.map(subject => subject.clipId))] : compositionState.ui.selectedClipIds.length ? compositionState.ui.selectedClipIds : selectedClip ? [selectedClip.id] : []
    if (slot.kind === 'color-keyframes' && (ids.length !== slot.groups.length || !ids.every(id => compositionState.document.tracks.some(track => (track.kind === 'text' || track.kind === 'color') && track.clips.some(clip => clip.id === id))))) return availability(false, 'Выберите текстовые или цветовые слои по числу групп цвета')
    if (!ids.length) return availability(false, 'Сначала выберите клип')
    return availability(!compositionState.document.tracks.some((track) => track.clips.some((clip) => ids.includes(clip.id) && (track.locked || clip.locked))), 'Клип или дорожка заблокированы')
  }
  switch (command) {
    case 'composition.linkSelection': {
      const ids = compositionState.ui.selectedClipIds.length ? compositionState.ui.selectedClipIds : selectedClip ? [selectedClip.id] : []
      const status = compositionLinkToggleState(compositionState.document, ids)
      return availability(status.mode !== 'unavailable', status.reason)
    }
    case 'composition.undo':
      if (agentSession.lock && agentSession.lock.expiresAt > Date.now()) return availability(false, `История занята: ${agentSession.lock.reason}`)
      return availability(compositionState.history.past.length > 0, 'Нет действий для отмены')
    case 'composition.redo':
      if (agentSession.lock && agentSession.lock.expiresAt > Date.now()) return availability(false, `История занята: ${agentSession.lock.reason}`)
      return availability(compositionState.history.future.length > 0, 'Нет действий для повтора')
    case 'composition.freezeInsert': {
      const reason = insertFreezeReason(compositionState.document, compositionState.ui.selectedClipId, compositionState.transport.playheadTicks)
      return availability(reason === null, reason ?? '')
    }
    case 'composition.merge': {
      const reason = mergeClipsReason(compositionState.document, compositionState.ui.selectedClipIds)
      return availability(reason === null, reason ?? '')
    }
    case 'composition.splitAll': {
      const reason = splitAllReason(compositionState.document, compositionState.transport.playheadTicks)
      return availability(reason === null, reason ?? '')
    }
    case 'composition.split':
    case 'composition.duplicate':
    case 'composition.delete':
    case 'composition.nudgePrevious':
    case 'composition.nudgeNext':
    case 'composition.trimStart':
    case 'composition.trimEnd':
      if (!selectedClip || !selectedTrack) return availability(false, 'Сначала выберите клип')
      return availability(!selectedTrack.locked && !selectedClip.locked, 'Клип или дорожка заблокированы')
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
  const rate = canvasTimeGrid(compositionState.document.canvas)
  switch (command) {
    case 'composition.playPause': return run(toggleCompositionPlayback)
    case 'composition.framePrevious': return run(() => setCompositionPlayhead(stepTimeGridTicks(compositionState.transport.playheadTicks, -1, rate)))
    case 'composition.frameNext': return run(() => setCompositionPlayhead(stepTimeGridTicks(compositionState.transport.playheadTicks, 1, rate)))
    case 'composition.seekPrevious': return run(() => setCompositionPlayhead(compositionState.transport.playheadTicks - COMPOSITION_TIME_BASE / 10))
    case 'composition.seekNext': return run(() => setCompositionPlayhead(compositionState.transport.playheadTicks + COMPOSITION_TIME_BASE / 10))
    case 'composition.seekPreviousLarge': return run(() => setCompositionPlayhead(compositionState.transport.playheadTicks - 5 * COMPOSITION_TIME_BASE))
    case 'composition.seekNextLarge': return run(() => setCompositionPlayhead(compositionState.transport.playheadTicks + 5 * COMPOSITION_TIME_BASE))
    case 'composition.undo': return run(undoComposition)
    case 'composition.redo': return run(redoComposition)
    case 'composition.freezeInsert': return run(insertCompositionFreezeFrame)
    case 'composition.merge': return run(mergeSelectedCompositionClips)
    case 'composition.splitAll': return run(splitAllCompositionClips)
    case 'composition.split': return run(splitSelectedCompositionClip)
    case 'composition.copy': return run(copyCompositionSelection)
    case 'composition.paste': return run(pasteCompositionClipboard)
    case 'composition.pasteInsert': return run(pasteInsertCompositionClipboard)
    case 'composition.duplicate': return run(duplicateSelectedCompositionClip)
    case 'composition.delete': return run(deleteSelectedCompositionClip)
    case 'composition.toggleSnapping': return run(toggleCompositionSnapping)
    case 'composition.toggleLinks': return run(toggleCompositionLinkOverride)
    case 'composition.linkSelection': return run(toggleSelectedCompositionLink)
    case 'composition.nudgePrevious': return nudgeClip(-1)
    case 'composition.nudgeNext': return nudgeClip(1)
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

function nudgeClip(direction: -1 | 1): boolean {
  if (compositionState.ui.selectedKeyframes.length) return run(() => {
    compositionState.ui.selectedKeyframes = [...moveCompositionKeyframeSelection(compositionState.ui.selectedKeyframes, direction * gridBoundaryTicks(1, canvasTimeGrid(compositionState.document.canvas)))]
  })
  const clip = selectedCompositionClip()!
  const track = selectedCompositionTrack()!
  return run(() => moveCompositionClip(clip.id, track.id, stepTimeGridTicks(clip.timelineStartTicks, direction, canvasTimeGrid(compositionState.document.canvas)), false))
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
